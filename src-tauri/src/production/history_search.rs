//! 史料检索系统：查询规划、源适配、候选排序与全文抓取。
//!
//! 它和通用网页搜索分开：
//! - 先规划“查哪本书/哪一篇”，而不是拿整条主张搜网页；
//! - 只把明确允许自动抓取的全文源（Wikisource / IA OCR / ctext API）当 citation 候选；
//! - ctext 网页有反爬声明，只走它的官方 JSON API（需要 API Key），不抓网页；
//! - NLC / 一史馆 / HathiTrust 只作为外链候选，不自动引用；
//! - 所有候选都记录 authority、provider、reason，便于审计。

use std::collections::HashSet;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::checkpoint::{self, CostModel};
use super::clients::AiConfig;
use super::history::{CachedPage, HistoryStore};
use super::prompts;
use super::source_search;
use super::store::ProductionStore;
use super::web_fetch;
use super::Usage;

const MAX_QUERIES: usize = 4;
const MAX_HITS: usize = 12;
const CACHE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MAX_TEXT_CHARS: usize = 20_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQueryPlan {
    pub work: String,
    pub section: String,
    pub people: Vec<String>,
    pub era: String,
    pub queries: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    LinkOnly,
    Reference,
    Secondary,
    Ocr,
    Transcription,
    Primary,
}

impl Authority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LinkOnly => "link_only",
            Self::Reference => "reference",
            Self::Secondary => "secondary",
            Self::Ocr => "ocr",
            Self::Transcription => "transcription",
            Self::Primary => "primary",
        }
    }

    fn weight(self) -> i32 {
        match self {
            Self::LinkOnly => 1,
            Self::Reference => 2,
            Self::Secondary => 3,
            Self::Ocr => 5,
            Self::Transcription => 5,
            Self::Primary => 6,
        }
    }

    pub fn can_auto_fetch(self) -> bool {
        matches!(self, Self::Primary | Self::Ocr | Self::Transcription)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchKind {
    WikisourcePage,
    CtextUrn,
    InternetArchiveItem,
    None,
}

#[derive(Debug, Clone)]
pub struct SourceHit {
    pub provider: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub work_title: String,
    pub section_title: String,
    pub authority: Authority,
    pub fetch_kind: FetchKind,
    pub fetch_ref: String,
    pub scan_url: String,
    pub edition: String,
}

#[derive(Debug, Clone)]
pub struct FetchedText {
    pub title: String,
    pub text: String,
    pub url: String,
}

#[derive(Debug, Clone, Default)]
pub struct Discovery {
    pub hits: Vec<SourceHit>,
    pub notes: Vec<String>,
}

struct RegistryWork {
    title: &'static str,
    aliases: &'static [&'static str],
    author: &'static str,
    era: &'static str,
}

const REGISTRY: &[RegistryWork] = &[
    RegistryWork { title: "明史", aliases: &["明史", "明书"], author: "张廷玉等", era: "清" },
    RegistryWork { title: "明实录", aliases: &["明实录", "大明实录"], author: "明官修", era: "明" },
    RegistryWork { title: "明史纪事本末", aliases: &["明史纪事本末", "明史紀事本末"], author: "谷应泰", era: "清" },
    RegistryWork { title: "国榷", aliases: &["国榷", "國榷"], author: "谈迁", era: "清" },
    RegistryWork { title: "清史稿", aliases: &["清史稿"], author: "赵尔巽等", era: "民国" },
    RegistryWork { title: "清实录", aliases: &["清实录", "大清实录"], author: "清官修", era: "清" },
    RegistryWork { title: "史记", aliases: &["史记", "史記"], author: "司马迁", era: "西汉" },
    RegistryWork { title: "汉书", aliases: &["汉书", "漢書"], author: "班固", era: "东汉" },
    RegistryWork { title: "后汉书", aliases: &["后汉书", "後漢書"], author: "范晔", era: "南朝宋" },
    RegistryWork { title: "三国志", aliases: &["三国志", "三國志"], author: "陈寿", era: "西晋" },
    RegistryWork { title: "资治通鉴", aliases: &["资治通鉴", "資治通鑑", "通鉴"], author: "司马光", era: "北宋" },
    RegistryWork { title: "续资治通鉴长编", aliases: &["续资治通鉴长编", "長編"], author: "李焘", era: "南宋" },
    RegistryWork { title: "宋史", aliases: &["宋史"], author: "脱脱等", era: "元" },
    RegistryWork { title: "元史", aliases: &["元史"], author: "宋濂等", era: "明" },
    RegistryWork { title: "旧唐书", aliases: &["旧唐书", "舊唐書"], author: "刘昫等", era: "后晋" },
    RegistryWork { title: "新唐书", aliases: &["新唐书", "新唐書"], author: "欧阳修等", era: "北宋" },
    RegistryWork { title: "明会典", aliases: &["明会典", "大明会典"], author: "明官修", era: "明" },
    RegistryWork { title: "日知录", aliases: &["日知录", "日知錄"], author: "顾炎武", era: "清" },
];

/// 先让模型规划“查哪本书/哪一篇”；失败时用 registry + 现有 queries 兜底。
pub async fn plan_query(
    store: &ProductionStore,
    fact_id: &str,
    claim: &str,
    queries: &[String],
    context: &str,
    ai: Option<&AiConfig>,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> HistoryQueryPlan {
    let fallback = fallback_plan(claim, queries, context);
    let Some(ai) = ai else { return fallback };
    let user = format!(
        "主张：{claim}\n\n笔记上下文：\n{}",
        context.chars().take(2400).collect::<String>()
    );
    let result = checkpoint::cached_chat(
        store,
        &format!("history_plan_{fact_id}"),
        "history",
        "history:plan",
        prompts::history_query_system(),
        &user,
        ai,
        usage,
        cost,
        token,
    )
    .await;
    let Ok(result) = result else { return fallback };
    let Some(mut plan) = parse_plan(&result.content) else { return fallback };
    plan.source = "model".into();
    plan.queries = merge_queries(plan.queries, queries);
    if plan.queries.is_empty() {
        plan.queries = fallback.queries;
    }
    plan
}

fn parse_plan(content: &str) -> Option<HistoryQueryPlan> {
    let raw = super::clients::strip_fence(content);
    let value: Value = serde_json::from_str(&raw).ok()?;
    let work = value.get("work").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let section = value.get("section").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let era = value.get("era").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let people = string_list(value.get("people"));
    let mut queries = string_list(value.get("queries"));
    queries.retain(|query| !query.is_empty());
    queries.truncate(MAX_QUERIES);
    if work.is_empty() && queries.is_empty() {
        return None;
    }
    Some(HistoryQueryPlan {
        work: work.chars().take(120).collect(),
        section: section.chars().take(120).collect(),
        people: people.into_iter().take(8).collect(),
        era: era.chars().take(80).collect(),
        queries,
        source: String::new(),
    })
}

fn fallback_plan(claim: &str, queries: &[String], context: &str) -> HistoryQueryPlan {
    let mut plan = HistoryQueryPlan { source: "fallback".into(), ..Default::default() };
    plan.queries = merge_queries(Vec::new(), queries);
    let haystack = format!("{claim} {} {context}", queries.join(" "));
    let works = match_registry(&haystack);
    if let Some(work) = works.first() {
        plan.work = work.title.into();
        plan.people = works
            .iter()
            .filter_map(|item| {
                if item.title == work.title {
                    Some(item.author.to_string())
                } else {
                    None
                }
            })
            .collect();
        if !plan.queries.iter().any(|query| query == work.title) {
            plan.queries.insert(0, work.title.to_string());
        }
    }
    if plan.queries.is_empty() {
        plan.queries.push(claim.chars().take(80).collect());
    }
    plan.queries.truncate(MAX_QUERIES);
    plan
}

fn merge_queries(mut first: Vec<String>, second: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for query in first.drain(..).chain(second.iter().cloned()) {
        let query = query.trim().chars().take(128).collect::<String>();
        if query.is_empty() {
            continue;
        }
        let key = query.to_lowercase();
        if seen.insert(key) {
            output.push(query);
        }
        if output.len() >= MAX_QUERIES {
            break;
        }
    }
    output
}

fn match_registry(text: &str) -> Vec<&'static RegistryWork> {
    REGISTRY
        .iter()
        .filter(|work| work.aliases.iter().any(|alias| text.contains(alias)))
        .collect()
}

pub async fn discover(
    plan: &HistoryQueryPlan,
    ctext_key: &str,
    token: &CancellationToken,
) -> Discovery {
    let mut discovery = Discovery::default();
    let queries = plan.queries.iter().take(2).cloned().collect::<Vec<_>>();
    let ctext_results = futures_util::future::join_all(
        plan_works(plan)
            .iter()
            .take(2)
            .map(|work| search_ctext(work, ctext_key)),
    )
    .await;
    let (wiki1, wiki2, ia1, ia2) = tokio::join!(
        async {
            match queries.first() {
                Some(query) => source_search::search_wikisource(query, 6).await,
                None => Ok(Vec::new()),
            }
        },
        async {
            match queries.get(1) {
                Some(query) => source_search::search_wikisource(query, 6).await,
                None => Ok(Vec::new()),
            }
        },
        async {
            match queries.first() {
                Some(query) => search_ia(query).await,
                None => Ok(Vec::new()),
            }
        },
        async {
            match queries.get(1) {
                Some(query) => search_ia(query).await,
                None => Ok(Vec::new()),
            }
        },
    );
    if token.is_cancelled() {
        return discovery;
    }
    for result in ctext_results {
        match result {
            Ok(mut hits) => discovery.hits.append(&mut hits),
            Err(error) => discovery.notes.push(format!("ctext 检索失败：{error}")),
        }
    }
    for (provider, result) in [("Wikisource", wiki1), ("Wikisource", wiki2)] {
        match result {
            Ok(hits) => discovery.hits.extend(wikisource_hits(hits)),
            Err(error) => discovery.notes.push(format!("{provider} 检索失败：{error}")),
        }
    }
    for (provider, result) in [("Internet Archive", ia1), ("Internet Archive", ia2)] {
        match result {
            Ok(mut hits) => discovery.hits.append(&mut hits),
            Err(error) => discovery.notes.push(format!("{provider} 检索失败：{error}")),
        }
    }
    discovery.hits.extend(link_only_hits(plan));
    discovery.notes.sort();
    discovery.notes.dedup();
    discovery.hits = rank_hits(plan, discovery.hits);
    discovery
}

fn wikisource_hits(items: Vec<source_search::SourceSearchHit>) -> Vec<SourceHit> {
    items
        .into_iter()
        .map(|hit| {
            let (work, section) = split_title(&hit.title);
            SourceHit {
                provider: "Wikisource".into(),
                title: hit.title.clone(),
                url: hit.url,
                snippet: hit.snippet,
                work_title: work,
                section_title: section,
                authority: Authority::Transcription,
                fetch_kind: FetchKind::WikisourcePage,
                fetch_ref: hit.title,
                scan_url: String::new(),
                edition: "Wikisource 转录".into(),
            }
        })
        .collect()
}

fn split_title(title: &str) -> (String, String) {
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

fn rank_hits(plan: &HistoryQueryPlan, hits: Vec<SourceHit>) -> Vec<SourceHit> {
    let works = plan_works(plan)
        .into_iter()
        .map(|work| normalize_match(&work))
        .filter(|work| !work.is_empty())
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let mut ranked = Vec::new();
    for hit in hits {
        let key = if hit.provider == "ctext" {
            format!("ctext:{}", normalize_match(&hit.title))
        } else {
            format!("{}:{}", hit.provider, if hit.fetch_ref.is_empty() { &hit.url } else { &hit.fetch_ref })
        };
        if !seen.insert(key) {
            continue;
        }
        let mut score = hit.authority.weight() * 100;
        let title = normalize_match(&hit.title);
        if !works.is_empty() {
            if works.iter().any(|work| *work == title) {
                score += 30;
            } else if works.iter().any(|work| title.contains(work)) {
                score += 18;
            } else if works.iter().any(|work| work.contains(&title)) {
                score += 8;
            }
        }
        if !plan.section.is_empty() && hit.title.contains(plan.section.trim()) {
            score += 12;
        }
        let overlap = plan
            .queries
            .iter()
            .map(|query| best_overlap_len(query, &format!("{} {}", hit.title, hit.snippet)))
            .max()
            .unwrap_or(0);
        score += overlap.min(10) as i32;
        ranked.push((score, hit));
    }
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.provider.cmp(&right.1.provider)));
    ranked.into_iter().take(MAX_HITS).map(|(_, hit)| hit).collect()
}

/// 从规划结果里提取可检索的书名：模型有时返回“《三国志》裴松之注引《江表传》”，
/// ctext 需要的是《三国志》/《江表传》这样的书名，而不是整段引用说明。
fn plan_works(plan: &HistoryQueryPlan) -> Vec<String> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let leading = plan.work.split('《').next().unwrap_or("").trim();
    push_work(&mut output, &mut seen, leading);
    let mut rest = plan.work.as_str();
    while let Some(start) = rest.find('《') {
        let after = &rest[start + 3..];
        let Some(end) = after.find('》') else { break };
        push_work(&mut output, &mut seen, &after[..end]);
        rest = &after[end + 3..];
    }
    if output.is_empty() && !plan.section.trim().is_empty() {
        push_work(&mut output, &mut seen, &plan.section);
    }
    output.into_iter().take(4).collect()
}

fn push_work(output: &mut Vec<String>, seen: &mut HashSet<String>, raw: &str) {
    let raw = raw.trim().trim_matches(|ch: char| ch.is_whitespace() || ch == '《' || ch == '》');
    let raw = raw.split(['（', '(']).next().unwrap_or(raw).trim();
    let raw = raw.split('·').next().unwrap_or(raw).trim();
    if raw.chars().count() < 2 {
        return;
    }
    if seen.insert(normalize_match(raw)) {
        output.push(raw.to_string());
    }
}

fn link_only_hits(plan: &HistoryQueryPlan) -> Vec<SourceHit> {
    let subject = plan_works(plan)
        .into_iter()
        .next()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| plan.queries.first().cloned())
        .unwrap_or_default();
    if subject.trim().is_empty() {
        return Vec::new();
    }
    let query = plan.queries.first().cloned().unwrap_or_else(|| subject.clone());
    vec![
        SourceHit {
            provider: "NLC".into(),
            title: format!("中华古籍资源库：{subject}"),
            url: "https://read.nlc.cn/".into(),
            snippet: "馆藏目录/影像；需要按书名检索，当前只作候选".into(),
            work_title: plan.work.clone(),
            section_title: plan.section.clone(),
            authority: Authority::LinkOnly,
            fetch_kind: FetchKind::None,
            fetch_ref: String::new(),
            scan_url: String::new(),
            edition: String::new(),
        },
        SourceHit {
            provider: "一史馆".into(),
            title: format!("第一历史档案馆：{subject}"),
            url: "https://fhac.com.cn/".into(),
            snippet: "明清档案目录；公开全文有限，当前只作候选".into(),
            work_title: plan.work.clone(),
            section_title: plan.section.clone(),
            authority: Authority::LinkOnly,
            fetch_kind: FetchKind::None,
            fetch_ref: String::new(),
            scan_url: String::new(),
            edition: String::new(),
        },
        SourceHit {
            provider: "HathiTrust".into(),
            title: format!("HathiTrust：{subject}"),
            url: format!(
                "https://babel.hathitrust.org/cgi/ls?q1={};a=srchls;anyall1=all;lmt=ft",
                encode(&query)
            ),
            snippet: "外文/公版全文；Full view 才可引用，当前只作候选".into(),
            work_title: plan.work.clone(),
            section_title: plan.section.clone(),
            authority: Authority::LinkOnly,
            fetch_kind: FetchKind::None,
            fetch_ref: String::new(),
            scan_url: String::new(),
            edition: String::new(),
        },
    ]
}

async fn search_ctext(work: &str, key: &str) -> Result<Vec<SourceHit>, String> {
    let url = format!("https://api.ctext.org/searchtexts?title={}&if=zh", encode(work));
    let value: Value = client(12)?.get(url).send().await.map_err(network_error)?.json().await.map_err(network_error)?;
    let books = value.get("books").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut hits = Vec::new();
    for book in books {
        let title = book.get("title").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let urn = book.get("urn").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if title.is_empty() || urn.is_empty() || !title_matches(work, &title) {
            continue;
        }
        let authority = if key.trim().is_empty() { Authority::LinkOnly } else { Authority::Transcription };
        hits.push(SourceHit {
            provider: "ctext".into(),
            title: title.clone(),
            url: format!("https://ctext.org/{}", urn.trim_start_matches("ctp:")),
            snippet: if key.trim().is_empty() {
                "ctext 书目；全文 API 需要订阅/API Key".into()
            } else {
                "ctext 全文（API）".into()
            },
            work_title: title,
            section_title: String::new(),
            authority,
            fetch_kind: FetchKind::CtextUrn,
            fetch_ref: urn,
            scan_url: String::new(),
            edition: "ctext".into(),
        });
        if hits.len() >= 40 {
            break;
        }
    }
    hits.sort_by_key(|hit| ctext_title_rank(work, &hit.title));
    let mut seen = HashSet::new();
    hits.retain(|hit| seen.insert(normalize_match(&hit.title)));
    hits.truncate(6);
    Ok(hits)
}

fn ctext_title_rank(work: &str, title: &str) -> u8 {
    let work = normalize_match(work);
    let title = normalize_match(title);
    if work == title {
        0
    } else if title.starts_with(&work) || title.ends_with(&work) {
        1
    } else {
        2
    }
}

async fn search_ia(query: &str) -> Result<Vec<SourceHit>, String> {
    let q = format!("({query}) AND mediatype:texts");
    let url = format!(
        "https://archive.org/advancedsearch.php?q={}&fl[]=identifier&fl[]=title&fl[]=creator&fl[]=year&fl[]=language&rows=6&page=1&output=json",
        encode(&q)
    );
    let value: Value = client(20)?.get(url).send().await.map_err(network_error)?.json().await.map_err(network_error)?;
    let docs = value
        .get("response")
        .and_then(|item| item.get("docs"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut hits = Vec::new();
    for doc in docs {
        let identifier = doc.get("identifier").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if identifier.is_empty() {
            continue;
        }
        let title = value_text(doc.get("title"));
        let snippet = [
            value_text(doc.get("creator")),
            value_text(doc.get("year")),
            value_text(doc.get("language")),
        ]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let url = format!("https://archive.org/details/{identifier}");
        hits.push(SourceHit {
            provider: "Internet Archive".into(),
            title: if title.is_empty() { identifier.clone() } else { title },
            url: url.clone(),
            snippet,
            work_title: String::new(),
            section_title: String::new(),
            authority: Authority::Ocr,
            fetch_kind: FetchKind::InternetArchiveItem,
            fetch_ref: identifier,
            scan_url: url,
            edition: "Internet Archive OCR".into(),
        });
    }
    Ok(hits)
}

pub async fn fetch_hit(
    hit: &SourceHit,
    history: &HistoryStore,
    ctext_key: &str,
    token: &CancellationToken,
) -> Result<FetchedText, String> {
    if token.is_cancelled() {
        return Err("已取消".into());
    }
    let cache_key = format!("{}:{}", hit.provider, hit.fetch_ref);
    if let Some(cached) = history.cache_get(&cache_key, CACHE_TTL_MS)? {
        if !cached.text.trim().is_empty() {
            return Ok(FetchedText { title: cached.title, text: cached.text, url: cached.url });
        }
    }
    let fetched = match hit.fetch_kind {
        FetchKind::WikisourcePage => fetch_wikisource(&hit.fetch_ref).await?,
        FetchKind::CtextUrn => fetch_ctext(&hit.fetch_ref, ctext_key).await?,
        FetchKind::InternetArchiveItem => fetch_ia(&hit.fetch_ref).await?,
        FetchKind::None => return Err("这个来源没有可自动抓取的全文".into()),
    };
    history.cache_put(&CachedPage {
        key: cache_key,
        provider: hit.provider.clone(),
        url: fetched.url.clone(),
        title: fetched.title.clone(),
        text: fetched.text.clone(),
        fetched_at: super::store::now_ms(),
    })?;
    Ok(fetched)
}

async fn fetch_wikisource(title: &str) -> Result<FetchedText, String> {
    let url = format!(
        "https://zh.wikisource.org/w/api.php?action=parse&page={}&prop=text&format=json&formatversion=2&utf8=1",
        encode(title)
    );
    let value: Value = client(20)?.get(url).send().await.map_err(network_error)?.json().await.map_err(network_error)?;
    if let Some(error) = value.get("error") {
        return Err(format!("Wikisource API：{}", value_text(error.get("info"))));
    }
    let html = value
        .get("parse")
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Wikisource API 没有返回 parse.text".to_string())?;
    let (_, text) = web_fetch::extract_html(html);
    if text.is_empty() {
        return Err("Wikisource 页面没有提取到正文".into());
    }
    Ok(FetchedText {
        title: value
            .get("parse")
            .and_then(|item| item.get("title"))
            .and_then(Value::as_str)
            .unwrap_or(title)
            .to_string(),
        text: text.chars().take(MAX_TEXT_CHARS).collect(),
        url: wikisource_page_url(title),
    })
}

async fn fetch_ctext(urn: &str, key: &str) -> Result<FetchedText, String> {
    if key.trim().is_empty() {
        return Err("ctext 全文需要订阅 API Key；当前只保留候选链接".into());
    }
    let url = format!(
        "https://api.ctext.org/gettext?urn={}&apikey={}&if=zh",
        encode(urn),
        encode(key.trim())
    );
    let value: Value = client(20)?.get(url).send().await.map_err(network_error)?.json().await.map_err(network_error)?;
    if let Some(error) = value.get("error") {
        return Err(format!(
            "ctext API：{} {}",
            value_text(error.get("code")),
            value_text(error.get("description"))
        ));
    }
    let title = value.get("title").and_then(Value::as_str).unwrap_or(urn).to_string();
    let paragraphs = value
        .get("fulltext")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    if paragraphs.is_empty() {
        return Err("ctext 只返回目录/章节列表；当前版本不递归抓章节".into());
    }
    let text = paragraphs.join("\n");
    Ok(FetchedText {
        title,
        text: text.chars().take(MAX_TEXT_CHARS).collect(),
        url: format!("https://ctext.org/{}", urn.trim_start_matches("ctp:")),
    })
}

async fn fetch_ia(identifier: &str) -> Result<FetchedText, String> {
    let metadata_url = format!("https://archive.org/metadata/{identifier}");
    let value: Value = client(20)?
        .get(metadata_url)
        .send()
        .await
        .map_err(network_error)?
        .json()
        .await
        .map_err(network_error)?;
    let files = value.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
    let text_file = files.iter().find_map(|file| {
        let name = file.get("name").and_then(Value::as_str)?;
        let format = file.get("format").and_then(Value::as_str).unwrap_or("");
        if name.ends_with("_djvu.txt") || format == "DjVuTXT" {
            Some(name.to_string())
        } else {
            None
        }
    });
    let Some(text_file) = text_file else {
        return Err("Internet Archive 条目没有 OCR 全文（_djvu.txt）".into());
    };
    let url = format!(
        "https://archive.org/download/{identifier}/{}",
        encode(&text_file).replace('+', "%20")
    );
    let text = fetch_text_limited(&url, 20).await?;
    let title = value
        .get("metadata")
        .and_then(|item| item.get("title"))
        .map(|value| value_text(Some(value)))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| identifier.to_string());
    Ok(FetchedText {
        title,
        text: text.chars().take(MAX_TEXT_CHARS).collect(),
        url: format!("https://archive.org/details/{identifier}"),
    })
}

async fn fetch_text_limited(url: &str, timeout_secs: u64) -> Result<String, String> {
    let response = client(timeout_secs)?
        .get(url)
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if output.len() + chunk.len() > MAX_BYTES {
            return Err("全文超过 8 MB 上限".into());
        }
        output.extend_from_slice(&chunk);
    }
    let text = String::from_utf8_lossy(&output).to_string();
    if text.trim().is_empty() {
        return Err("全文为空".into());
    }
    Ok(text)
}

fn client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent("Yebian/0.1 (local history research; source search)")
        .build()
        .map_err(|error| error.to_string())
}

fn network_error(error: reqwest::Error) -> String {
    let text = error.to_string();
    text.split(" for url (").next().unwrap_or(&text).trim().to_string()
}

fn encode(value: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", value)
        .finish()
        .trim_start_matches("q=")
        .to_string()
}

fn wikisource_page_url(title: &str) -> String {
    let path = title
        .split('/')
        .map(|part| encode(part).replace('+', "%20"))
        .collect::<Vec<_>>()
        .join("/");
    format!("https://zh.wikisource.org/wiki/{path}")
}

fn title_matches(work: &str, title: &str) -> bool {
    let work = normalize_match(work);
    let title = normalize_match(title);
    !work.is_empty() && (title.contains(&work) || work.contains(&title))
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>()
            .join("、"),
        _ => String::new(),
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        Some(Value::String(text)) => text
            .split(['、', ',', '，'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_match(value: &str) -> String {
    hanconv::t2s(value)
        .chars()
        .filter(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

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

    #[test]
    fn normalizes_traditional_titles_and_extracts_book_titles() {
        assert!(title_matches("三国志", "三國志"));
        assert!(best_overlap_len("三國志", "三国志") >= 3);
        let plan = HistoryQueryPlan {
            work: "《三国志》裴松之注引《江表传》".into(),
            ..Default::default()
        };
        let works = plan_works(&plan);
        assert!(works.contains(&"三国志".to_string()), "{works:?}");
        assert!(works.contains(&"江表传".to_string()), "{works:?}");
        let plan = HistoryQueryPlan {
            work: "三国志（裴松之注引《江表传》）".into(),
            ..Default::default()
        };
        let works = plan_works(&plan);
        assert!(works.contains(&"三国志".to_string()), "{works:?}");
        assert!(works.contains(&"江表传".to_string()), "{works:?}");
    }

    #[test]
    fn parses_query_plan() {        let plan = parse_plan(
            r#"{"work":"明史","section":"卷三百八·严嵩传","people":["严嵩","嘉靖"],"era":"明","queries":["明史 严嵩 窃权罔利","嘉靖 严嵩"]}"#,
        )
        .unwrap();
        assert_eq!(plan.work, "明史");
        assert_eq!(plan.section, "卷三百八·严嵩传");
        assert_eq!(plan.queries.len(), 2);
    }

    #[test]
    fn fallback_uses_registry_and_original_queries() {
        let plan = fallback_plan("严嵩无他才略。", &["严嵩 明史".into()], "嘉靖朝");
        assert_eq!(plan.work, "明史");
        assert!(plan.queries.iter().any(|query| query == "明史"));
        assert!(plan.queries.iter().any(|query| query == "严嵩 明史"));
    }

    #[test]
    fn ranks_transcription_above_link_only() {
        let plan = HistoryQueryPlan {
            work: "明史".into(),
            queries: vec!["明史 严嵩".into()],
            ..Default::default()
        };
        let hits = vec![
            SourceHit {
                provider: "NLC".into(),
                title: "中华古籍资源库：明史".into(),
                url: "https://read.nlc.cn/".into(),
                snippet: String::new(),
                work_title: "明史".into(),
                section_title: String::new(),
                authority: Authority::LinkOnly,
                fetch_kind: FetchKind::None,
                fetch_ref: String::new(),
                scan_url: String::new(),
                edition: String::new(),
            },
            SourceHit {
                provider: "Wikisource".into(),
                title: "明史/卷三百八".into(),
                url: "https://zh.wikisource.org/wiki/明史/卷三百八".into(),
                snippet: "严嵩".into(),
                work_title: "明史".into(),
                section_title: "卷三百八".into(),
                authority: Authority::Transcription,
                fetch_kind: FetchKind::WikisourcePage,
                fetch_ref: "明史/卷三百八".into(),
                scan_url: String::new(),
                edition: "Wikisource".into(),
            },
        ];
        let ranked = rank_hits(&plan, hits);
        assert_eq!(ranked[0].provider, "Wikisource");
    }

    #[test]
    fn exact_work_title_outranks_related_title() {
        let plan = HistoryQueryPlan {
            work: "明史".into(),
            queries: vec!["明史 严嵩".into()],
            ..Default::default()
        };
        let hits = vec![
            SourceHit {
                provider: "ctext".into(),
                title: "明史紀事本末".into(),
                url: "https://ctext.org/wb582770".into(),
                snippet: String::new(),
                work_title: "明史紀事本末".into(),
                section_title: String::new(),
                authority: Authority::LinkOnly,
                fetch_kind: FetchKind::CtextUrn,
                fetch_ref: "ctp:wb582770".into(),
                scan_url: String::new(),
                edition: "ctext".into(),
            },
            SourceHit {
                provider: "ctext".into(),
                title: "明史".into(),
                url: "https://ctext.org/wb410835".into(),
                snippet: String::new(),
                work_title: "明史".into(),
                section_title: String::new(),
                authority: Authority::LinkOnly,
                fetch_kind: FetchKind::CtextUrn,
                fetch_ref: "ctp:wb410835".into(),
                scan_url: String::new(),
                edition: "ctext".into(),
            },
        ];
        let ranked = rank_hits(&plan, hits);
        assert_eq!(ranked[0].title, "明史");
    }

    #[test]
    fn keeps_slash_in_wikisource_page_url() {
        let url = wikisource_page_url("明史/卷三百八");
        assert!(url.contains("/wiki/%E6%98%8E%E5%8F%B2/%E5%8D%B7%E4%B8%89%E7%99%BE%E5%85%AB"), "{url}");
        assert!(!url.contains("%2F"));
    }

    #[test]
    fn ranks_exact_ctext_title_before_related_titles() {
        assert_eq!(ctext_title_rank("资治通鉴", "資治通鑑"), 0);
        assert_eq!(ctext_title_rank("资治通鉴", "續資治通鑑"), 1);
        assert_eq!(ctext_title_rank("资治通鉴", "御批資治通鑑綱目"), 2);
    }

    #[test]
    fn parses_ctext_book_shape() {
        let value: Value = serde_json::from_str(r#"{"books":[{"title":"明史","urn":"ctp:wb410835"}]}"#).unwrap();
        let books = value.get("books").and_then(Value::as_array).unwrap();
        assert_eq!(books[0]["urn"], "ctp:wb410835");
    }

    #[test]
    fn parses_ia_doc_shape() {
        let value: Value = serde_json::from_str(
            r#"{"response":{"docs":[{"identifier":"ming-shi","title":"明史","creator":"张廷玉","year":"1739"}]}}"#,
        )
        .unwrap();
        let docs = value.get("response").and_then(|item| item.get("docs")).and_then(Value::as_array).unwrap();
        assert_eq!(docs[0]["identifier"], "ming-shi");
        assert_eq!(value_text(docs[0].get("title")), "明史");
    }

    #[tokio::test]
    #[ignore]
    async fn live_eval_history_candidates() {
        use crate::production::history::HistoryCandidateInput;
        let db = std::env::var("YEBIAN_DB").expect("YEBIAN_DB");
        let path = std::path::Path::new(&db);
        let store = ProductionStore::open(path).expect("production store");
        let facts = crate::production::facts::FactStore::open(path).expect("fact store");
        let history = HistoryStore::open(path).expect("history store");
        let settings = store.settings();
        let key = store.ai_key(&settings.ai_provider);
        let ai = AiConfig {
            provider: settings.ai_provider.clone(),
            protocol: settings.ai_protocol.clone(),
            base_url: settings.ai_base_url.clone(),
            model: settings.ai_model.clone(),
            api_key: if key.is_empty() { "local".into() } else { key },
            retries: settings.retries,
        };
        let cost = CostModel { price: None, asr_price_per_minute: 0.0 };
        let token = CancellationToken::new();
        let all = facts.list_facts("", "", 300).expect("facts");
        let mut usage = Usage::default();
        let mut ctext_cache: std::collections::HashMap<String, Vec<SourceHit>> = std::collections::HashMap::new();
        for fact in all.into_iter().filter(|fact| fact.domain == "history") {
            let context = fact
                .mentions
                .iter()
                .map(|mention| mention.quote.clone())
                .collect::<Vec<_>>()
                .join("\n");
            let plan = plan_query(
                &store,
                &fact.id,
                &fact.text,
                &fact.queries,
                &context,
                Some(&ai),
                &mut usage,
                &cost,
                &token,
            )
            .await;
            let mut hits = Vec::new();
            for work in plan_works(&plan) {
                let work_hits = if let Some(cached) = ctext_cache.get(&work) {
                    cached.clone()
                } else {
                    let work_hits = search_ctext(&work, "").await.unwrap_or_default();
                    ctext_cache.insert(work.clone(), work_hits.clone());
                    work_hits
                };
                hits.extend(work_hits);
            }
            hits.extend(link_only_hits(&plan));
            let ranked = rank_hits(&plan, hits);
            let ctext = ranked.iter().filter(|hit| hit.provider == "ctext").count();
            let candidates = ranked
                .iter()
                .map(|hit| HistoryCandidateInput {
                    provider: hit.provider.clone(),
                    title: hit.title.clone(),
                    url: hit.url.clone(),
                    snippet: hit.snippet.clone(),
                    authority: hit.authority.as_str().into(),
                    status: "candidate".into(),
                    reason: if hit.provider == "ctext" && hit.authority == Authority::LinkOnly {
                        "ctext 全文需要订阅 API Key；当前只保留候选链接".into()
                    } else if hit.authority == Authority::LinkOnly {
                        "外部/目录源，未自动抓取".into()
                    } else {
                        "待抓取全文".into()
                    },
                    work_title: hit.work_title.clone(),
                    section_title: hit.section_title.clone(),
                    ..Default::default()
                })
                .collect::<Vec<_>>();
            history.replace_candidates(&fact.id, &candidates).expect("replace candidates");
            println!(
                "{}\t{}\twork={}\tworks={}\tcandidates={}\tctext={}",
                fact.id,
                fact.text,
                plan.work,
                plan_works(&plan).join(" / "),
                ranked.len(),
                ctext
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_eval_history_plans() {        let db = std::env::var("YEBIAN_DB").expect("YEBIAN_DB");
        let path = std::path::Path::new(&db);
        let store = ProductionStore::open(path).expect("production store");
        let facts = crate::production::facts::FactStore::open(path).expect("fact store");
        let settings = store.settings();
        let key = store.ai_key(&settings.ai_provider);
        let ai = AiConfig {
            provider: settings.ai_provider.clone(),
            protocol: settings.ai_protocol.clone(),
            base_url: settings.ai_base_url.clone(),
            model: settings.ai_model.clone(),
            api_key: if key.is_empty() { "local".into() } else { key },
            retries: settings.retries,
        };
        let cost = CostModel { price: None, asr_price_per_minute: 0.0 };
        let token = CancellationToken::new();
        let facts = facts.list_facts("", "", 300).expect("facts");
        let mut usage = Usage::default();
        let mut ctext_cache: std::collections::HashMap<String, Vec<SourceHit>> = std::collections::HashMap::new();
        for fact in facts.into_iter().filter(|fact| fact.domain == "history") {
            let context = fact
                .mentions
                .iter()
                .map(|mention| mention.quote.clone())
                .collect::<Vec<_>>()
                .join("\n");
            let plan = plan_query(
                &store,
                &fact.id,
                &fact.text,
                &fact.queries,
                &context,
                Some(&ai),
                &mut usage,
                &cost,
                &token,
            )
            .await;
            let works = plan_works(&plan);
            let mut ctext_titles = Vec::new();
            for work in &works {
                let hits = if let Some(cached) = ctext_cache.get(work) {
                    cached.clone()
                } else {
                    let hits = search_ctext(work, "").await.unwrap_or_default();
                    ctext_cache.insert(work.clone(), hits.clone());
                    hits
                };
                ctext_titles.extend(hits.into_iter().map(|hit| hit.title).take(4));
            }
            println!(
                "{}\t{}\twork={}\tsection={}\tworks={}\tctext={}",
                fact.id,
                fact.text,
                plan.work,
                plan.section,
                works.join(" / "),
                ctext_titles.join(" | ")
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn live_ctext_search_returns_ming_shi() {
        let hits = search_ctext("明史", "").await.expect("ctext search");
        assert!(hits.iter().any(|hit| hit.title == "明史"), "{hits:?}");
        let hits = search_ctext("三国志", "").await.expect("ctext sanguozhi");
        assert!(hits.iter().any(|hit| title_matches("三国志", &hit.title)), "{hits:?}");
        let hits = search_ctext("资治通鉴", "").await.expect("ctext zizhi");
        assert!(hits.iter().any(|hit| title_matches("资治通鉴", &hit.title)), "{hits:?}");
    }
}
