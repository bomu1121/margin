//! Experimental agentic trace runner.
//!
//! `trace.rs` is a fixed pipeline: extract claims -> search -> fetch -> verify.
//! This module is the opposite experiment: the model owns the per-claim
//! decisions (which claims matter, which queries, which sources, when it has
//! enough evidence) through native tool calls.
//!
//! It is deliberately isolated from `note_engine.rs`: own system prompt, own
//! tools, own output contract (facts, never note.md). The only shared pieces
//! are the fact store, search/fetch clients and the job/usage plumbing.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::checkpoint::{self, CostModel};
use super::clients::{self, AiConfig, ChatMessage, SearchConfig, ToolSpec};
use super::facts::{FactEvidenceInput, FactStore};
use super::web_fetch;
use super::{TraceItem, Usage};

const MAX_NOTE_CHARS: usize = 12_000;
const MAX_OBSERVATION_CHARS: usize = 4_000;
const MAX_QUOTE_CHARS: usize = 600;
const FETCH_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct TraceAgentOptions {
    pub max_rounds: usize,
    pub max_claims: usize,
    pub max_searches: usize,
    pub max_fetches: usize,
}

impl Default for TraceAgentOptions {
    fn default() -> Self {
        Self { max_rounds: 14, max_claims: 8, max_searches: 10, max_fetches: 6 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TraceAgentTurn {
    pub index: usize,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default)]
    pub observation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub model_ms: u64,
    #[serde(default)]
    pub tool_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TraceAgentReport {
    pub status: String,
    pub stop_reason: String,
    pub summary: String,
    pub claims: Vec<TraceItem>,
    #[serde(default)]
    pub inventory: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
    pub turns: Vec<TraceAgentTurn>,
    pub usage: Usage,
    pub search_count: usize,
    pub fetch_count: usize,
    pub duration_ms: u64,
}

#[derive(Default)]
struct AgentState {
    /// URL -> search snippets and/or fetched page text, used for quote checks.
    evidence: HashMap<String, String>,
    titles: HashMap<String, String>,
    queries: Vec<String>,
    searches: usize,
    fetches: usize,
    fetched: HashSet<String>,
    reported: Vec<TraceItem>,
    reported_keys: HashSet<String>,
    expected_claims: Option<HashSet<String>>,
    ignore_facts: bool,
    summary: String,
    finished: bool,
}

fn system_prompt(options: &TraceAgentOptions) -> String {
    format!(
        "你是「页边笔记」的溯源研究代理。目标是查清一篇已经写好的笔记里可外部核查的主张，把判定和证据写入事实库。\n\n\
你不改写笔记、不生成新笔记、不编造来源。\n\n\
先确定主语：这篇笔记在讲哪个项目 / 事件 / 人物？正文、标题、frontmatter 里有没有项目名、来源链接、UP 主？\n\
claim 必须保留原句的主语和限定词；不要把「这个项目支持四平台」改写成「这类项目支持四平台」，也不要把具体数字改成类别判断。\n\
如果主语无法定位：具体数字、平台、功能、版本一律只能报「未找到外部出处」，并在 finish summary 里写「主语无法定位」。\n\
只有来源明确在讲同一个主语时，才能报「外部可印证」；仅凭搜索摘要不能报，必须先 fetch_source 读正文。\n\n\
可用工具：\n\
- find_facts：先查事实库；如果有同一事实，记下 fact_id。\n\
- search_web：提交一个检索词；返回标题、URL、摘要。\n\
- fetch_source：读取一个候选来源的正文；用于核对数字、日期、机构名。\n\
- report_claim：提交一条主张的判定和证据。source_url 必须是本轮 search_web 或 fetch_source 见过的 URL；quote 必须是该来源摘要/正文里的连续原文，不能改写。\n\
- finish：结束，并写一句总结。\n\n\
判定值只能是：外部可印证 / 存在争议 / 未找到外部出处 / 仅非权威来源。\n\
relation 只能是：supports / contradicts / context。\n\
外部可印证 → relation 必须是 supports；存在争议 → relation 必须是 contradicts，并把矛盾的那句放进 quote；仅非权威来源 / 未找到 → context。\n\n\
优先处理：数字、日期、机构、人物身份、事件因果。主观评价和泛泛感慨跳过。\n\
每条主张最多 2 次 search_web、1 次 fetch_source。\n\
本次预算：最多报告 {} 条主张、{} 次 search_web、{} 次 fetch_source、{} 回合。\n\
证据不足时直接 report_claim(verdict=\"未找到外部出处\")，不要编造，也不要把“没搜到”写成“存在争议”。\n\
完成后必须调用 finish。",
        options.max_claims, options.max_searches, options.max_fetches, options.max_rounds
    )
}

fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "find_facts".into(),
            description: "在事实库里找与这条主张相近的已有事实。命中时把 fact_id 传给 report_claim，避免新建重复事实。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "claim": {"type": "string", "description": "要查的原子主张"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 12}
                },
                "required": ["claim"]
            }),
        },
        ToolSpec {
            name: "search_web".into(),
            description: "用联网检索查一个主张。返回标题、URL 和摘要；摘要可以直接作为 quote 的来源。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 6}
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "fetch_source".into(),
            description: "抓取一个公开网页的正文，用于核对摘要里没有的数字/日期/机构名。只读，不跟随站内导航。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"]
            }),
        },
        ToolSpec {
            name: "report_claim".into(),
            description: "提交一条主张的判定与证据。claim 必须保留笔记原句的主语和限定词；source_url/quote 会被程序校验；报「外部可印证 / 存在争议」前必须先 fetch_source 读正文。校验失败会返回错误，必须换证据重试。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "claim": {"type": "string", "description": "原子主张，不要合并多个断言"},
                    "verdict": {"type": "string", "enum": ["外部可印证", "存在争议", "未找到外部出处", "仅非权威来源"]},
                    "relation": {"type": "string", "enum": ["supports", "contradicts", "context"]},
                    "source_url": {"type": "string"},
                    "quote": {"type": "string", "description": "来源里的连续原文，逐字复制"},
                    "fact_id": {"type": "string", "description": "find_facts 命中的已有事实 id；新事实留空"},
                    "queries": {"type": "array", "items": {"type": "string"}, "description": "本条主张用过的检索词"}
                },
                "required": ["claim", "verdict"]
            }),
        },
        ToolSpec {
            name: "finish".into(),
            description: "结束本次溯源；summary 写一句结论（查了几条、几条有证据、哪些没找到）。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"summary": {"type": "string"}},
                "required": ["summary"]
            }),
        },
    ]
}

pub async fn run(
    facts: &FactStore,
    note_id: &str,
    run_id: &str,
    title: &str,
    body: &str,
    ai: &AiConfig,
    search: &SearchConfig,
    options: &TraceAgentOptions,
    fixed_claims: Option<&[String]>,
    ignore_facts: bool,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<TraceAgentReport, String> {
    let started = Instant::now();
    let note: String = body.chars().take(MAX_NOTE_CHARS).collect();
    let mut state = AgentState { ignore_facts, ..AgentState::default() };
    let inventory: Vec<String> = fixed_claims
        .map(|items| items.iter().map(|item| item.trim().to_string()).filter(|item| !item.is_empty()).collect())
        .unwrap_or_default();
    if !inventory.is_empty() {
        state.expected_claims = Some(inventory.iter().map(|item| normalize_claim(item)).collect());
    }
    let mut turns: Vec<TraceAgentTurn> = Vec::new();
    let mut total = Usage::default();
    let system = if inventory.is_empty() {
        system_prompt(options)
    } else {
        format!(
            "{}\n\n本次只核查下面 {} 条主张：claim 必须逐字使用清单里的句子，不要新增、不要改写；每条都必须 report_claim，找不到就报「未找到外部出处」。\n{}",
            system_prompt(options),
            inventory.len(),
            inventory.iter().enumerate().map(|(index, claim)| format!("{}. {}", index + 1, claim)).collect::<Vec<_>>().join("\n")
        )
    };
    let mut messages = vec![
        ChatMessage::system(system),
        ChatMessage::user(format!(
            "笔记标题：{}\n\n正文：\n{}\n\n请开始；先找出可核查的原子主张，再逐条查。",
            title.trim(),
            note
        )),
    ];
    let tools = tool_specs();
    let mut status = "success".to_string();
    let mut stop_reason = "finish".to_string();
    let mut rounds = 0usize;

    for round in 0..options.max_rounds {
        if token.is_cancelled() {
            status = "cancelled".into();
            stop_reason = "cancelled".into();
            break;
        }
        rounds = round + 1;
        let model_start = Instant::now();
        let response = clients::chat_tools(ai, &messages, &tools, None, None, token)
            .await
            .map_err(|error| format!("第 {} 回合模型失败：{error}", rounds))?;
        let model_ms = model_start.elapsed().as_millis() as u64;
        checkpoint::add_usage(&mut total, &usage_from_tool(&response));
        turns.push(TraceAgentTurn {
            index: turns.len(),
            phase: "model".into(),
            tool: None,
            args: None,
            observation: String::new(),
            content: if response.content.trim().is_empty() { None } else { Some(response.content.clone()) },
            reasoning: response.reasoning_content.clone(),
            model_ms,
            tool_ms: 0,
        });

        if response.tool_calls.is_empty() {
            if !response.content.trim().is_empty() {
                state.summary = response.content.trim().chars().take(400).collect();
            }
            stop_reason = "model_stopped".into();
            break;
        }

        let calls = response.tool_calls.clone();
        messages.push(ChatMessage::assistant(
            if response.content.trim().is_empty() { None } else { Some(response.content.clone()) },
            calls.clone(),
        ));
        for call in calls {
            if token.is_cancelled() {
                status = "cancelled".into();
                stop_reason = "cancelled".into();
                break;
            }
            let tool_start = Instant::now();
            let (observation, args) =
                execute_tool(facts, note_id, run_id, ai, search, options, &mut state, &call, token).await;
            let tool_ms = tool_start.elapsed().as_millis() as u64;
            messages.push(ChatMessage::tool(call.id.clone(), observation.clone()));
            turns.push(TraceAgentTurn {
                index: turns.len(),
                phase: "tool".into(),
                tool: Some(call.function.name.clone()),
                args,
                observation: observation.chars().take(MAX_OBSERVATION_CHARS).collect(),
                content: None,
                reasoning: None,
                model_ms: 0,
                tool_ms,
            });
            if state.finished {
                break;
            }
        }
        if state.finished {
            stop_reason = "finish".into();
            break;
        }
        if round + 1 == options.max_rounds {
            status = "max_rounds".into();
            stop_reason = "max_rounds".into();
        }
    }

    if state.finished && state.reported.is_empty() {
        stop_reason = "no_claims".into();
    }
    let missing = if inventory.is_empty() {
        Vec::new()
    } else {
        inventory
            .iter()
            .filter(|claim| !state.reported_keys.contains(&normalize_claim(claim)))
            .cloned()
            .collect::<Vec<_>>()
    };
    if state.summary.trim().is_empty() {
        state.summary = if state.reported.is_empty() {
            "没有报告可核查主张".into()
        } else {
            format!("报告 {} 条主张", state.reported.len())
        };
    }
    cost.apply(&mut total);
    checkpoint::add_usage(usage, &total);
    usage.estimated_cost += total.estimated_cost;

    Ok(TraceAgentReport {
        status,
        stop_reason,
        summary: state.summary,
        claims: state.reported,
        inventory,
        missing,
        turns,
        usage: total,
        search_count: state.searches,
        fetch_count: state.fetches,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

async fn execute_tool(
    facts: &FactStore,
    note_id: &str,
    run_id: &str,
    ai: &AiConfig,
    search: &SearchConfig,
    options: &TraceAgentOptions,
    state: &mut AgentState,
    call: &clients::ToolCall,
    token: &CancellationToken,
) -> (String, Option<Value>) {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
    let result = match call.function.name.as_str() {
        "find_facts" if state.ignore_facts => Ok(json!({"candidates": [], "ignored": "本次对照运行忽略已有事实"})),
        "find_facts" => tool_find_facts(facts, &args),
        "search_web" => tool_search(search, options, state, &args, token).await,
        "fetch_source" => tool_fetch(options, state, &args).await,
        "report_claim" => tool_report_claim(facts, note_id, run_id, ai, search, options, state, &args),
        "finish" => {
            state.summary = text_arg(&args, "summary").trim().chars().take(400).collect();
            state.finished = true;
            Ok(json!({"ok": true}))
        }
        other => Err(format!("未知工具：{other}")),
    };
    let observation = match result {
        Ok(value) => value.to_string(),
        Err(error) => json!({"error": error}).to_string(),
    };
    (observation, if args.is_null() { None } else { Some(args) })
}

fn tool_find_facts(facts: &FactStore, args: &Value) -> Result<Value, String> {
    let claim = text_arg(args, "claim");
    if claim.trim().is_empty() {
        return Err("claim 不能为空".into());
    }
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5).clamp(1, 12) as usize;
    let candidates = facts.candidate_facts_for_claim(&claim, limit)?;
    Ok(json!({"candidates": candidates}))
}

async fn tool_search(
    search: &SearchConfig,
    options: &TraceAgentOptions,
    state: &mut AgentState,
    args: &Value,
    token: &CancellationToken,
) -> Result<Value, String> {
    if state.searches >= options.max_searches {
        return Err(format!("search_web 预算已用完（{} 次）", options.max_searches));
    }
    let query = text_arg(args, "query");
    if query.trim().is_empty() {
        return Err("query 不能为空".into());
    }
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(4).clamp(1, 6) as u32;
    state.searches += 1;
    state.queries.push(query.clone());
    let hits = clients::search(search, &query, limit, token).await?;
    let mut results = Vec::new();
    for hit in &hits {
        if hit.url.trim().is_empty() {
            continue;
        }
        state
            .evidence
            .entry(hit.url.clone())
            .or_default()
            .push_str(&format!("\n{}\n", hit.content));
        if !hit.title.trim().is_empty() {
            state.titles.insert(hit.url.clone(), hit.title.clone());
        }
        results.push(json!({
            "title": hit.title,
            "url": hit.url,
            "snippet": hit.content.chars().take(500).collect::<String>(),
        }));
    }
    Ok(json!({"query": query, "results": results}))
}

async fn tool_fetch(
    options: &TraceAgentOptions,
    state: &mut AgentState,
    args: &Value,
) -> Result<Value, String> {
    if state.fetches >= options.max_fetches {
        return Err(format!("fetch_source 预算已用完（{} 次）", options.max_fetches));
    }
    let url = text_arg(args, "url");
    if url.trim().is_empty() {
        return Err("url 不能为空".into());
    }
    state.fetches += 1;
    let page = tokio::time::timeout(FETCH_TIMEOUT, web_fetch::fetch(&url))
        .await
        .map_err(|_| "抓取来源超时".to_string())?
        .map_err(|error| error.to_string())?;
    state.fetched.insert(page.url.clone());
    state.fetched.insert(url.clone());
    state
        .evidence
        .entry(page.url.clone())
        .or_default()
        .push_str(&format!("\n{}\n", page.text));
    if page.url != url {
        state
            .evidence
            .entry(url.clone())
            .or_default()
            .push_str(&format!("\n{}\n", page.text));
    }
    if !page.title.trim().is_empty() {
        state.titles.insert(page.url.clone(), page.title.clone());
    }
    Ok(json!({
        "url": page.url,
        "title": page.title,
        "text": page.text.chars().take(5_000).collect::<String>(),
    }))
}

fn tool_report_claim(
    facts: &FactStore,
    note_id: &str,
    run_id: &str,
    ai: &AiConfig,
    search: &SearchConfig,
    options: &TraceAgentOptions,
    state: &mut AgentState,
    args: &Value,
) -> Result<Value, String> {
    if state.reported.len() >= options.max_claims {
        return Err(format!("report_claim 预算已用完（{} 条）", options.max_claims));
    }
    let claim = text_arg(args, "claim").trim().to_string();
    if claim.is_empty() {
        return Err("claim 不能为空".into());
    }
    let key = normalize_claim(&claim);
    if let Some(expected) = &state.expected_claims {
        if !expected.contains(&key) {
            return Err("claim 必须逐字使用本次清单里的主张；不要改写或新增".into());
        }
    }
    if state.reported_keys.contains(&key) {
        return Err("这条主张本轮已经报过；不要重复 report_claim".into());
    }
    let verdict = normalize_verdict(&text_arg(args, "verdict"))?;
    let relation = normalize_relation(&text_arg(args, "relation"))?;
    check_verdict_relation(&verdict, &relation)?;
    let source_url = text_arg(args, "source_url").trim().to_string();
    let quote = text_arg(args, "quote").trim().chars().take(MAX_QUOTE_CHARS).collect::<String>();
    let fact_id_arg = text_arg(args, "fact_id").trim().to_string();
    let queries: Vec<String> = args
        .get("queries")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    let queries = if queries.is_empty() {
        state.queries.iter().rev().take(4).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    } else {
        queries
    };

    let mut evidence = Vec::new();
    if !source_url.is_empty() {
        validate_citation(state, &source_url, &quote)?;
        if matches!(verdict.as_str(), "外部可印证" | "存在争议") && !state.fetched.contains(&source_url) {
            return Err("报「外部可印证 / 存在争议」前必须先 fetch_source 读该来源正文，不能只凭搜索摘要".into());
        }
        evidence.push(FactEvidenceInput {
            source_url: source_url.clone(),
            title: state.titles.get(&source_url).cloned().unwrap_or_default(),
            quote: quote.clone(),
            relation: relation.clone(),
        });
    } else if verdict != "未找到外部出处" {
        return Err("除了「未找到外部出处」，判定必须带 source_url 和 quote".into());
    }

    let fact_id = if fact_id_arg.is_empty() {
        facts.record_candidate_with_domain(note_id, run_id, &claim, &claim, "", &queries, "general")?
    } else {
        facts.record_mention_for_fact_with_domain(&fact_id_arg, note_id, run_id, &claim, &claim, "general")?;
        fact_id_arg
    };
    facts.record_verification(&fact_id, &verdict, &search.provider, &ai.model, &queries, &evidence)?;
    let status = facts.get_fact(&fact_id)?.map(|fact| fact.status).unwrap_or_default();

    state.reported_keys.insert(key);
    state.reported.push(TraceItem {
        claim: claim.clone(),
        verdict: verdict.clone(),
        relation: relation.clone(),
        source: source_url,
        quote,
    });
    Ok(json!({"ok": true, "factId": fact_id, "status": status, "reported": state.reported.len()}))
}

fn validate_citation(state: &AgentState, source_url: &str, quote: &str) -> Result<(), String> {
    let Some(text) = state.evidence.get(source_url) else {
        return Err("source_url 不是本轮 search_web / fetch_source 见过的 URL".into());
    };
    if quote.trim().chars().count() < 4 {
        return Err("quote 太短；请复制来源里完整的一句".into());
    }
    let quote = normalize_ws(quote);
    let text = normalize_ws(text);
    if !text.contains(&quote) {
        return Err("quote 不是该来源的连续原文；请从摘要或正文里逐字复制，不要改写".into());
    }
    Ok(())
}

fn normalize_verdict(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.contains("可印证") {
        return Ok("外部可印证".into());
    }
    if value.contains("争议") || value.contains("矛盾") {
        return Ok("存在争议".into());
    }
    if value.contains("未找到") || value.contains("没有找到") {
        return Ok("未找到外部出处".into());
    }
    if value.contains("非权威") {
        return Ok("仅非权威来源".into());
    }
    Err("verdict 只能是：外部可印证 / 存在争议 / 未找到外部出处 / 仅非权威来源".into())
}

fn normalize_relation(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok("context".into());
    }
    match value {
        "supports" | "contradicts" | "context" => Ok(value.to_string()),
        _ if value.contains("支持") || value.contains("印证") => Ok("supports".into()),
        _ if value.contains("矛盾") || value.contains("反驳") => Ok("contradicts".into()),
        _ if value.contains("背景") || value.contains("提及") => Ok("context".into()),
        _ => Err("relation 只能是 supports / contradicts / context".into()),
    }
}

/// A verdict without the matching relation is the main source of the old
/// "disputed but evidence says supports" mess. Make the agent point at the
/// sentence that supports or contradicts.
fn check_verdict_relation(verdict: &str, relation: &str) -> Result<(), String> {
    if verdict == "外部可印证" && relation != "supports" {
        return Err("报「外部可印证」时 relation 必须是 supports".into());
    }
    if verdict == "存在争议" && relation != "contradicts" {
        return Err("报「存在争议」时 relation 必须是 contradicts，并把矛盾的那句放进 quote".into());
    }
    Ok(())
}

fn normalize_claim(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, '，' | '。' | '；' | '：' | ',' | '.' | ';' | ':' | '！' | '？' | '!' | '?'))
        .collect::<String>()
        .to_lowercase()
}

fn normalize_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join("")
}

fn text_arg(args: &Value, key: &str) -> String {
    args.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn usage_from_tool(result: &clients::ToolChatResult) -> Usage {
    Usage {
        prompt_tokens: result.prompt_tokens,
        uncached_input_tokens: result.uncached_input_tokens,
        completion_tokens: result.completion_tokens,
        cached_tokens: result.cached_tokens,
        cache_write_tokens: result.cache_write_tokens,
        reasoning_tokens: result.reasoning_tokens,
        provider_cost: result.provider_cost.unwrap_or(0.0),
        provider_currency: result.provider_currency.clone().unwrap_or_default(),
        ..Usage::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_labels_map_to_fact_statuses() {
        assert_eq!(normalize_verdict("可印证").unwrap(), "外部可印证");
        assert_eq!(normalize_verdict("存在争议").unwrap(), "存在争议");
        assert_eq!(normalize_verdict("未找到").unwrap(), "未找到外部出处");
        assert_eq!(normalize_verdict("仅非权威来源").unwrap(), "仅非权威来源");
        assert!(normalize_verdict("大概是对的").is_err());
    }

    #[test]
    fn citation_must_be_a_verbatim_substring() {
        let mut state = AgentState::default();
        state
            .evidence
            .insert("https://example.com/a".into(), "2024 年销量为 120 万台，同比增长 18%。".into());
        assert!(validate_citation(&state, "https://example.com/a", "2024 年销量为 120 万台，同比增长 18%。").is_ok());
        assert!(validate_citation(&state, "https://example.com/a", "2024 年销量为 130 万台").is_err());
        assert!(validate_citation(&state, "https://example.com/other", "2024 年销量为 120 万台").is_err());
    }

    #[test]
    fn verdict_and_relation_must_agree() {
        assert!(check_verdict_relation("外部可印证", "supports").is_ok());
        assert!(check_verdict_relation("存在争议", "contradicts").is_ok());
        assert!(check_verdict_relation("外部可印证", "context").is_err());
        assert!(check_verdict_relation("存在争议", "supports").is_err());
        assert!(check_verdict_relation("未找到外部出处", "context").is_ok());
    }

    #[test]
    fn unknown_url_and_short_quote_are_rejected() {
        let state = AgentState::default();
        assert!(validate_citation(&state, "https://example.com", "一句话").is_err());
    }
}
