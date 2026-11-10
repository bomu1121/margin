//! 溯源流水线：抽取可核查主张 -> 多查询检索 -> 去重排序 -> 分批核查。
//!
//! 旧实现只发一次“标题 + 事实核查”，然后让模型拿 5 条结果核对整篇笔记；
//! 一条视频通常只能命中标题事件，数字/机构类主张大量落空。这里把检索
//! 粒度降到“主张”，并保留 checkpoint，失败后可继续而不是重烧全部检索。

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::checkpoint::{self, CostModel};
use super::clients::{self, AiConfig, SearchConfig, SearchHit};
use super::prompts;
use super::facts::{FactCandidate, FactEvidenceInput, FactStore};
use super::store::ProductionStore;
use super::web_fetch;
use super::{TraceItem, Usage};

const VERIFY_BATCH: usize = 4;
const VERIFY_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_QUERIES_PER_CLAIM: usize = 2;
const MAX_QUOTE_CHARS: usize = 400;
const MAX_EVIDENCE_CHARS: usize = 3500;
const MAX_FETCHES: usize = 12;
const MAX_FETCH_TEXT_CHARS: usize = 6000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TraceClaim {
    claim: String,
    #[serde(default)]
    queries: Vec<String>,
    #[serde(default)]
    source_quote: String,
    /// `history` 表示这条主张走史料溯源；其余走网页证据。
    #[serde(default)]
    domain: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatchDecision {
    index: usize,
    fact_id: Option<String>,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Clone)]
struct ClaimEvidence {
    claim: String,
    queries: Vec<String>,
    hits: Vec<SearchHit>,
}

pub async fn verify_fact(
    store: &ProductionStore,
    facts: &FactStore,
    fact_id: &str,
    claim: &str,
    queries: &[String],
    ai: &AiConfig,
    search: &SearchConfig,
    max_sources: u32,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<TraceItem, String> {
    let mut queries = queries
        .iter()
        .map(|query| query.trim().chars().take(128).collect::<String>())
        .filter(|query| !query.is_empty())
        .take(MAX_QUERIES_PER_CLAIM)
        .collect::<Vec<_>>();
    if queries.is_empty() {
        queries.push(claim.chars().take(60).collect());
    }
    let job_key = format!("factcheck_{fact_id}");
    let claim = TraceClaim {
        claim: claim.trim().to_string(),
        queries: queries.clone(),
        source_quote: String::new(),
        domain: "general".into(),
    };
    let evidence = search_claims(store, &job_key, 0, search, std::slice::from_ref(&claim), max_sources.clamp(1, 10) as usize, usage, cost, token).await?;
    let evidence = fetch_evidence(store, &job_key, 0, evidence, usage, cost, token).await;
    let mut items = verify_claims(store, &job_key, 0, ai, &evidence, max_sources.clamp(1, 10) as usize, usage, cost, token).await?;
    let item = items.pop().ok_or_else(|| "事实核验没有返回结果".to_string())?;
    let evidence_inputs = evidence.first().map(|source| {
        source.hits.iter()
            .filter(|hit| !item.source.is_empty() && normalize_url(&hit.url) == normalize_url(&item.source))
            .map(|hit| FactEvidenceInput {
                source_url: hit.url.clone(),
                title: hit.title.clone(),
                quote: item.quote.clone(),
                relation: evidence_relation(&item),
            })
            .collect::<Vec<_>>()
    }).unwrap_or_default();
    facts.record_verification(fact_id, &item.verdict, &search.provider, &ai.model, &queries, &evidence_inputs)?;
    Ok(item)
}

pub async fn run(
    store: &ProductionStore,
    facts: &FactStore,
    history: &super::history::HistoryStore,
    note_id: &str,
    job_id: &str,
    step_index: usize,
    ai: &AiConfig,
    search: &SearchConfig,
    title: &str,
    body: &str,
    max_claims: u32,
    max_sources: u32,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<Vec<TraceItem>, String> {
    let max_claims = max_claims.clamp(3, 20) as usize;
    let max_sources = max_sources.clamp(1, 10) as usize;
    let claims = extract_claims(store, job_id, ai, title, body, max_claims, usage, cost, token)
        .await
        .unwrap_or_else(|error| {
            let _ = store.log(job_id, Some(step_index), "warn", &format!("主张抽取失败，使用启发式兜底：{error}"));
            fallback_claims(body, title, max_claims)
        });
    let claims = if claims.is_empty() {
        let fallback = fallback_claims(body, title, max_claims);
        if !fallback.is_empty() {
            let _ = store.log(job_id, Some(step_index), "warn", "模型没有返回可核查主张，使用正文启发式兜底");
        }
        fallback
    } else {
        claims
    };
    if claims.is_empty() {
        let _ = store.log(job_id, Some(step_index), "warn", "正文没有提取到可外部核查的主张");
        return Ok(Vec::new());
    }
    let _ = store.log(
        job_id,
        Some(step_index),
        "info",
        &format!("提取到 {} 条可核查主张，开始按主张检索", claims.len()),
    );

    let matches = if note_id.trim().is_empty() {
        vec![None; claims.len()]
    } else {
        match match_existing_claims(store, facts, job_id, ai, &claims, usage, cost, token, step_index).await {
            Ok(items) => items,
            Err(error) => {
                let _ = store.log(job_id, Some(step_index), "warn", &format!("事实归并判断失败，本次全部作为新事实：{error}"));
                vec![None; claims.len()]
            }
        }
    };
    let matched_count = matches.iter().filter(|item| item.is_some()).count();
    if matched_count > 0 {
        let _ = store.log(job_id, Some(step_index), "info", &format!("归并到已有事实：{matched_count} 条"));
    }

    let mut fact_ids = HashMap::new();
    let mut pending_claims = Vec::new();
    let mut pending_indices = Vec::new();
    let mut ordered: Vec<Option<TraceItem>> = vec![None; claims.len()];

    for (index, claim) in claims.iter().enumerate() {
        let mut attached = false;
        if let Some(fact_id) = matches.get(index).and_then(|item| item.as_deref()) {
            match facts.record_mention_for_fact_with_domain(fact_id, note_id, job_id, &claim.claim, &claim.source_quote, &claim.domain) {
                Ok(()) => {
                    let existing = facts.get_fact(fact_id).ok().flatten();
                    let verdict = existing
                        .as_ref()
                        .map(|item| verdict_for_fact_status(&item.status))
                        .unwrap_or("未核验")
                        .to_string();
                    let first = existing.as_ref().and_then(|item| item.evidence.first());
                    ordered[index] = Some(TraceItem {
                        claim: claim.claim.clone(),
                        verdict,
                        relation: first.map(|item| item.relation.clone()).unwrap_or_default(),
                        source: first.map(|item| item.url.clone()).unwrap_or_default(),
                        quote: first.map(|item| item.quote.clone()).unwrap_or_default(),
                    });
                    attached = true;
                }
                Err(error) => {
                    let _ = store.log(job_id, Some(step_index), "warn", &format!("归并提及失败，改走新事实：{error}"));
                }
            }
        }
        if !attached {
            pending_claims.push(claim.clone());
            pending_indices.push(index);
        }
    }

    if !note_id.trim().is_empty() {
        for claim in &pending_claims {
            match facts.record_candidate_with_domain(note_id, job_id, &claim.claim, &claim.source_quote, "", &claim.queries, &claim.domain) {
                Ok(id) => { fact_ids.insert(normalize_claim(&claim.claim), id); }
                Err(error) => { let _ = store.log(job_id, Some(step_index), "warn", &format!("事实入库失败：{error}")); }
            }
        }
        let _ = store.log(
            job_id,
            Some(step_index),
            "info",
            &format!("新事实候选 {} 条；归并复用 {} 条", fact_ids.len(), matched_count),
        );
    }

    let evidence = if pending_claims.is_empty() {
        Vec::new()
    } else {
        search_claims(store, job_id, step_index, search, &pending_claims, max_sources, usage, cost, token).await?
    };
    let evidence = if evidence.is_empty() {
        Vec::new()
    } else {
        fetch_evidence(store, job_id, step_index, evidence, usage, cost, token).await
    };
    let with_hits = evidence.iter().filter(|item| !item.hits.is_empty()).count();
    if !pending_claims.is_empty() {
        let _ = store.log(
            job_id,
            Some(step_index),
            "info",
            &format!("检索/抓取完成：{} 条新主张有候选证据，开始分批核查", with_hits),
        );
    }

    let verified_items = if evidence.is_empty() {
        Vec::new()
    } else {
        verify_claims(store, job_id, step_index, ai, &evidence, max_sources, usage, cost, token).await?
    };
    for (item, source) in verified_items.iter().zip(evidence.iter()) {
        let Some(fact_id) = fact_ids.get(&normalize_claim(&item.claim)) else { continue };
        let evidence_inputs = source
            .hits
            .iter()
            .filter(|hit| !item.source.is_empty() && normalize_url(&hit.url) == normalize_url(&item.source))
            .map(|hit| FactEvidenceInput {
                source_url: hit.url.clone(),
                title: hit.title.clone(),
                quote: item.quote.clone(),
                relation: evidence_relation(&item),
            })
            .collect::<Vec<_>>();
        if let Err(error) = facts.record_verification(
            fact_id,
            &item.verdict,
            &search.provider,
            &ai.model,
            &source.queries,
            &evidence_inputs,
        ) {
            let _ = store.log(job_id, Some(step_index), "warn", &format!("事实核验入库失败：{error}"));
        }
    }

    // 历史主张走源优先：自动检索史料、抓全文、逐字对齐、写 citation。
    // 找不到或网络失败只记日志，不影响网页溯源结果。
    let history_candidates = claims
        .iter()
        .enumerate()
        .filter_map(|(index, claim)| {
            if claim.domain != "history" {
                return None;
            }
            let fact_id = matches
                .get(index)
                .and_then(|item| item.clone())
                .or_else(|| fact_ids.get(&normalize_claim(&claim.claim)).cloned())?;
            Some((claim.claim.clone(), fact_id, claim.queries.clone()))
        })
        .take(8)
        .collect::<Vec<_>>();
    if !history_candidates.is_empty() {
        let _ = store.log(
            job_id,
            Some(step_index),
            "info",
            &format!("历史主张 {} 条，开始自动史料入库", history_candidates.len()),
        );
    }
    let history_context = format!("{title}\n{}", body.chars().take(1800).collect::<String>());
    let ctext_key = store
        .get_setting("production.history.ctext_api_key")
        .ok()
        .flatten()
        .unwrap_or_default();
    for (claim, fact_id, queries) in history_candidates {
        if token.is_cancelled() {
            break;
        }
        match super::history_ingest::ingest_fact_history(
            store,
            facts,
            history,
            &fact_id,
            &claim,
            &queries,
            &history_context,
            &ctext_key,
            Some(ai),
            usage,
            cost,
            token,
            false,
            1,
        )
        .await
        {
            Ok(report) => {
                let _ = store.log(
                    job_id,
                    Some(step_index),
                    "info",
                    &format!(
                        "史料自动入库：检索 {} 次、抓取 {} 页、新增 {} 条 citation",
                        report.searched, report.fetched, report.citations_created
                    ),
                );
                for note in report.notes.into_iter().take(6) {
                    let _ = store.log(job_id, Some(step_index), "info", &format!("史料：{note}"));
                }
            }
            Err(error) => {
                let _ = store.log(job_id, Some(step_index), "warn", &format!("史料自动入库失败：{error}"));
            }
        }
    }

    let mut verified_by_claim = HashMap::new();
    for item in verified_items {
        verified_by_claim.insert(normalize_claim(&item.claim), item);
    }
    let result = ordered
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            item.or_else(|| verified_by_claim.remove(&normalize_claim(&claims[index].claim)))
                .unwrap_or_else(|| not_found(&claims[index].claim))
        })
        .collect::<Vec<_>>();

    let verified = result.iter().filter(|item| item.verdict.contains("可印证")).count();
    let _ = store.log(
        job_id,
        Some(step_index),
        "info",
        &format!("溯源完成：{} 条主张，{} 条外部可印证", result.len(), verified),
    );
    Ok(result)
}

fn verdict_for_fact_status(status: &str) -> &'static str {
    match status {
        "verified" => "外部可印证",
        "disputed" => "存在争议",
        "unsupported" => "未找到外部出处",
        _ => "未核验",
    }
}

async fn match_existing_claims(
    store: &ProductionStore,
    facts: &FactStore,
    job_id: &str,
    ai: &AiConfig,
    claims: &[TraceClaim],
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
    step_index: usize,
) -> Result<Vec<Option<String>>, String> {
    let mut candidates: Vec<Vec<FactCandidate>> = Vec::with_capacity(claims.len());
    let mut has_candidates = false;
    for claim in claims {
        let list = facts.candidate_facts_for_claim(&claim.claim, 5).unwrap_or_default();
        has_candidates |= !list.is_empty();
        candidates.push(list);
    }
    if !has_candidates {
        return Ok(vec![None; claims.len()]);
    }
    let payload = serde_json::json!({
        "claims": claims.iter().enumerate().map(|(index, claim)| serde_json::json!({
            "index": index,
            "claim": claim.claim,
        })).collect::<Vec<_>>(),
        "candidates": claims.iter().enumerate().map(|(index, _claim)| serde_json::json!({
            "claimIndex": index,
            "facts": candidates.get(index).map(|items| items.iter().map(|item| serde_json::json!({
                "id": item.id, "text": item.text, "status": item.status
            })).collect::<Vec<_>>()).unwrap_or_default(),
        })).collect::<Vec<_>>(),
    });
    let user = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
    let key = format!("trace:match:{}", checkpoint::hash_json(&payload)?);
    let result = checkpoint::cached_chat(
        store,
        job_id,
        "trace",
        &key,
        prompts::trace_match_system(),
        &user,
        ai,
        usage,
        cost,
        token,
    )
    .await;
    let mut decisions = vec![None; claims.len()];
    if let Ok(result) = result {
        let parsed = clients::strip_fence(&result.content);
        if let Ok(items) = serde_json::from_str::<Vec<MatchDecision>>(&parsed) {
            for item in items {
                if item.index >= claims.len() {
                    continue;
                }
                let Some(fact_id) = item.fact_id else { continue };
                let allowed = candidates.get(item.index).map(|list| list.iter().any(|candidate| candidate.id == fact_id)).unwrap_or(false);
                if !allowed {
                    let _ = store.log(job_id, Some(step_index), "warn", &format!("事实归并返回了候选外 id，已忽略：{fact_id}"));
                    continue;
                }
                decisions[item.index] = Some(fact_id.clone());
                let _ = store.log(
                    job_id,
                    Some(step_index),
                    "info",
                    &format!("归并：主张 {} -> 已有事实 {}", item.index + 1, fact_id.rsplit('_').next().unwrap_or(&fact_id)),
                );
                if !item.reason.is_empty() {
                    let _ = store.log(job_id, Some(step_index), "info", &format!("归并理由：{}", item.reason));
                }
            }
        }
    }
    // Deterministic fallback: lexical same_as is safe enough to attach without a model call.
    for (index, slot) in decisions.iter_mut().enumerate() {
        if slot.is_some() {
            continue;
        }
        if let Some(candidate) = candidates.get(index).and_then(|list| list.iter().find(|item| item.relation == "same_as")) {
            *slot = Some(candidate.id.clone());
        }
    }
    Ok(decisions)
}

async fn extract_claims(
    store: &ProductionStore,
    job_id: &str,
    ai: &AiConfig,
    title: &str,
    body: &str,
    max_claims: usize,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<Vec<TraceClaim>, String> {
    let user = format!(
        "标题：{title}\n\n正文：\n{}",
        body.chars().take(14_000).collect::<String>()
    );
    let result = checkpoint::cached_chat(
        store,
        job_id,
        "trace",
        "trace:claims",
        &prompts::trace_claims_system(max_claims as u32),
        &user,
        ai,
        usage,
        cost,
        token,
    )
    .await?;
    Ok(parse_claims(&result.content, body, max_claims))
}

fn parse_claims(content: &str, body: &str, max_claims: usize) -> Vec<TraceClaim> {
    let raw = clients::strip_fence(content);
    let Ok(value) = serde_json::from_str::<Value>(&raw) else { return Vec::new() };
    let items = value
        .as_array()
        .cloned()
        .or_else(|| value.get("items").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut claims = Vec::new();
    for item in items {
        let claim = item
            .get("claim")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if claim.chars().count() < 8 || !claim_grounded(&claim, body) || !seen.insert(claim.clone()) {
            continue;
        }
        let source_quote = item
            .get("source_quote")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let mut queries = item
            .get("queries")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|query| !query.is_empty())
                    .map(|query| query.chars().take(128).collect::<String>())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        queries.retain(|query| !query.is_empty());
        queries.truncate(MAX_QUERIES_PER_CLAIM);
        if queries.is_empty() {
            queries.push(claim.chars().take(60).collect());
        }
        let valid_quote = !source_quote.is_empty()
            && normalize_quote(body).contains(&normalize_quote(&source_quote));
        let source_quote = if valid_quote { source_quote } else { claim.clone() };
        let domain = match item.get("domain").and_then(Value::as_str).unwrap_or("") {
            "history" => "history",
            _ => "general",
        };
        claims.push(TraceClaim { claim, queries, source_quote, domain: domain.to_string() });
        if claims.len() >= max_claims {
            break;
        }
    }
    claims
}

fn claim_grounded(claim: &str, body: &str) -> bool {
    let body = body.to_lowercase();
    let digits = claim
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|value| value.len() >= 2)
        .collect::<Vec<_>>();
    if !digits.is_empty() {
        let found = digits.iter().filter(|number| body.contains(**number)).count();
        if found == 0 || (digits.len() >= 2 && found * 3 < digits.len()) {
            return false;
        }
    }
    let cjk = claim
        .chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .collect::<Vec<_>>();
    if cjk.len() < 4 {
        return true;
    }
    let pairs = cjk
        .windows(2)
        .map(|window| window.iter().collect::<String>())
        .collect::<Vec<_>>();
    if pairs.is_empty() {
        return false;
    }
    let found = pairs.iter().filter(|pair| body.contains(pair.as_str())).count();
    let ratio = found as f32 / pairs.len() as f32;
    if pairs.len() < 4 {
        ratio >= 0.6
    } else {
        ratio >= 0.35
    }
}

fn fallback_claims(body: &str, title: &str, max_claims: usize) -> Vec<TraceClaim> {
    let mut claims = Vec::new();
    let mut seen = HashSet::new();
    for line in body.lines() {
        let mut text = line.trim().trim_start_matches(['-', '*', '>', '#', ' ']).trim().to_string();
        if text.starts_with("**") {
            text = text.trim_matches('*').trim().to_string();
        }
        let chars = text.chars().count();
        if chars < 12 || chars > 220 {
            continue;
        }
        let has_signal = text.chars().any(|ch| ch.is_ascii_digit())
            || ["年", "美国", "政府", "选举", "战争", "亿美元", "万人"].iter().any(|word| text.contains(word));
        if !has_signal || !seen.insert(text.clone()) {
            continue;
        }
        claims.push(TraceClaim {
            queries: vec![text.chars().take(60).collect()],
            claim: text.clone(),
            source_quote: text,
            domain: "general".into(),
        });
        if claims.len() >= max_claims {
            break;
        }
    }
    if claims.is_empty() && !title.trim().is_empty() {
        claims.push(TraceClaim {
            claim: title.trim().to_string(),
            queries: vec![title.chars().take(60).collect()],
            source_quote: title.trim().to_string(),
            domain: "general".into(),
        });
    }
    claims
}

async fn search_claims(
    store: &ProductionStore,
    job_id: &str,
    step_index: usize,
    search: &SearchConfig,
    claims: &[TraceClaim],
    max_sources: usize,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<Vec<ClaimEvidence>, String> {
    let mut output = Vec::with_capacity(claims.len());
    let mut successful_queries = 0usize;
    let mut failed_queries = 0usize;
    let mut last_error = String::new();
    let mut total_hits = 0usize;
    for (claim_index, claim) in claims.iter().enumerate() {
        if token.is_cancelled() {
            return Err("已取消".into());
        }
        let mut seen = HashSet::new();
        let mut hits = Vec::new();
        let _ = store.log(
            job_id,
            Some(step_index),
            "info",
            &format!("主张 {} 检索词：{}", claim_index + 1, claim.queries.join(" | ")),
        );
        for query in claim.queries.iter().take(MAX_QUERIES_PER_CLAIM) {
            let query = query.chars().take(128).collect::<String>();
            if query.trim().is_empty() {
                continue;
            }
            let input_hash = checkpoint::hash_json(&json!({
                "pipelineVersion": checkpoint::PIPELINE_VERSION,
                "kind": "search",
                "provider": search.provider,
                "query": query,
                "max": max_sources,
            }))?;
            let unit_key = format!("trace:search:{}", &input_hash[..16]);
            let result = checkpoint::memo(
                store,
                job_id,
                "trace",
                &unit_key,
                "search",
                &input_hash,
                usage,
                cost,
                || async {
                    let hits = clients::search(search, &query, max_sources as u32, token).await?;
                    Ok((hits, Usage::default()))
                },
            )
            .await;
            match result {
                Ok(items) => {
                    successful_queries += 1;
                    for hit in items {
                        let key = hit.url.trim().to_string();
                        if key.is_empty() || seen.insert(key) {
                            hits.push(hit);
                        }
                    }
                }
                Err(error) => {
                    failed_queries += 1;
                    last_error = error.clone();
                    let _ = store.log(
                        job_id,
                        Some(step_index),
                        "warn",
                        &format!("主张 {} 的检索「{}」失败：{}", claim_index + 1, query, error),
                    );
                }
            }
            // DeepSeek Search 是账号级 5 QPS；串行 + 轻微间隔比并发更稳。
            if !token.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(180)).await;
            }
        }
        total_hits += hits.len();
        if claims.len() <= 12 {
            let _ = store.log(
                job_id,
                Some(step_index),
                "info",
                &format!(
                    "主张 {}/{} 检索到 {} 条候选",
                    claim_index + 1,
                    claims.len(),
                    hits.len()
                ),
            );
        }
        output.push(ClaimEvidence {
            claim: claim.claim.clone(),
            queries: claim.queries.clone(),
            hits: rank_hits(&claim.claim, hits, max_sources),
        });
    }
    if successful_queries == 0 && failed_queries > 0 {
        return Err(format!("联网检索全部失败：{last_error}"));
    }
    let _ = store.log(
        job_id,
        Some(step_index),
        "info",
        &format!(
            "检索汇总：成功 {} 次 / 失败 {} 次，去重后 {} 条候选",
            successful_queries, failed_queries, total_hits
        ),
    );
    Ok(output)
}

fn rank_hits(claim: &str, hits: Vec<SearchHit>, max_sources: usize) -> Vec<SearchHit> {
    let mut seen = HashSet::new();
    let mut ranked = hits
        .into_iter()
        .enumerate()
        .filter(|(_, hit)| !hit.url.trim().is_empty())
        .filter(|(_, hit)| seen.insert(normalize_url(&hit.url)))
        .map(|(index, hit)| {
            let score = authority_score(&hit.url) as usize * 30 + overlap_score(claim, &format!("{} {}", hit.title, hit.content));
            (score, index, hit)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked.into_iter().take(max_sources).map(|(_, _, hit)| hit).collect()
}

fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_lowercase()
}

fn authority_score(url: &str) -> i32 {
    let Ok(parsed) = Url::parse(url) else { return 0 };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if host.ends_with(".gov")
        || host.ends_with(".gov.cn")
        || host.ends_with(".edu")
        || host.ends_with(".edu.cn")
        || ["who.int", "un.org", "imf.org", "worldbank.org", "oecd.org", "europa.eu"]
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        return 4;
    }
    if [
        "reuters.com", "apnews.com", "bbc.com", "bbc.co.uk", "nytimes.com", "wsj.com",
        "ft.com", "xinhuanet.com", "news.cn", "people.com.cn", "thepaper.cn", "caixin.com",
        "chinadaily.com.cn", "cctv.com", "cnr.cn", "gmw.cn",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        return 3;
    }
    if [
        "sohu.com", "163.com", "sina.com.cn", "qq.com", "toutiao.com", "baijiahao.baidu.com",
        "zhihu.com", "weibo.com", "bilibili.com", "douyin.com", "csdn.net", "jianshu.com",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        return 1;
    }
    2
}

fn overlap_score(claim: &str, text: &str) -> usize {
    let claim = claim.to_lowercase();
    let text = text.to_lowercase();
    let chars = claim.chars().collect::<Vec<_>>();
    let mut score = 0usize;
    for window in chars.windows(2) {
        if window.iter().all(|ch| ch.is_alphanumeric()) {
            let pair = window.iter().collect::<String>();
            if text.contains(&pair) {
                score += 1;
            }
        }
    }
    for number in claim
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|value| value.len() >= 2)
    {
        if text.contains(number) {
            score += 6;
        }
    }
    score
}

async fn fetch_evidence(
    store: &ProductionStore,
    job_id: &str,
    step_index: usize,
    mut evidence: Vec<ClaimEvidence>,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Vec<ClaimEvidence> {
    let mut attempts = 0usize;
    let mut fetched = 0usize;
    let mut failed = 0usize;
    let deadline = Instant::now() + Duration::from_secs(180);
    // 第一轮每条主张抓 top1，第二轮在预算内补 top2；预算按“尝试次数”算，
    // 失败也消耗预算，避免慢站点把整条任务拖死。
    'passes: for pass in 0..2 {
        for (claim_index, item) in evidence.iter_mut().enumerate() {
            if attempts >= MAX_FETCHES || token.is_cancelled() || Instant::now() >= deadline {
                break 'passes;
            }
            let Some(hit) = item.hits.get_mut(pass) else { continue };
            let url = hit.url.trim().to_string();
            if url.is_empty() {
                continue;
            }
            attempts += 1;
            let input_hash = match checkpoint::hash_json(&json!({
                "pipelineVersion": checkpoint::PIPELINE_VERSION,
                "kind": "fetch",
                "url": url,
            })) {
                Ok(hash) => hash,
                Err(_) => continue,
            };
            let unit_key = format!("trace:fetch:{}", &input_hash[..16]);
            let result = checkpoint::memo(
                store,
                job_id,
                "trace",
                &unit_key,
                "fetch",
                &input_hash,
                usage,
                cost,
                || async {
                    match tokio::time::timeout(Duration::from_secs(70), web_fetch::fetch(&url)).await {
                        Ok(Ok(page)) => Ok((page, Usage::default())),
                        Ok(Err(error)) => Err(error),
                        Err(_) => Err("抓取来源超时（70 秒）".into()),
                    }
                },
            )
            .await;
            match result {
                Ok(page) => {
                    fetched += 1;
                    let excerpt = page.text.chars().take(MAX_FETCH_TEXT_CHARS).collect::<String>();
                    hit.content = format!(
                        "{}

[网页正文 {}
{}]",
                        hit.content,
                        page.url,
                        excerpt
                    );
                    if hit.title.trim().is_empty() && !page.title.trim().is_empty() {
                        hit.title = page.title;
                    }
                    let _ = store.log(
                        job_id,
                        Some(step_index),
                        "info",
                        &format!("抓取主张 {} 的来源成功：{}", claim_index + 1, page.url),
                    );
                }
                Err(error) => {
                    failed += 1;
                    let _ = store.log(
                        job_id,
                        Some(step_index),
                        "warn",
                        &format!("抓取主张 {} 的来源失败（{}）：{}", claim_index + 1, url, error),
                    );
                }
            }
        }
    }
    let _ = store.log(
        job_id,
        Some(step_index),
        "info",
        &format!("正文抓取：成功 {} / 尝试 {}（失败 {}）", fetched, attempts, failed),
    );
    evidence
}

async fn verify_claims(
    store: &ProductionStore,
    job_id: &str,
    step_index: usize,
    ai: &AiConfig,
    evidence: &[ClaimEvidence],
    max_sources: usize,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<Vec<TraceItem>, String> {
    let mut results: Vec<Option<TraceItem>> = vec![None; evidence.len()];
    let mut batch_index = 0usize;
    for batch_start in (0..evidence.len()).step_by(VERIFY_BATCH) {
        if token.is_cancelled() {
            return Err("已取消".into());
        }
        let batch = &evidence[batch_start..(batch_start + VERIFY_BATCH).min(evidence.len())];
        let with_hits = batch.iter().filter(|item| !item.hits.is_empty()).count();
        if with_hits == 0 {
            for (offset, item) in batch.iter().enumerate() {
                results[batch_start + offset] = Some(not_found(&item.claim));
            }
            continue;
        }
        let user = verification_user(batch, max_sources);
        let _ = store.log(
            job_id,
            Some(step_index),
            "info",
            &format!(
                "开始核查主张 {}-{}/{}",
                batch_start + 1,
                (batch_start + batch.len()).min(evidence.len()),
                evidence.len()
            ),
        );
        let result = tokio::time::timeout(
            VERIFY_TIMEOUT,
            checkpoint::cached_chat(
                store,
                job_id,
                "trace",
                &format!("trace:verify:{batch_index}"),
                prompts::trace_system(),
                &user,
                ai,
                usage,
                cost,
                token,
            ),
        )
        .await;
        let result = match result {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                let _ = store.log(
                    job_id,
                    Some(step_index),
                    "warn",
                    &format!("核查主张 {}-{} 失败：{error}", batch_start + 1, batch_start + batch.len()),
                );
                for (offset, item) in batch.iter().enumerate() {
                    results[batch_start + offset] = Some(not_found(&item.claim));
                }
                batch_index += 1;
                continue;
            }
            Err(_) => {
                let _ = store.log(
                    job_id,
                    Some(step_index),
                    "warn",
                    &format!(
                        "核查主张 {}-{} 超时（{}s），本批按未找到处理",
                        batch_start + 1,
                        batch_start + batch.len(),
                        VERIFY_TIMEOUT.as_secs()
                    ),
                );
                for (offset, item) in batch.iter().enumerate() {
                    results[batch_start + offset] = Some(not_found(&item.claim));
                }
                batch_index += 1;
                continue;
            }
        };
        let parsed = parse_trace_items(&result.content);
        for (offset, item) in batch.iter().enumerate() {
            let mut trace = parsed
                .iter()
                .find(|candidate| normalize_claim(&candidate.claim) == normalize_claim(&item.claim))
                .cloned()
                .unwrap_or_else(|| not_found(&item.claim));
            validate_trace_item(&mut trace, item, max_sources);
            results[batch_start + offset] = Some(trace);
        }
        batch_index += 1;
        let _ = store.log(
            job_id,
            Some(step_index),
            "info",
            &format!(
                "核查进度 {}/{}",
                (batch_start + batch.len()).min(evidence.len()),
                evidence.len()
            ),
        );
    }
    Ok(results.into_iter().map(|item| item.unwrap_or_else(|| not_found(""))).collect())
}

fn not_found(claim: &str) -> TraceItem {
    TraceItem {
        claim: claim.to_string(),
        verdict: "未找到外部出处".into(),
        relation: "context".into(),
        source: String::new(),
        quote: String::new(),
    }
}

fn verification_user(batch: &[ClaimEvidence], max_sources: usize) -> String {
    let mut out = String::from("请逐条核查以下主张，只使用每条主张下面列出的证据：\n\n");
    for (index, item) in batch.iter().enumerate() {
        out.push_str(&format!("主张 {}：{}\n证据：\n", index + 1, item.claim));
        if item.hits.is_empty() {
            out.push_str("（没有检索到证据）\n");
            continue;
        }
        for (hit_index, hit) in item.hits.iter().take(max_sources).enumerate() {
            let content = hit.content.chars().take(MAX_EVIDENCE_CHARS).collect::<String>();
            out.push_str(&format!(
                "[{}] 标题：{}\nURL：{}\n摘要：{}\n",
                hit_index + 1,
                hit.title,
                hit.url,
                content
            ));
        }
        out.push('\n');
    }
    out.push_str("只输出 JSON 数组，claim 必须原样回填。\n");
    out
}

fn parse_trace_items(content: &str) -> Vec<TraceItem> {
    let raw = clients::strip_fence(content);
    let Ok(value) = serde_json::from_str::<Value>(&raw) else { return Vec::new() };
    let items = value
        .as_array()
        .cloned()
        .or_else(|| value.get("items").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    items
        .iter()
        .filter_map(|item| {
            let claim = item.get("claim").and_then(Value::as_str)?.trim().to_string();
            if claim.is_empty() {
                return None;
            }
            Some(TraceItem {
                claim,
                verdict: item
                    .get("verdict")
                    .and_then(Value::as_str)
                    .unwrap_or("未核查")
                    .trim()
                    .to_string(),
                relation: item
                    .get("relation")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                source: item.get("source").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
                quote: item
                    .get("quote")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .chars()
                    .take(MAX_QUOTE_CHARS)
                    .collect(),
            })
        })
        .collect()
}

fn relation_for_verdict(verdict: &str) -> &'static str {
    if verdict.contains("可印证") || verdict.contains("非权威") {
        "supports"
    } else if verdict.contains("争议") {
        "contradicts"
    } else {
        "context"
    }
}

fn evidence_relation(item: &TraceItem) -> String {
    match item.relation.as_str() {
        "supports" | "contradicts" | "context" => item.relation.clone(),
        _ => relation_for_verdict(&item.verdict).into(),
    }
}

fn normalize_claim(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn normalize_quote(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn validate_trace_item(item: &mut TraceItem, evidence: &ClaimEvidence, max_sources: usize) {
    let source = item.source.trim().trim_end_matches('/').to_string();
    let matching = evidence
        .hits
        .iter()
        .take(max_sources)
        .find(|hit| normalize_url(&hit.url) == normalize_url(&source));
    if matching.is_none() {
        item.source.clear();
    }
    let quote = normalize_quote(&item.quote);
    let quote_ok = quote.chars().count() >= 8
        && evidence
            .hits
            .iter()
            .take(max_sources)
            .any(|hit| normalize_quote(&hit.content).contains(&quote));
    if !quote_ok {
        item.quote.clear();
    }
    if item.source.is_empty() && !item.quote.is_empty() {
        if let Some(hit) = evidence
            .hits
            .iter()
            .take(max_sources)
            .find(|hit| normalize_quote(&hit.content).contains(&normalize_quote(&item.quote)))
        {
            item.source = hit.url.clone();
        }
    }
    if (item.quote.is_empty() || item.source.is_empty())
        && !matches!(item.verdict.as_str(), "未找到外部出处" | "未核查")
    {
        // 没有可核验的 URL/引文时，不能保留“可印证/存在争议/仅非权威来源”；
        // 这些 verdict 都暗示“已经看到了证据”，但界面拿不出任何依据。
        item.verdict = "未找到外部出处".into();
    }
    if !matches!(
        item.verdict.as_str(),
        "外部可印证" | "存在争议" | "未找到外部出处" | "仅非权威来源" | "未核查"
    ) {
        item.verdict = "未核查".into();
    }
    if !matches!(item.relation.as_str(), "supports" | "contradicts" | "context") {
        item.relation = relation_for_verdict(&item.verdict).into();
    }
    if matches!(item.verdict.as_str(), "未找到外部出处" | "未核查") {
        item.relation = "context".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claims_limits_and_falls_back_to_claim_query() {
        let body = "2026年美国中期选举耗资超过120亿美元。正文还提到了一个没有检索词的事实主张。";
        let claims = parse_claims(
            r#"[{"claim":"2026年美国中期选举耗资超过120亿美元","queries":["2026 中期选举 120亿美元"]},{"claim":"太短"},{"claim":"没有检索词的事实主张内容","queries":[]},{"claim":"美国贸易战对企鹅加征10%关税","queries":["企鹅 关税 10%"]}]"#,
            body,
            10,
        );
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].queries, vec!["2026 中期选举 120亿美元"]);
        assert!(!claims[1].queries.is_empty());
    }

    #[test]
    fn parses_history_domain_and_defaults_general() {
        let body = "建安十三年，曹操率军南下。2026年美国中期选举耗资超过120亿美元。";
        let raw = r#"[
            {"claim":"建安十三年曹操率军南下。","source_quote":"建安十三年，曹操率军南下。","queries":["建安十三年 曹操"],"domain":"history"},
            {"claim":"2026年美国中期选举耗资超过120亿美元。","source_quote":"2026年美国中期选举耗资超过120亿美元。","queries":["2026 中期选举 120亿美元"]}
        ]"#;
        let claims = parse_claims(raw, body, 5);
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].domain, "history");
        assert_eq!(claims[1].domain, "general");
    }

    #[test]
    fn rejects_claim_not_grounded_in_body() {
        let body = "2026年美国中期选举耗资超过120亿美元。";
        assert!(claim_grounded("2026年美国中期选举耗资超过120亿美元", body));
        assert!(!claim_grounded("美国贸易战对企鹅加征10%关税", body));
        assert!(!claim_grounded("有美国议员称加州山火是太空激光装置导致的", body));
    }

    #[test]
    fn fallback_claims_only_keeps_signal_lines() {
        let claims = fallback_claims(
            "# 标题\n\n- 2026年中期选举耗资超过120亿美元。\n- 这是一句没有数字和实体信号的普通感慨。\n- 美国有五十多万儿童铅中毒。",
            "测试",
            5,
        );
        assert_eq!(claims.len(), 2);
        assert!(claims.iter().all(|claim| claim.queries.len() == 1));
    }

    #[test]
    fn ranks_authoritative_hit_over_self_media_for_same_overlap() {
        let hits = vec![
            SearchHit { title: "新闻".into(), url: "https://sohu.com/a".into(), content: "2026 中期选举 120亿美元".into() },
            SearchHit { title: "新闻".into(), url: "https://www.reuters.com/a".into(), content: "2026 中期选举 120亿美元".into() },
        ];
        let ranked = rank_hits("2026 中期选举耗资超过120亿美元", hits, 2);
        assert_eq!(ranked[0].url, "https://www.reuters.com/a");
    }

    #[test]
    fn rejects_hallucinated_quote_and_source() {
        let evidence = ClaimEvidence {
            claim: "2026年中期选举耗资超过120亿美元。".into(),
            queries: vec!["2026 中期选举 120亿美元".into()],
            hits: vec![SearchHit {
                title: "选举花费".into(),
                url: "https://example.com/a".into(),
                content: "2026年中期选举花费超过120亿美元。".into(),
            }],
        };
        let mut item = TraceItem {
            claim: evidence.claim.clone(),
            verdict: "外部可印证".into(),
            relation: "supports".into(),
            source: "https://example.com/fake".into(),
            quote: "2026年中期选举花费超过120亿美元。".into(),
        };
        validate_trace_item(&mut item, &evidence, 5);
        assert_eq!(item.source, "https://example.com/a");
        assert!(item.quote.contains("120亿美元"));

        let mut hallucinated = TraceItem {
            claim: evidence.claim.clone(),
            verdict: "外部可印证".into(),
            relation: "supports".into(),
            source: "https://example.com/a".into(),
            quote: "这句话不在证据里".into(),
        };
        validate_trace_item(&mut hallucinated, &evidence, 5);
        assert_eq!(hallucinated.verdict, "未找到外部出处");
        assert!(hallucinated.quote.is_empty());

        let mut unsupported = TraceItem {
            claim: evidence.claim.clone(),
            verdict: "存在争议".into(),
            relation: "contradicts".into(),
            source: String::new(),
            quote: String::new(),
        };
        validate_trace_item(&mut unsupported, &evidence, 5);
        assert_eq!(unsupported.verdict, "未找到外部出处");
    }
}
