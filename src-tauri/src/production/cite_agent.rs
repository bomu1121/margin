//! Citation agent: find where a history note's sentences come from.
//!
//! This is the small replacement for the fact/trace pipeline. It writes only
//! `note_citations` (note sentence -> source passage or candidate); it never
//! writes facts, verdicts, merges or note text. The source catalog and the
//! existing candidate queue are read-only inputs.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::checkpoint::{self, CostModel};
use super::clients::{self, AiConfig, ChatMessage, ToolSpec};
use super::history::{HistoryStore, NoteCitation, NoteCitationInput};
use super::Usage;

#[derive(Debug, Clone)]
pub struct CiteAgentOptions {
    pub max_rounds: usize,
    pub max_citations: usize,
}

impl Default for CiteAgentOptions {
    fn default() -> Self {
        Self { max_rounds: 12, max_citations: 6 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CiteTurn {
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
pub struct CiteReport {
    pub status: String,
    pub summary: String,
    pub citations: Vec<NoteCitation>,
    pub turns: Vec<CiteTurn>,
    pub usage: Usage,
    pub duration_ms: u64,
}

#[derive(Default)]
struct CiteState {
    citations: Vec<NoteCitation>,
    summary: String,
    finished: bool,
}

fn system_prompt(options: &CiteAgentOptions) -> String {
    format!(
        "你是「页边笔记」的史料管理员。任务：找出这篇历史笔记里需要给出处的句子，并为每句找一条史料。\n\n\
你只做出处，不判真假、不写事实、不写白话解释、不改笔记。\n\n\
可用工具：\n\
- find_candidates：在已有史料候选里按书名/标题/摘要找候选；候选只有书名和链接，原文待补。\n\
- list_sources：在已入库史料目录里按书名/作者找书。\n\
- read_source：读一本书的原文段落；有段落才能 attach_citation。\n\
- attach_citation：把笔记原句和一段原文绑定；需要有 passage_id。\n\
- attach_candidate：把笔记原句和一条候选（书名+链接）绑定；原文标“待补”。\n\
- finish：结束并写一句总结。\n\n\
规则：\n\
1. note_quote 必须逐字来自笔记，可以是一整句；不要改写。\n\
2. 只处理需要出处的内容：人物、事件、制度、书名、直接引文；主观评价跳过。\n\
3. 优先级：正史/一手 > 考古报告/档案 > 二手；不要把小说、影视、百科当唯一史料。\n\
4. 有原文段落用 attach_citation；只有候选用 attach_candidate，明确留“待补原文”。\n\
5. 不要编造书名、卷页或原文；找不到就 finish 说明。\n\
6. 最多 {} 条出处，{} 回合。完成后必须调用 finish。",
        options.max_citations, options.max_rounds
    )
}

fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "find_candidates".into(),
            description: "在已有史料候选里找书名/标题/摘要匹配的候选；返回候选 id、书名、链接、摘要。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 12}},
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "list_sources".into(),
            description: "在已入库史料目录里找书；返回 source id、书名、作者、卷、链接。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 12}},
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "read_source".into(),
            description: "读一本书的原文段落；返回段落 id、卷/章/页、原文、locator。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"source_id": {"type": "string"}},
                "required": ["source_id"]
            }),
        },
        ToolSpec {
            name: "attach_citation".into(),
            description: "把笔记原句绑定到一段已入库原文；需要 note_quote 和 passage_id。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"note_quote": {"type": "string"}, "passage_id": {"type": "string"}},
                "required": ["note_quote", "passage_id"]
            }),
        },
        ToolSpec {
            name: "attach_candidate".into(),
            description: "把笔记原句绑定到一条候选（书名+链接，原文待补）；优先用 candidate_id。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "note_quote": {"type": "string"},
                    "candidate_id": {"type": "string"},
                    "work_title": {"type": "string"},
                    "title": {"type": "string"},
                    "url": {"type": "string"},
                    "snippet": {"type": "string"},
                    "provider": {"type": "string"}
                },
                "required": ["note_quote"]
            }),
        },
        ToolSpec {
            name: "finish".into(),
            description: "结束；summary 写一句：找到几条出处、哪些待补原文。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"summary": {"type": "string"}},
                "required": ["summary"]
            }),
        },
    ]
}

pub async fn run(
    history: &HistoryStore,
    note_id: &str,
    title: &str,
    body: &str,
    ai: &AiConfig,
    options: &CiteAgentOptions,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<CiteReport, String> {
    let started = Instant::now();
    let existing = history.note_citations(note_id).unwrap_or_default();
    let existing_lines = existing
        .iter()
        .map(|item| format!("- {} → {}（{}）", item.note_quote, item.work_title, item.status))
        .collect::<Vec<_>>()
        .join("\n");
    let existing_text = if existing_lines.is_empty() { "（无）".to_string() } else { existing_lines };
    let mut messages = vec![
        ChatMessage::system(system_prompt(options)),
        ChatMessage::user(format!(
            "笔记标题：{}\n\n正文：\n{}\n\n已有出处：\n{}\n\n请开始；只贴出处，不判真假。",
            title.trim(),
            body.chars().take(12_000).collect::<String>(),
            existing_text
        )),
    ];
    let tools = tool_specs();
    let mut state = CiteState::default();
    let mut turns: Vec<CiteTurn> = Vec::new();
    let mut total = Usage::default();
    let mut status = "success".to_string();

    for round in 0..options.max_rounds {
        if token.is_cancelled() {
            status = "cancelled".into();
            break;
        }
        let model_start = Instant::now();
        let response = clients::chat_tools(ai, &messages, &tools, None, None, token)
            .await
            .map_err(|error| format!("第 {} 回合模型失败：{error}", round + 1))?;
        let model_ms = model_start.elapsed().as_millis() as u64;
        checkpoint::add_usage(&mut total, &usage_from_tool(&response));
        turns.push(CiteTurn {
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
                break;
            }
            let tool_start = Instant::now();
            let (observation, args) = execute_tool(history, note_id, body, options, &mut state, &call);
            let tool_ms = tool_start.elapsed().as_millis() as u64;
            messages.push(ChatMessage::tool(call.id.clone(), observation.clone()));
            turns.push(CiteTurn {
                index: turns.len(),
                phase: "tool".into(),
                tool: Some(call.function.name.clone()),
                args,
                observation: observation.chars().take(4000).collect(),
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
            break;
        }
        if citation_limit_reached(state.citations.len(), options.max_citations) {
            break;
        }
        if round + 1 == options.max_rounds {
            status = "max_rounds".into();
        }
    }

    if state.citations.is_empty() {
        status = if status == "success" { "no_sources".into() } else { status };
    }
    if state.summary.trim().is_empty() {
        state.summary = if state.citations.is_empty() {
            "没有找到可用出处".into()
        } else {
            format!(
                "找到 {} 条出处，其中 {} 条待补原文",
                state.citations.len(),
                state.citations.iter().filter(|item| item.status == "candidate").count()
            )
        };
    }
    cost.apply(&mut total);
    checkpoint::add_usage(usage, &total);
    usage.estimated_cost += total.estimated_cost;

    Ok(CiteReport {
        status,
        summary: state.summary,
        citations: state.citations,
        turns,
        usage: total,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn execute_tool(
    history: &HistoryStore,
    note_id: &str,
    body: &str,
    options: &CiteAgentOptions,
    state: &mut CiteState,
    call: &clients::ToolCall,
) -> (String, Option<Value>) {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
    let result = match call.function.name.as_str() {
        "find_candidates" => tool_find_candidates(history, &args),
        "list_sources" => tool_list_sources(history, &args),
        "read_source" => tool_read_source(history, &args),
        "attach_citation" => tool_attach(history, note_id, body, options, state, &args, true),
        "attach_candidate" => tool_attach(history, note_id, body, options, state, &args, false),
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

/// Common classical works with a Wikisource work page. The candidate queue
/// often only has HathiTrust search pages or library catalogs; a direct work
/// page is the minimum needed to read the original.
fn full_text_url(work: &str) -> Option<String> {
    let key = work.trim().trim_matches(|ch: char| ch == '《' || ch == '》' || ch.is_whitespace());
    let path = match key {
        "三国志" | "三國志" => "三國志",
        "后汉书" | "後漢書" => "後漢書",
        "宋书" | "宋書" => "宋書",
        "魏书" | "魏書" => "魏書",
        "资治通鉴" | "資治通鑑" => "資治通鑑",
        "明史" => "明史",
        "史记" | "史記" => "史記",
        "汉书" | "漢書" => "漢書",
        "晋书" | "晉書" => "晉書",
        "南史" => "南史",
        "北史" => "北史",
        "隋书" | "隋書" => "隋書",
        "旧唐书" | "舊唐書" => "舊唐書",
        "新唐书" | "新唐書" => "新唐書",
        "宋史" => "宋史",
        "元史" => "元史",
        "清史稿" => "清史稿",
        _ => return None,
    };
    Some(format!("https://zh.wikisource.org/wiki/{path}"))
}

fn tool_find_candidates(history: &HistoryStore, args: &Value) -> Result<Value, String> {
    let query = text_arg(args, "query");
    if query.trim().is_empty() {
        return Err("query 不能为空".into());
    }
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(6).clamp(1, 12) as usize;
    let rows = history.search_candidates(&query, limit)?;
    Ok(json!({
        "candidates": rows.iter().map(|item| json!({
            "id": item.id,
            "work_title": if item.work_title.is_empty() { &item.title } else { &item.work_title },
            "title": item.title,
            "url": item.url,
            "snippet": item.snippet.chars().take(200).collect::<String>(),
            "authority": item.authority,
        })).collect::<Vec<_>>()
    }))
}

fn tool_list_sources(history: &HistoryStore, args: &Value) -> Result<Value, String> {
    let query = text_arg(args, "query");
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(6).clamp(1, 12) as usize;
    let rows = history.list_sources(&query, "", limit)?;
    Ok(json!({
        "sources": rows.iter().map(|item| json!({
            "id": item.id,
            "work_title": item.work_title,
            "author": item.author,
            "volume": item.volume,
            "source_type": item.source_type,
            "url": if item.url.is_empty() { &item.scan_url } else { &item.url },
            "passages": item.passage_count,
        })).collect::<Vec<_>>()
    }))
}

fn tool_read_source(history: &HistoryStore, args: &Value) -> Result<Value, String> {
    let source_id = text_arg(args, "source_id");
    if source_id.trim().is_empty() {
        return Err("source_id 不能为空".into());
    }
    let detail = history.get_source(&source_id)?.ok_or_else(|| "来源不存在".to_string())?;
    Ok(json!({
        "work_title": detail.source.work_title,
        "author": detail.source.author,
        "volume": detail.source.volume,
        "url": if detail.source.url.is_empty() { detail.source.scan_url } else { detail.source.url },
        "passages": detail.passages.iter().take(8).map(|item| json!({
            "passage_id": item.id,
            "volume": item.volume,
            "chapter": item.chapter,
            "page": item.page,
            "locator": item.locator,
            "original_text": item.original_text.chars().take(1200).collect::<String>(),
        })).collect::<Vec<_>>()
    }))
}

fn tool_attach(
    history: &HistoryStore,
    note_id: &str,
    body: &str,
    options: &CiteAgentOptions,
    state: &mut CiteState,
    args: &Value,
    require_passage: bool,
) -> Result<Value, String> {
    if citation_limit_reached(state.citations.len(), options.max_citations) {
        return Err(format!("出处数量已达上限 {}", options.max_citations));
    }
    let note_quote = text_arg(args, "note_quote").trim().to_string();
    if note_quote.is_empty() {
        return Err("note_quote 不能为空".into());
    }
    if !normalize_ws(body).contains(&normalize_ws(&note_quote)) {
        return Err("note_quote 必须逐字来自这篇笔记；不要改写".into());
    }
    let input = if require_passage {
        NoteCitationInput {
            note_id: note_id.into(),
            note_quote,
            passage_id: text_arg(args, "passage_id"),
            ..NoteCitationInput::default()
        }
    } else {
        NoteCitationInput {
            note_id: note_id.into(),
            note_quote,
            passage_id: String::new(),
            candidate_id: text_arg(args, "candidate_id"),
            work_title: text_arg(args, "work_title"),
            section_title: String::new(),
            author: String::new(),
            volume: String::new(),
            page: String::new(),
            original_text: String::new(),
            locator: String::new(),
            status: "candidate".into(),
            url: text_arg(args, "url"),
            snippet: text_arg(args, "snippet"),
            provider: text_arg(args, "provider"),
        }
    };
    let mut input = input;
    if !require_passage {
        if !input.candidate_id.is_empty() {
            if let Some(candidate) = history.candidate_by_id(&input.candidate_id)? {
                if input.work_title.trim().is_empty() {
                    input.work_title = if candidate.work_title.trim().is_empty() { candidate.title } else { candidate.work_title };
                }
                if input.url.trim().is_empty() { input.url = candidate.url; }
                if input.snippet.trim().is_empty() { input.snippet = candidate.snippet; }
                if input.provider.trim().is_empty() { input.provider = candidate.provider; }
                input.candidate_id.clear();
            }
        }
        if let Some(full) = full_text_url(&input.work_title) {
            let current = input.url.to_lowercase();
            if current.is_empty() || current.contains("hathitrust") || current.contains("read.nlc.cn") || current.contains("fhac.com.cn") {
                input.url = full;
            }
        }
    }
    let citation = history.add_note_citation(input)?;
    state.citations.push(citation.clone());
    Ok(json!({
        "ok": true,
        "id": citation.id,
        "status": citation.status,
        "work_title": citation.work_title,
        "total": state.citations.len(),
    }))
}

fn citation_limit_reached(count: usize, max: usize) -> bool {
    max > 0 && count >= max
}

fn text_arg(args: &Value, key: &str) -> String {
    args.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn normalize_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join("")
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
    fn note_quote_must_be_substring() {
        let body = "曹操确有过多次疫区接触史。";
        assert!(normalize_ws(body).contains(&normalize_ws("曹操确有过多次疫区接触史")));
        assert!(!normalize_ws(body).contains(&normalize_ws("曹操多次接触疫区")));
    }

    #[test]
    fn citation_limit_stops_at_configured_max() {
        assert!(!citation_limit_reached(5, 6));
        assert!(citation_limit_reached(6, 6));
        assert!(citation_limit_reached(7, 6));
        assert!(!citation_limit_reached(0, 0));
    }

    #[test]
    fn full_text_url_maps_common_works() {
        assert_eq!(
            full_text_url("《三国志》").as_deref(),
            Some("https://zh.wikisource.org/wiki/三國志")
        );
        assert_eq!(
            full_text_url("后汉书").as_deref(),
            Some("https://zh.wikisource.org/wiki/後漢書")
        );
        assert_eq!(
            full_text_url("宋书").as_deref(),
            Some("https://zh.wikisource.org/wiki/宋書")
        );
        assert_eq!(full_text_url("未知书名"), None);
    }
}
