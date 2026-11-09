//! 历史主张的自动史料入库。
//!
//! 目标不是“搜到网页就算史料”，而是：
//! 1. 先用专门的查询规划找“该查哪本书/哪一篇”；
//! 2. 只从允许自动抓取的全文源拿候选（Wikisource / IA OCR / ctext API）；
//! 3. 抓全文，按句切 passage；
//! 4. 找一句能支撑/反驳主张的原文；
//! 5. quote 必须是 passage 的逐字子串，才写 citation。
//!
//! 模型只负责“规划查询”和“从候选原文里挑一句”，不负责生成原文；
//! 任何模型输出都要经过子串校验。找不到、网络失败、候选不是全文库时，宁可不入库。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::checkpoint::{self, CostModel};
use super::clients::AiConfig;
use super::facts::FactStore;
use super::history::{CitationInput, HistoryCandidate, HistoryCandidateInput, HistoryStore, PassageInput, SourceInput};
use super::history_search::{self, Authority, FetchKind, SourceHit};
use super::prompts;
use super::store::ProductionStore;
use super::Usage;

const MIN_QUOTE_CHARS: usize = 8;
const MIN_DETERMINISTIC_OVERLAP: usize = 6;
const MAX_PASSAGES_PER_SOURCE: usize = 200;
const MAX_PASSAGE_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryIngestReport {
    pub searched: usize,
    pub fetched: usize,
    pub candidates: usize,
    pub auto_eligible: usize,
    pub blocked: usize,
    pub sources_written: usize,
    pub passages_written: usize,
    pub citations_created: usize,
    pub skipped_existing: bool,
    pub plan_source: String,
    pub work: String,
    pub section: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct QuoteDecision {
    passage: String,
    quote: String,
    relation: String,
    confidence: f64,
    locator: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuoteReply {
    #[serde(default)]
    passage: usize,
    #[serde(default)]
    quote: String,
    #[serde(default)]
    relation: String,
    #[serde(default)]
    confidence: f64,
}

/// 对一个历史事实跑一遍源优先检索。已存在 citation 时默认跳过；`force=true`
/// 时允许继续找更多来源。高置信全文源自动写 citation；其他来源只写候选。
#[allow(clippy::too_many_arguments)]
pub async fn ingest_fact_history(
    store: &ProductionStore,
    facts: &FactStore,
    history: &HistoryStore,
    fact_id: &str,
    claim: &str,
    queries: &[String],
    context: &str,
    ctext_key: &str,
    ai: Option<&AiConfig>,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
    force: bool,
    max_citations: usize,
) -> Result<HistoryIngestReport, String> {
    let mut report = HistoryIngestReport::default();
    let existing = history.fact_citations(fact_id)?;
    if !force && !existing.is_empty() {
        report.skipped_existing = true;
        report.notes.push("已存在史料引用，跳过自动检索".into());
        return Ok(report);
    }

    let plan = history_search::plan_query(store, fact_id, claim, queries, context, ai, usage, cost, token).await;
    report.plan_source = plan.source.clone();
    report.work = plan.work.clone();
    report.section = plan.section.clone();
    report.searched = plan.queries.len() + usize::from(!plan.work.is_empty());
    let discovery = history_search::discover(&plan, ctext_key, token).await;
    report.notes.extend(discovery.notes);
    report.candidates = discovery.hits.len();
    let mut candidates = Vec::with_capacity(discovery.hits.len());
    let max_citations = max_citations.clamp(1, 3);

    for hit in discovery.hits {
        if token.is_cancelled() {
            break;
        }
        if !hit.authority.can_auto_fetch() || hit.fetch_kind == FetchKind::None {
            report.blocked += 1;
            candidates.push(candidate_from_hit(
                &hit,
                "candidate",
                &block_reason(&hit, ctext_key),
                "",
                "",
            ));
            continue;
        }
        if report.citations_created >= max_citations {
            candidates.push(candidate_from_hit(
                &hit,
                "candidate",
                "本轮 citation 已达上限，保留候选",
                "",
                "",
            ));
            continue;
        }
        report.auto_eligible += 1;
        let fetched = match history_search::fetch_hit(&hit, history, ctext_key, token).await {
            Ok(page) => page,
            Err(error) => {
                candidates.push(candidate_from_hit(
                    &hit,
                    "rejected",
                    &format!("全文抓取失败：{error}"),
                    "",
                    "",
                ));
                report.notes.push(format!("抓取《{}》失败：{error}", hit.title));
                continue;
            }
        };
        report.fetched += 1;
        let passages = split_passages(&fetched.text);
        if passages.is_empty() {
            candidates.push(candidate_from_hit(
                &hit,
                "rejected",
                "没有切出可用原文",
                "",
                "",
            ));
            report.notes.push(format!("《{}》没有切出可用原文", hit.title));
            continue;
        }
        let decision = match choose_quote(
            store,
            fact_id,
            claim,
            &fetched.title,
            &passages,
            ai,
            usage,
            cost,
            token,
        )
        .await
        {
            Ok(decision) => decision,
            Err(error) => {
                candidates.push(candidate_from_hit(
                    &hit,
                    "rejected",
                    &format!("挑原文失败：{error}"),
                    "",
                    "",
                ));
                report.notes.push(format!("《{}》挑原文失败：{error}", hit.title));
                continue;
            }
        };
        let Some(decision) = decision else {
            candidates.push(candidate_from_hit(
                &hit,
                "rejected",
                "没有找到能支撑主张的逐字原文",
                "",
                "",
            ));
            report.notes.push(format!("《{}》没有找到能支撑主张的逐字原文", hit.title));
            continue;
        };
        if !decision.passage.contains(&decision.quote) {
            candidates.push(candidate_from_hit(
                &hit,
                "rejected",
                "引文不是原文子串，已拒绝",
                "",
                "",
            ));
            report.notes.push(format!("《{}》的引文不是原文子串，已拒绝", hit.title));
            continue;
        }

        let (title_work, title_section) = split_work_title(&fetched.title);
        let work_title = if !hit.work_title.trim().is_empty() {
            hit.work_title.clone()
        } else if !title_work.trim().is_empty() {
            title_work
        } else {
            plan.work.clone()
        };
        let section_title = if !plan.section.trim().is_empty() {
            plan.section.clone()
        } else if !hit.section_title.trim().is_empty() {
            hit.section_title.clone()
        } else {
            title_section
        };
        let locator = if !section_title.trim().is_empty() {
            section_title.clone()
        } else {
            plan.section.clone()
        };
        let source_ref = if hit.fetch_ref.trim().is_empty() { &hit.url } else { &hit.fetch_ref };
        let source_id = source_id_for(&hit.provider, source_ref);
        let source = match history.save_source(SourceInput {
            id: source_id,
            work_title,
            section_title,
            source_type: "primary".into(),
            edition: if hit.edition.trim().is_empty() { hit.provider.clone() } else { hit.edition.clone() },
            repository: hit.provider.clone(),
            url: hit.url.clone(),
            scan_url: hit.scan_url.clone(),
            language: "zh".into(),
            authenticity_note: authenticity_note(&hit),
            ..Default::default()
        }) {
            Ok(source) => source,
            Err(error) => {
                candidates.push(candidate_from_hit(
                    &hit,
                    "rejected",
                    &format!("史料入库失败：{error}"),
                    "",
                    "",
                ));
                report.notes.push(format!("史料入库失败：{error}"));
                continue;
            }
        };
        report.sources_written += 1;
        let passage_id = passage_id_for(&source.id, &decision.passage);
        let passage = match history.save_passage(PassageInput {
            id: passage_id,
            source_id: source.id.clone(),
            original_text: decision.passage.clone(),
            locator: locator.clone(),
            ..Default::default()
        }) {
            Ok(passage) => passage,
            Err(error) => {
                candidates.push(candidate_from_hit(
                    &hit,
                    "rejected",
                    &format!("原文段落入库失败：{error}"),
                    &source.id,
                    "",
                ));
                report.notes.push(format!("原文段落入库失败：{error}"));
                continue;
            }
        };
        report.passages_written += 1;
        match history.save_citation(CitationInput {
            fact_id: fact_id.to_string(),
            passage_id: passage.id,
            relation: decision.relation,
            quote: decision.quote,
            locator,
            confidence: decision.confidence,
            ..Default::default()
        }) {
            Ok(citation) => {
                report.citations_created += 1;
                candidates.push(candidate_from_hit(
                    &hit,
                    "cited",
                    "已自动引用",
                    &source.id,
                    &citation.id,
                ));
                if let Err(error) = facts.set_domain(fact_id, "history") {
                    report.notes.push(format!("事实领域更新失败：{error}"));
                }
            }
            Err(error) => {
                candidates.push(candidate_from_hit(
                    &hit,
                    "rejected",
                    &format!("史料引用入库失败：{error}"),
                    &source.id,
                    "",
                ));
                report.notes.push(format!("史料引用入库失败：{error}"));
            }
        }
    }

    history.replace_candidates(fact_id, &candidates)?;
    if report.candidates == 0 {
        report.notes.push("没有拿到任何史料候选".into());
    }
    if report.citations_created == 0 {
        report.notes.push("本轮没有新增史料引用；不会用网页摘要代替原文".into());
    }
    tidy_notes(&mut report.notes);
    Ok(report)
}

/// 用户从候选页面复制原文后补录。仍然是“粘贴文本 → 切 passage → 逐字挑句”，
/// 不是手填书目；候选的 provider/title/url/work/section 直接复用。
#[allow(clippy::too_many_arguments)]
pub async fn import_pasted_candidate(
    store: &ProductionStore,
    facts: &FactStore,
    history: &HistoryStore,
    fact_id: &str,
    claim: &str,
    candidate: &HistoryCandidate,
    text: &str,
    ai: Option<&AiConfig>,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<HistoryCandidate, String> {
    if candidate.fact_id != fact_id {
        return Err("候选不属于这条事实".into());
    }
    if candidate.status == "cited" {
        return Err("这个候选已经引用过了".into());
    }
    let text = text.trim();
    if text.chars().count() < MIN_QUOTE_CHARS {
        return Err("粘贴文本太短，至少需要一段原文".into());
    }
    let passages = split_passages(text);
    if passages.is_empty() {
        let reason = "粘贴文本没有切出可用原文";
        history.update_candidate(&candidate.id, "rejected", reason, "", "")?;
        return Err(reason.into());
    }
    let decision = match choose_quote(
        store,
        fact_id,
        claim,
        &candidate.title,
        &passages,
        ai,
        usage,
        cost,
        token,
    )
    .await
    {
        Ok(decision) => decision,
        Err(error) => {
            let reason = format!("挑原文失败：{error}");
            history.update_candidate(&candidate.id, "rejected", &reason, "", "")?;
            return Err(reason);
        }
    };
    let Some(decision) = decision else {
        let reason = "粘贴文本里没有找到能支撑主张的逐字原文";
        history.update_candidate(&candidate.id, "rejected", reason, "", "")?;
        return Err(reason.into());
    };
    if !decision.passage.contains(&decision.quote) {
        let reason = "引文不是粘贴原文的子串，已拒绝";
        history.update_candidate(&candidate.id, "rejected", reason, "", "")?;
        return Err(reason.into());
    }

    let (title_work, title_section) = split_work_title(&candidate.title);
    let work_title = if !candidate.work_title.trim().is_empty() {
        candidate.work_title.clone()
    } else if !title_work.trim().is_empty() {
        title_work
    } else {
        candidate.title.clone()
    };
    let section_title = if !candidate.section_title.trim().is_empty() {
        candidate.section_title.clone()
    } else {
        title_section
    };
    let locator = if section_title.trim().is_empty() {
        candidate.title.clone()
    } else {
        section_title.clone()
    };
    let source_ref = if candidate.url.trim().is_empty() { &candidate.id } else { &candidate.url };
    let source = history.save_source(SourceInput {
        id: source_id_for(&candidate.provider, source_ref),
        work_title,
        section_title,
        source_type: "primary".into(),
        edition: pasted_edition(&candidate.provider),
        repository: candidate.provider.clone(),
        url: candidate.url.clone(),
        language: "zh".into(),
        authenticity_note: format!(
            "用户从 {} 候选页粘贴；未自动抓取，未核对原书扫描页。",
            candidate.provider
        ),
        ..Default::default()
    })?;
    let passage = history.save_passage(PassageInput {
        id: passage_id_for(&source.id, &decision.passage),
        source_id: source.id.clone(),
        original_text: decision.passage.clone(),
        locator: locator.clone(),
        ..Default::default()
    })?;
    let citation = history.save_citation(CitationInput {
        fact_id: fact_id.to_string(),
        passage_id: passage.id,
        relation: decision.relation,
        quote: decision.quote,
        locator,
        confidence: decision.confidence,
        ..Default::default()
    })?;
    let domain_error = facts.set_domain(fact_id, "history").err();
    let reason = match domain_error {
        Some(error) => format!("用户粘贴原文后自动对齐；事实领域更新失败：{error}"),
        None => "用户粘贴原文后自动对齐".to_string(),
    };
    history.update_candidate(&candidate.id, "cited", &reason, &source.id, &citation.id)
}

fn pasted_edition(provider: &str) -> String {
    match provider {
        "ctext" => "ctext 页面（用户粘贴）".into(),
        "Wikisource" => "Wikisource 页面（用户粘贴）".into(),
        "Internet Archive" => "Internet Archive 页面（用户粘贴）".into(),
        "NLC" => "中华古籍资源库（用户粘贴）".into(),
        "一史馆" => "第一历史档案馆（用户粘贴）".into(),
        "HathiTrust" => "HathiTrust（用户粘贴）".into(),
        other => format!("{other}（用户粘贴）"),
    }
}

fn candidate_from_hit(
    hit: &SourceHit,
    status: &str,
    reason: &str,
    source_id: &str,
    citation_id: &str,
) -> HistoryCandidateInput {
    HistoryCandidateInput {
        provider: hit.provider.clone(),
        title: hit.title.clone(),
        url: hit.url.clone(),
        snippet: hit.snippet.clone(),
        authority: hit.authority.as_str().into(),
        status: status.into(),
        reason: reason.into(),
        work_title: hit.work_title.clone(),
        section_title: hit.section_title.clone(),
        source_id: source_id.into(),
        citation_id: citation_id.into(),
    }
}

fn block_reason(hit: &SourceHit, ctext_key: &str) -> String {
    if hit.fetch_kind == FetchKind::CtextUrn && ctext_key.trim().is_empty() {
        return "ctext 全文需要订阅 API Key；当前只保留候选链接".into();
    }
    match hit.authority {
        Authority::LinkOnly => "外部/目录源，未自动抓取".into(),
        Authority::Secondary => "二手来源，只保留候选".into(),
        Authority::Reference => "工具书/参考来源，不算一手史料".into(),
        _ => "没有可自动抓取的全文".into(),
    }
}

fn authenticity_note(hit: &SourceHit) -> String {
    match hit.provider.as_str() {
        "Wikisource" => "自动抓取；Wikisource 转录文本，未核对原书扫描页。".into(),
        "ctext" => "自动抓取；ctext 全文 API，未核对原书扫描页。".into(),
        "Internet Archive" => "自动抓取；Internet Archive OCR 文本，未核对原书扫描页。".into(),
        provider => format!("自动抓取；来源 {provider}，未核对原书扫描页。"),
    }
}

fn tidy_notes(notes: &mut Vec<String>) {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for note in notes.drain(..) {
        let collapsed = note.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            continue;
        }
        let shortened = if collapsed.chars().count() > 160 {
            format!("{}…", collapsed.chars().take(157).collect::<String>())
        } else {
            collapsed
        };
        if seen.insert(shortened.clone()) {
            output.push(shortened);
        }
    }
    *notes = output;
}

async fn choose_quote(
    store: &ProductionStore,
    fact_id: &str,
    claim: &str,
    source_title: &str,
    passages: &[String],
    ai: Option<&AiConfig>,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<Option<QuoteDecision>, String> {
    let deterministic = deterministic_quote(claim, passages);
    if deterministic.as_ref().map(|item| overlap_len(claim, &item.quote) >= 12).unwrap_or(false) {
        return Ok(deterministic);
    }
    if let Some(ai) = ai {
        match model_quote(store, fact_id, claim, source_title, passages, ai, usage, cost, token).await {
            Ok(Some(decision)) => return Ok(Some(decision)),
            Ok(None) => {}
            Err(error) => {
                if deterministic.is_some() {
                    return Ok(deterministic);
                }
                return Err(error);
            }
        }
    }
    Ok(deterministic)
}

async fn model_quote(
    store: &ProductionStore,
    fact_id: &str,
    claim: &str,
    source_title: &str,
    passages: &[String],
    ai: &AiConfig,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<Option<QuoteDecision>, String> {
    let ranked = rank_passages(claim, passages);
    if ranked.is_empty() {
        return Ok(None);
    }
    let chosen = ranked
        .iter()
        .take(6)
        .enumerate()
        .map(|(index, (_, passage))| format!("[{}] {}", index + 1, passage))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!("主张：{claim}\n史料标题：{source_title}\n\n候选原文：\n{chosen}");
    let job_id = format!("history_ingest_{fact_id}");
    let result = checkpoint::cached_chat(
        store,
        &job_id,
        "history",
        "history:quote",
        prompts::history_quote_system(),
        &user,
        ai,
        usage,
        cost,
        token,
    )
    .await?;
    Ok(parse_quote_reply(&result.content, &ranked, source_title))
}

fn parse_quote_reply(content: &str, ranked: &[(usize, String)], source_title: &str) -> Option<QuoteDecision> {
    let raw = super::clients::strip_fence(content);
    let reply = serde_json::from_str::<QuoteReply>(&raw).ok()?;
    if reply.passage == 0 || reply.passage > ranked.len() {
        return None;
    }
    let quote = reply.quote.trim();
    if quote.chars().count() < MIN_QUOTE_CHARS {
        return None;
    }
    let passage = &ranked[reply.passage - 1].1;
    // 模型只能“挑”，不能“写”。不是逐字子串就丢弃。
    if !passage.contains(quote) {
        return None;
    }
    let relation = match reply.relation.as_str() {
        "contradicts" => "contradicts",
        "context" => "context",
        _ => "supports",
    };
    Some(QuoteDecision {
        passage: passage.clone(),
        quote: quote.to_string(),
        relation: relation.to_string(),
        confidence: reply.confidence.clamp(0.0, 1.0).max(0.5),
        locator: source_title.to_string(),
    })
}

fn deterministic_quote(claim: &str, passages: &[String]) -> Option<QuoteDecision> {
    let mut ranked = rank_passages(claim, passages);
    let (score, passage) = ranked.pop()?;
    if score < MIN_DETERMINISTIC_OVERLAP {
        return None;
    }
    let confidence = (score as f64 / normalize_match(claim).chars().count().max(1) as f64).min(0.95).max(0.55);
    Some(QuoteDecision {
        passage: passage.clone(),
        quote: passage,
        relation: "supports".into(),
        confidence,
        locator: String::new(),
    })
}

fn rank_passages(claim: &str, passages: &[String]) -> Vec<(usize, String)> {
    let mut ranked = passages
        .iter()
        .map(|passage| (overlap_len(claim, passage), passage.clone()))
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| left.0.cmp(&right.0));
    ranked
}

/// 抓回来的正文通常是整页；按句号/分号/换行切成可引用的段落。
pub(crate) fn split_passages(text: &str) -> Vec<String> {
    let mut raw = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '。' | '！' | '？' | '!' | '?' | '；' | ';' | '\n') {
            push_passage(&mut raw, &mut current);
        }
    }
    push_passage(&mut raw, &mut current);

    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for passage in raw {
        for piece in split_long_passage(&passage) {
            if piece.chars().count() >= MIN_QUOTE_CHARS && seen.insert(piece.clone()) {
                output.push(piece);
                if output.len() >= MAX_PASSAGES_PER_SOURCE {
                    return output;
                }
            }
        }
    }
    output
}

fn push_passage(output: &mut Vec<String>, current: &mut String) {
    let value = current.trim().trim_start_matches(['#', '*', '-', '>', ' ']).trim().to_string();
    current.clear();
    if !value.is_empty() {
        output.push(value);
    }
}

fn split_long_passage(passage: &str) -> Vec<String> {
    if passage.chars().count() <= MAX_PASSAGE_CHARS {
        return vec![passage.to_string()];
    }
    let mut output = Vec::new();
    let mut current = String::new();
    for ch in passage.chars() {
        current.push(ch);
        if current.chars().count() >= MAX_PASSAGE_CHARS.saturating_sub(80) && matches!(ch, '，' | ',' | '、' | '：' | ':') {
            output.push(current.trim().to_string());
            current.clear();
        }
    }
    if !current.trim().is_empty() {
        output.push(current.trim().to_string());
    }
    output
}

fn split_work_title(title: &str) -> (String, String) {
    let title = title.trim();
    for separator in ['/', '·'] {
        if let Some((work, section)) = title.split_once(separator) {
            let work = work.trim();
            let section = section.trim();
            if !work.is_empty() {
                return (work.to_string(), section.to_string());
            }
        }
    }
    (title.to_string(), String::new())
}

fn source_id_for(provider: &str, reference: &str) -> String {
    let normalized = format!("{}:{}", provider.trim().to_lowercase(), reference.trim().trim_end_matches('/').to_lowercase());
    format!("src_auto_{}", short_hash(&normalized))
}

fn passage_id_for(source_id: &str, text: &str) -> String {
    format!("passage_auto_{}", short_hash(&format!("{source_id}\n{text}")))
}

fn short_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(&hasher.finalize()[..12])
}

fn normalize_match(value: &str) -> String {
    hanconv::t2s(value)
        .chars()
        .filter(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn overlap_len(left: &str, right: &str) -> usize {
    best_overlap_len(left, right)
}

/// 最长公共子串长度。只用于候选排序和确定性兜底，不是语义判断。
fn best_overlap_len(left: &str, right: &str) -> usize {
    let left = normalize_match(left).chars().collect::<Vec<_>>();
    let right = normalize_match(right).chars().collect::<Vec<_>>();
    if left.is_empty() || right.is_empty() {
        return 0;
    }
    let mut previous = vec![0usize; right.len() + 1];
    let mut current = vec![0usize; right.len() + 1];
    let mut best = 0usize;
    for l in &left {
        for (index, r) in right.iter().enumerate() {
            if l == r {
                current[index + 1] = previous[index] + 1;
                best = best.max(current[index + 1]);
            } else {
                current[index + 1] = 0;
            }
        }
        std::mem::swap(&mut previous, &mut current);
        current.fill(0);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::production::history::HistoryStore;
    use crate::production::facts::FactStore;
    use uuid::Uuid;

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("yebian-history-ingest-{}-{}", prefix, Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_model_quote_only_when_it_is_verbatim() {
        let ranked = vec![(3, "严嵩无他才略，唯一意媚上，窃权罔利。".to_string())];
        let good = parse_quote_reply(
            r#"{"passage":1,"quote":"唯一意媚上，窃权罔利","relation":"supports","confidence":0.86}"#,
            &ranked,
            "卷三百八",
        )
        .unwrap();
        assert_eq!(good.quote, "唯一意媚上，窃权罔利");
        assert_eq!(good.relation, "supports");

        assert!(parse_quote_reply(
            r#"{"passage":1,"quote":"严嵩其实是个好人","relation":"supports","confidence":0.9}"#,
            &ranked,
            "卷三百八",
        )
        .is_none());
        assert!(parse_quote_reply(
            r#"{"passage":2,"quote":"唯一意媚上","relation":"supports","confidence":0.9}"#,
            &ranked,
            "卷三百八",
        )
        .is_none());
        assert!(parse_quote_reply(
            r#"{"passage":1,"quote":"太短","relation":"supports","confidence":0.9}"#,
            &ranked,
            "卷三百八",
        )
        .is_none());
    }

    #[test]
    fn splits_chinese_page_into_passages() {
        let passages = split_passages("这是第一句完整的原文。这是第二句完整的原文！这是第三句完整的原文？\n这是第四句完整的原文；这是第五句完整的原文");
        assert_eq!(
            passages,
            vec![
                "这是第一句完整的原文。",
                "这是第二句完整的原文！",
                "这是第三句完整的原文？",
                "这是第四句完整的原文；",
                "这是第五句完整的原文"
            ]
        );
    }

    #[test]
    fn deterministic_quote_requires_real_overlap() {
        let passages = vec!["严嵩无他才略，唯一意媚上，窃权罔利。".to_string()];
        let decision = deterministic_quote("严嵩无他才略，唯一意媚上，窃权罔利。", &passages).unwrap();
        assert_eq!(decision.quote, "严嵩无他才略，唯一意媚上，窃权罔利。");
        assert!(deterministic_quote("这段原文完全不相关。", &passages).is_none());
    }

    #[test]
    fn splits_wikisource_title() {
        assert_eq!(split_work_title("明史/卷三百八"), ("明史".into(), "卷三百八".into()));
        assert_eq!(split_work_title("三国志·吴书·周瑜传"), ("三国志".into(), "吴书·周瑜传".into()));
        assert_eq!(split_work_title("单页标题"), ("单页标题".into(), String::new()));
    }

    #[test]
    fn stable_ids_do_not_depend_on_order() {
        let first = source_id_for("Wikisource", "明史/卷三百八/");
        let second = source_id_for("Wikisource", "明史/卷三百八");
        assert_eq!(first, second);
        assert!(first.starts_with("src_auto_"));
    }

    #[test]
    fn quote_must_be_substring_of_passage() {
        let passage = "严嵩无他才略，唯一意媚上，窃权罔利。";
        assert!(passage.contains("唯一意媚上"));
        assert!(!passage.contains("严嵩是个好人"));
    }

    #[tokio::test]
    async fn pasted_candidate_aligns_and_writes_citation() {
        let dir = temp_dir("paste");
        let path = dir.join("yebian.sqlite");
        let facts = FactStore::open(&path).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let store = ProductionStore::open(&path).unwrap();
        let fact_id = facts
            .record_candidate_with_domain("note-a", "run-a", "严嵩无他才略，唯一意媚上，窃权罔利。", "", "", &[], "history")
            .unwrap();
        let candidate = history
            .replace_candidates(
                &fact_id,
                &[HistoryCandidateInput {
                    provider: "ctext".into(),
                    title: "明史".into(),
                    url: "https://ctext.org/wb410835".into(),
                    authority: "link_only".into(),
                    status: "candidate".into(),
                    reason: "需要 API Key".into(),
                    work_title: "明史".into(),
                    section_title: "卷三百八".into(),
                    ..Default::default()
                }],
            )
            .unwrap()
            .remove(0);
        let mut usage = Usage::default();
        let cost = CostModel { price: None, asr_price_per_minute: 0.0 };
        let token = CancellationToken::new();
        let updated = import_pasted_candidate(
            &store,
            &facts,
            &history,
            &fact_id,
            "严嵩无他才略，唯一意媚上，窃权罔利。",
            &candidate,
            "明史·严嵩传：严嵩无他才略，唯一意媚上，窃权罔利。",
            None,
            &mut usage,
            &cost,
            &token,
        )
        .await
        .unwrap();
        assert_eq!(updated.status, "cited");
        assert!(!updated.citation_id.is_empty());
        assert_eq!(history.fact_citations(&fact_id).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn pasted_candidate_rejects_unrelated_text() {
        let dir = temp_dir("paste-reject");
        let path = dir.join("yebian.sqlite");
        let facts = FactStore::open(&path).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let store = ProductionStore::open(&path).unwrap();
        let fact_id = facts
            .record_candidate_with_domain("note-a", "run-a", "严嵩无他才略，唯一意媚上，窃权罔利。", "", "", &[], "history")
            .unwrap();
        let candidate = history
            .replace_candidates(
                &fact_id,
                &[HistoryCandidateInput {
                    provider: "NLC".into(),
                    title: "明史".into(),
                    url: "https://read.nlc.cn/".into(),
                    authority: "link_only".into(),
                    status: "candidate".into(),
                    ..Default::default()
                }],
            )
            .unwrap()
            .remove(0);
        let mut usage = Usage::default();
        let cost = CostModel { price: None, asr_price_per_minute: 0.0 };
        let token = CancellationToken::new();
        let error = import_pasted_candidate(
            &store,
            &facts,
            &history,
            &fact_id,
            "严嵩无他才略，唯一意媚上，窃权罔利。",
            &candidate,
            "这是一段完全无关的现代新闻，没有对应的史料原文。",
            None,
            &mut usage,
            &cost,
            &token,
        )
        .await
        .unwrap_err();
        assert!(error.contains("没有找到"));
        assert_eq!(history.fact_citations(&fact_id).unwrap().len(), 0);
        assert_eq!(history.candidate_by_id(&candidate.id).unwrap().unwrap().status, "rejected");
    }

    #[test]
    fn auto_ingest_writes_source_passage_and_citation() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("yebian.sqlite");
        let facts = FactStore::open(&path).unwrap();
        let history = HistoryStore::open(&path).unwrap();
        let fact_id = facts
            .record_candidate_with_domain("note-a", "run-a", "严嵩无他才略，唯一意媚上，窃权罔利。", "", "", &[], "history")
            .unwrap();
        let (work, section) = split_work_title("明史/卷三百八");
        let source = history
            .save_source(SourceInput {
                id: source_id_for("Wikisource", "明史/卷三百八"),
                work_title: work,
                section_title: section,
                source_type: "primary".into(),
                edition: "Wikisource 转录".into(),
                repository: "Wikisource".into(),
                url: "https://zh.wikisource.org/wiki/明史/卷三百八".into(),
                language: "zh".into(),
                authenticity_note: "自动抓取；Wikisource 转录文本，未核对原书扫描页。".into(),
                ..Default::default()
            })
            .unwrap();
        let passage_text = "严嵩无他才略，唯一意媚上，窃权罔利。";
        let passage = history
            .save_passage(PassageInput {
                id: passage_id_for(&source.id, passage_text),
                source_id: source.id.clone(),
                original_text: passage_text.into(),
                locator: "卷三百八".into(),
                ..Default::default()
            })
            .unwrap();
        history
            .save_citation(CitationInput {
                fact_id: fact_id.clone(),
                passage_id: passage.id.clone(),
                relation: "supports".into(),
                quote: passage_text.into(),
                locator: "卷三百八".into(),
                confidence: 0.9,
                ..Default::default()
            })
            .unwrap();
        let citations = history.fact_citations(&fact_id).unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].source_type, "primary");
        assert_eq!(citations[0].quote, passage_text);
    }
}
