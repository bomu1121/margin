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
use super::clients::{self, AiConfig, ChatMessage, SearchUsage, ToolSpec};
use super::history::{HistoryStore, LocalPassageHit, NoteCitation, NoteCitationInput};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillOriginalReport {
    /// None means the fill failed; `error` says why and `usage` still carries
    /// any tokens spent before the failure so a batch budget can count them.
    pub citation: Option<NoteCitation>,
    #[serde(default)]
    pub error: String,
    pub usage: Usage,
    #[serde(default)]
    pub search_usage: SearchUsage,
    pub duration_ms: u64,
    pub source_quality: String,
}

#[derive(Default)]
struct CiteState {
    citations: Vec<NoteCitation>,
    summary: String,
    finished: bool,
}

/// One machine-readable claim emitted by the AI-note engine.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ClaimSpec {
    #[serde(default)]
    quote: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    work: String,
    #[serde(default)]
    section: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    original: String,
    #[serde(default)]
    url: String,
}

const CLAIMS_START: &str = "<!-- yebian-claims";

fn parse_claims(body: &str) -> Option<Vec<ClaimSpec>> {
    let start = body.find(CLAIMS_START)? + CLAIMS_START.len();
    let rest = &body[start..];
    let end = rest.find("-->")?;
    serde_json::from_str(rest[..end].trim()).ok()
}

fn is_citable_kind(kind: &str) -> bool {
    matches!(kind.trim(), "史载" | "史實" | "史实" | "转述" | "轉述")
}

/// Deterministic path for AI-note output. Returns `None` when the note has no
/// claims block, so old notes keep using the model/tool path.
pub fn run_claims_block(
    history: &HistoryStore,
    note_id: &str,
    body: &str,
    options: &CiteAgentOptions,
) -> Result<Option<CiteReport>, String> {
    let Some(claims) = parse_claims(body) else {
        return Ok(None);
    };
    let started = Instant::now();
    let normalized_body = normalize_ws(body);
    let mut citations = Vec::new();
    let mut invalid = 0usize;
    let mut pending_check = 0usize;
    let mut local_found = 0usize;

    for claim in claims {
        if citations.len() >= options.max_citations {
            invalid += 1;
            continue;
        }
        if claim.kind.trim() == "待查" {
            let quote = claim.quote.trim();
            if quote.is_empty() || !normalized_body.contains(&normalize_ws(quote)) {
                invalid += 1;
                continue;
            }
            let hits = history.local_candidates(quote, 2).unwrap_or_default();
            if let Some(hit) = hits.first() {
                let input = NoteCitationInput {
                    note_id: note_id.to_string(),
                    note_quote: quote.to_string(),
                    work_title: hit.work_title.clone(),
                    section_title: hit.section_title.clone(),
                    volume: hit.volume.clone(),
                    page: hit.page.clone(),
                    status: "candidate".into(),
                    snippet: hit.original_text.chars().take(200).collect::<String>(),
                    provider: "local-search".into(),
                    ..NoteCitationInput::default()
                };
                match history.add_note_citation(input) {
                    Ok(row) => {
                        citations.push(row);
                        local_found += 1;
                    }
                    Err(_) => pending_check += 1,
                }
            } else {
                pending_check += 1;
            }
            continue;
        }
        if !is_citable_kind(&claim.kind) {
            invalid += 1;
            continue;
        }
        let quote = claim.quote.trim();
        if quote.is_empty() || !normalized_body.contains(&normalize_ws(quote)) {
            invalid += 1;
            continue;
        }
        let original = claim.original.trim().to_string();
        let work = claim.work.trim().to_string();
        // A claim without a book is not a usable provenance record. Do not
        // turn the note's own paraphrase into a "found" source.
        if work.is_empty() {
            invalid += 1;
            continue;
        }
        let status = if original.is_empty() { "candidate" } else { "found" };
        let input = NoteCitationInput {
            note_id: note_id.to_string(),
            note_quote: quote.to_string(),
            work_title: work,
            section_title: claim.section.trim().to_string(),
            author: claim.author.trim().to_string(),
            original_text: original,
            status: status.to_string(),
            url: claim.url.trim().to_string(),
            provider: "ai-note-claims".into(),
            ..NoteCitationInput::default()
        };
        match history.add_note_citation(input) {
            Ok(row) => citations.push(row),
            Err(_) => invalid += 1,
        }
    }

    let pending = citations.iter().filter(|item| item.status == "candidate").count();
    let pending_manual = pending.saturating_sub(local_found);
    let status = if citations.is_empty() { "no_sources" } else { "success" };
    let mut detail = Vec::new();
    if local_found > 0 {
        detail.push(format!("{local_found} 条本地候选"));
    }
    if pending_manual > 0 {
        detail.push(format!("{pending_manual} 条待补原文"));
    }
    if pending_check > 0 {
        detail.push(format!("{pending_check} 条待查未命中"));
    }
    if invalid > 0 {
        detail.push(format!("{invalid} 条判断/无效/超限跳过"));
    }
    let skip_detail = if detail.is_empty() {
        "没有可记账的条目".to_string()
    } else {
        detail.join("，")
    };
    let summary = if citations.is_empty() {
        format!("AI 成稿史源清单没有可引用条目（{skip_detail}）")
    } else {
        format!(
            "从 AI 成稿史源清单读取 {} 条出处（{skip_detail}）",
            citations.len()
        )
    };
    Ok(Some(CiteReport {
        status: status.into(),
        summary,
        citations,
        turns: Vec::new(),
        usage: Usage::default(),
        duration_ms: started.elapsed().as_millis() as u64,
    }))
}

#[derive(Debug, Deserialize, Default)]
struct FillReply {
    #[serde(default)]
    quote: String,
}

fn parse_fill_reply(content: &str) -> Option<String> {
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    let reply: FillReply = serde_json::from_str(&content[start..=end]).ok()?;
    let quote = reply.quote.trim().to_string();
    (!quote.is_empty()).then_some(quote)
}

fn quote_in_text(text: &str, quote: &str) -> bool {
    let quote = normalize_ws(quote);
    quote.chars().count() >= 4 && normalize_ws(text).contains(&quote)
}

const FILL_SYSTEM: &str = "你是史料摘录员。只从给定网页正文中找与“笔记原句”对应的连续原文。\n\
只返回 JSON：{\"quote\":\"逐字原文\"}；找不到返回 {\"quote\":\"\"}。\n\
必须逐字复制网页正文，不要改写、不要繁简转换、不要补字、不要解释。原文至少包含一个能支撑笔记原句的完整分句。\n\
原文必须在人物、数量、时间、结果上与笔记原句对得上；只有人名或个别词相同、缺少关键数量或结果的，不算找到。\n\
只返回史书正文本身；不要包含现代年份、页码、“同上”、站点导航、编者注、引用信息或解释。";

fn quote_is_clean(quote: &str) -> bool {
    !quote.contains("同上")
        && !quote.contains("頁")
        && !quote.contains("页")
        && !quote.contains("http")
        && !quote.contains("www.")
}

fn source_quality(url: &str) -> &'static str {
    let value = url.to_ascii_lowercase();
    if ["zh.wikisource.org", "ctext.org", "shidianguji.com", "njmuseum.org.cn"]
        .iter()
        .any(|host| value.contains(host))
    {
        "primary"
    } else if ["read.nlc.cn", "hathitrust.org", "fhac.com.cn"]
        .iter()
        .any(|host| value.contains(host))
    {
        "catalog"
    } else {
        "secondary"
    }
}

/// Fetch the candidate URL (or search for one when the candidate has no URL)
/// and ask the model to extract the passage. The quote must be a verbatim
/// substring of the fetched text before anything is written.
pub async fn fill_original(
    history: &HistoryStore,
    id: &str,
    ai: &AiConfig,
    search: Option<&clients::SearchConfig>,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<FillOriginalReport, String> {
    let started = Instant::now();
    let citation = history.note_citation(id)?;
    let mut search_usage = SearchUsage::default();
    let mut total_usage = Usage::default();
    let mut last_error = String::new();
    let mut last_quality = String::new();

    // Local full text wins over the network when there is no URL or the URL is
    // only a catalog page. Explicit primary/secondary URLs stay authoritative.
    let local_first =
        citation.url.trim().is_empty() || source_quality(citation.url.trim()) == "catalog";
    let local_hits = if citation.provider == "local-search" {
        // The candidate was produced by the cross-work search; re-run it so the
        // fill path sees the same passage the user is looking at.
        history.local_candidates(&citation.note_quote, 3)
    } else if local_first && !citation.work_title.trim().is_empty() {
        history.local_passages(&citation.work_title, &citation.note_quote, 3)
    } else {
        Ok(Vec::new())
    };
    match local_hits {
        Ok(hits) => {
            for hit in hits {
                match fill_from_passage(history, id, &citation, &hit, ai, cost, token).await {
                    Ok((citation, usage)) => {
                        checkpoint::add_usage(&mut total_usage, &usage);
                        return Ok(FillOriginalReport {
                            citation: Some(citation),
                            error: String::new(),
                            usage: total_usage,
                            search_usage,
                            duration_ms: started.elapsed().as_millis() as u64,
                            source_quality: "local".into(),
                        });
                    }
                    Err((error, usage)) => {
                        checkpoint::add_usage(&mut total_usage, &usage);
                        last_error = error;
                        last_quality = "local".into();
                    }
                }
            }
        }
        Err(error) => last_error = error,
    }

    let mut candidates: Vec<(String, &'static str)> = Vec::new();
    if !citation.url.trim().is_empty() {
        let url = citation.url.trim().to_string();
        candidates.push((url.clone(), source_quality(&url)));
    } else if let Some(search) = search {
        let query = format!(
            "{} {} {}",
            citation.work_title.trim(),
            citation.section_title.trim(),
            citation.note_quote.trim()
        )
        .chars()
        .take(120)
        .collect::<String>();
        let hits = match clients::search_counted(search, &query, 5, token).await {
            Ok((hits, usage)) => {
                search_usage = usage;
                hits
            }
            Err(error) => {
                return Ok(FillOriginalReport {
                    citation: None,
                    error,
                    usage: total_usage,
                    search_usage,
                    duration_ms: started.elapsed().as_millis() as u64,
                    source_quality: last_quality,
                });
            }
        };
        let mut ranked = hits
            .into_iter()
            .filter(|item| !item.url.trim().is_empty())
            .map(|item| {
                let quality = source_quality(&item.url);
                (item.url, quality)
            })
            .filter(|(_, quality)| *quality != "catalog")
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(_, quality)| match *quality {
            "primary" => 0,
            "secondary" => 1,
            _ => 2,
        });
        candidates.extend(ranked.into_iter().take(3));
        if candidates.is_empty() {
            return Ok(FillOriginalReport {
                citation: None,
                error: "联网检索只返回目录页；请手动补链接或原文".into(),
                usage: total_usage,
                search_usage,
                duration_ms: started.elapsed().as_millis() as u64,
                source_quality: last_quality,
            });
        }
    } else if last_error.is_empty() {
        return Err("这条出处没有来源链接，且没有配置联网检索；先补链接或手动补原文".into());
    } else {
        return Ok(FillOriginalReport {
            citation: None,
            error: last_error,
            usage: total_usage,
            search_usage,
            duration_ms: started.elapsed().as_millis() as u64,
            source_quality: last_quality,
        });
    }

    for (url, quality) in candidates {
        last_quality = quality.to_string();
        match fill_from_url(history, id, &citation, &url, ai, cost, token).await {
            Ok((citation, usage)) => {
                checkpoint::add_usage(&mut total_usage, &usage);
                return Ok(FillOriginalReport {
                    citation: Some(citation),
                    error: String::new(),
                    usage: total_usage,
                    search_usage,
                    duration_ms: started.elapsed().as_millis() as u64,
                    source_quality: quality.to_string(),
                });
            }
            Err((error, usage)) => {
                checkpoint::add_usage(&mut total_usage, &usage);
                last_error = error;
            }
        }
    }
    Ok(FillOriginalReport {
        citation: None,
        error: format!("没有从来源页提取到原文：{last_error}"),
        usage: total_usage,
        search_usage,
        duration_ms: started.elapsed().as_millis() as u64,
        source_quality: last_quality,
    })
}

async fn fill_from_passage(
    history: &HistoryStore,
    id: &str,
    citation: &NoteCitation,
    hit: &LocalPassageHit,
    ai: &AiConfig,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<(NoteCitation, Usage), (String, Usage)> {
    let text = hit.original_text.trim();
    if text.is_empty() {
        return Err(("本地原文为空".into(), Usage::default()));
    }
    let exact = citation.note_quote.trim();
    if quote_in_text(text, exact) {
        let citation = history
            .set_note_citation_local(id, exact, &hit.url, &hit.source_id, &hit.passage_id)
            .map_err(|error| (error, Usage::default()))?;
        return Ok((citation, Usage::default()));
    }
    let user = format!(
        "书名：{}\n篇名：{}\n笔记原句：{}\n本地原文：\n{}",
        citation.work_title.trim(),
        citation.section_title.trim(),
        citation.note_quote.trim(),
        text.chars().take(16_000).collect::<String>()
    );
    let reply = clients::chat(ai, FILL_SYSTEM, &user, token)
        .await
        .map_err(|error| (error, Usage::default()))?;
    let mut usage = checkpoint::usage_from_chat(&reply);
    cost.apply(&mut usage);
    let quote = parse_fill_reply(&reply.content)
        .ok_or_else(|| ("AI 没有从本地原文找到可用句子".to_string(), usage.clone()))?;
    if !quote_is_clean(&quote) {
        return Err(("AI 返回的原文夹带页码/注记，未写入".into(), usage));
    }
    if !quote_in_text(text, &quote) {
        return Err(("AI 返回的原文不在本地段落里，未写入".into(), usage));
    }
    let citation = history
        .set_note_citation_local(id, &quote, &hit.url, &hit.source_id, &hit.passage_id)
        .map_err(|error| (error, usage.clone()))?;
    Ok((citation, usage))
}

async fn fill_from_url(
    history: &HistoryStore,
    id: &str,
    citation: &NoteCitation,
    url: &str,
    ai: &AiConfig,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<(NoteCitation, Usage), (String, Usage)> {
    let page = super::web_fetch::fetch(url)
        .await
        .map_err(|error| (error, Usage::default()))?;
    let page_text = page.text.trim();
    if page_text.is_empty() {
        return Err(("来源页面没有提取到正文".into(), Usage::default()));
    }
    let exact = citation.note_quote.trim();
    if quote_in_text(page_text, exact) {
        let citation = history
            .set_note_citation_original(id, exact, Some(&page.url))
            .map_err(|error| (error, Usage::default()))?;
        return Ok((citation, Usage::default()));
    }
    let user = format!(
        "书名：{}\n篇名：{}\n笔记原句：{}\n网页正文：\n{}",
        citation.work_title.trim(),
        citation.section_title.trim(),
        citation.note_quote.trim(),
        page_text.chars().take(16_000).collect::<String>()
    );
    let reply = clients::chat(ai, FILL_SYSTEM, &user, token)
        .await
        .map_err(|error| (error, Usage::default()))?;
    let mut usage = checkpoint::usage_from_chat(&reply);
    cost.apply(&mut usage);
    let quote = parse_fill_reply(&reply.content)
        .ok_or_else(|| ("AI 没有找到可用的原文".to_string(), usage.clone()))?;
    if !quote_is_clean(&quote) {
        return Err(("AI 返回的原文夹带页码/注记，未写入；可换一个来源".into(), usage));
    }
    if !quote_in_text(page_text, &quote) {
        return Err(("AI 返回的原文不在这页正文里，未写入；可手动补原文".into(), usage));
    }
    let citation = history
        .set_note_citation_original(id, &quote, Some(&page.url))
        .map_err(|error| (error, usage.clone()))?;
    Ok((citation, usage))
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
    fn fill_reply_parses_json_and_validates_quote() {
        let reply = "```json\n{\"quote\":\"公至延津，绍众大溃。\"}\n```";
        let quote = parse_fill_reply(reply).expect("quote");
        assert_eq!(quote, "公至延津，绍众大溃。");
        assert!(quote_in_text("前文 公至延津，绍众大溃。 后文", &quote));
        assert!(!quote_in_text("公至延津", &quote));
        assert!(parse_fill_reply("{\"quote\":\"\"}").is_none());
        assert!(parse_fill_reply("没有 JSON").is_none());
        assert!(quote_is_clean("熹平二年正月，大疫。"));
        assert!(!quote_is_clean("靈帝熹平二年，大疫。同上，頁334。"));
        assert_eq!(source_quality("https://zh.wikisource.org/wiki/三國志"), "primary");
        assert_eq!(source_quality("https://www.njmuseum.org.cn/xx"), "primary");
        assert_eq!(source_quality("https://w9.ihp.sinica.edu.tw/post"), "secondary");
        assert_eq!(source_quality("https://read.nlc.cn/"), "catalog");
    }

    #[test]
    fn full_text_url_maps_common_works() {        assert_eq!(
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

    #[test]
    fn claims_block_parses_and_filters_judgments() {
        let body = "曹操在官渡击败袁绍。这是作者判断。";
        let claims = vec![
            ClaimSpec {
                quote: "曹操在官渡击败袁绍".into(),
                kind: "史载".into(),
                work: "三国志".into(),
                section: "武帝纪".into(),
                original: "公至延津，绍众大溃。".into(),
                url: "https://example.com/1".into(),
                ..Default::default()
            },
            ClaimSpec {
                quote: "这是作者判断".into(),
                kind: "判断".into(),
                ..Default::default()
            },
        ];
        let block = serde_json::to_string(&claims).unwrap();
        let body = format!("{body}\n<!-- yebian-claims\n{block}\n-->");
        let parsed = parse_claims(&body).expect("claims block");
        assert_eq!(parsed.len(), 2);
        assert!(is_citable_kind(&parsed[0].kind));
        assert!(!is_citable_kind(&parsed[1].kind));
        assert!(!is_citable_kind("待查"));
        assert!(parse_claims("没有 claims").is_none());
        assert!(parse_claims("<!-- yebian-claims\nnot json\n-->").is_none());
    }

    #[test]
    fn claims_block_writes_found_candidate_and_skips_judgment() {
        let dir = std::env::temp_dir().join(format!(
            "yebian-cite-claims-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.sqlite");
        let _facts = super::super::facts::FactStore::open(&path).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let body = "曹操在官渡击败袁绍。袁绍兵力占优。这是作者判断。曹操坑杀降卒。";
        let claims = vec![
            ClaimSpec {
                quote: "曹操在官渡击败袁绍".into(),
                kind: "史载".into(),
                work: "三国志".into(),
                section: "武帝纪".into(),
                original: "公至延津，绍众大溃。".into(),
                ..Default::default()
            },
            ClaimSpec {
                quote: "袁绍兵力占优".into(),
                kind: "转述".into(),
                work: "三国志".into(),
                section: "袁绍传".into(),
                ..Default::default()
            },
            ClaimSpec {
                quote: "这是作者判断".into(),
                kind: "判断".into(),
                ..Default::default()
            },
            ClaimSpec {
                quote: "曹操坑杀降卒".into(),
                kind: "转述".into(),
                original: "素材句子，不是史书原文".into(),
                ..Default::default()
            },
            ClaimSpec {
                quote: "另一条无源史实".into(),
                kind: "待查".into(),
                ..Default::default()
            },
        ];
        let block = serde_json::to_string(&claims).unwrap();
        let body = format!("{body}\n<!-- yebian-claims\n{block}\n-->");
        let report = run_claims_block(&history, "note-1", &body, &CiteAgentOptions::default())
            .unwrap()
            .expect("claims report");
        assert_eq!(report.status, "success");
        assert_eq!(report.citations.len(), 2);
        assert_eq!(report.citations[0].status, "found");
        assert_eq!(report.citations[1].status, "candidate");
        assert!(report.summary.contains("1 条待查"));
        assert!(report.summary.contains("2 条判断/无效/超限跳过"));
        assert_eq!(report.citations.len(), history.note_citations("note-1").unwrap().len());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_claim_gets_local_candidate_without_fake_found() {
        let dir = std::env::temp_dir().join(format!(
            "yebian-cite-local-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.sqlite");
        let _facts = super::super::facts::FactStore::open(&path).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        history
            .import_local_work(
                "后汉书",
                "志第十七",
                "五行志五",
                "范晔",
                "南朝宋",
                "熹平二年正月，大疫流行，死者甚众。疫气流行，郡国被灾，百姓多死。诏遣使者循行，赐医药，所在存恤。其後连年疾疫，民多夭札，户口减耗。太史奏曰，此阴气盛而阳微之应也。光和二年春，大疫。中平元年，疫。建宁四年，大疫。郡国上言，民多疫死。",
            )
            .unwrap();
        let body = "灵帝时期疫病频发。熹平二年正月大疫。";
        let claims = vec![ClaimSpec {
            quote: "熹平二年正月大疫".into(),
            kind: "待查".into(),
            ..Default::default()
        }];
        let block = serde_json::to_string(&claims).unwrap();
        let body = format!("{body}\n<!-- yebian-claims\n{block}\n-->");
        let report = run_claims_block(&history, "note-local", &body, &CiteAgentOptions::default())
            .unwrap()
            .expect("claims report");
        assert_eq!(report.status, "success");
        assert_eq!(report.citations.len(), 1);
        let citation = &report.citations[0];
        assert_eq!(citation.status, "candidate");
        assert_eq!(citation.provider, "local-search");
        assert_eq!(citation.work_title, "后汉书");
        assert_eq!(citation.section_title, "五行志五");
        assert!(citation.original_text.is_empty());
        assert!(citation.snippet.contains("大疫"));
        assert!(citation.passage_id.is_empty());
        assert!(report.summary.contains("1 条本地候选"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
