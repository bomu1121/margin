//! 图片溯源：反查服务 + 视觉分析 + 联网检索 + 模型汇总。
//!
//! 这里不把“识别成文字再让用户搜”当作结果。用户点一次，就直接把图片送到
//! 360 识图（同图/相似图）和 trace.moe（动画场景），同时让视觉模型读 OCR/
//! 实体，再用联网检索交叉核对，最后由文本模型给出带证据的来源候选。
//!
//! 边界：360 的结果是“相似/同图页面”的索引，不保证是原图；trace.moe 只对
//! 动画截图有效；所有外部服务的可用性会随网络和反爬变化，失败要如实返回。

use std::collections::HashSet;
use std::time::{Duration, Instant};

use base64::Engine;
use futures_util::future::join_all;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, RgbImage};
use reqwest::header::{ACCEPT_LANGUAGE, ORIGIN, REFERER};
use reqwest::multipart::{Form, Part};
use scraper::{Html, Selector};
use serde::Serialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::production::clients::{self, AiConfig, SearchConfig, SearchHit, SearchUsage};
use crate::production::web_fetch;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const VISION_TIMEOUT: u64 = 120;
const REVERSE_TIMEOUT: u64 = 90;
const SEARCH_TIMEOUT: u64 = 35;
const SYNTHESIS_TIMEOUT: u64 = 120;
const MAX_UPLOAD_EDGE: u32 = 1600;
const MAX_REVERSE_HITS: usize = 20;
const MAX_SEARCH_QUERIES: usize = 6;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestigationProgress {
    pub step: String,
    pub status: String,
    pub detail: String,
    pub count: usize,
    pub ms: u64,
}

impl InvestigationProgress {
    fn new(step: &str, status: &str, detail: impl Into<String>, count: usize, ms: u64) -> Self {
        Self {
            step: step.to_string(),
            status: status.to_string(),
            detail: detail.into(),
            count,
            ms,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCandidate {
    pub title: String,
    pub url: String,
    pub site: String,
    /// high / medium / low
    pub confidence: String,
    /// reverse / anime / search
    pub kind: String,
    pub evidence: String,
    pub thumbnail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarImage {
    pub title: String,
    pub url: String,
    pub site: String,
    pub image_url: String,
    pub thumbnail: String,
    /// 360 / trace.moe
    pub provider: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub label: String,
    /// ok / empty / failed / skipped
    pub status: String,
    pub count: usize,
    pub ms: u64,
    pub error: String,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub search_calls: u32,
    pub search_cost_cny: f64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInvestigation {
    pub description: String,
    pub visible_text: Vec<String>,
    pub entities: Vec<String>,
    /// anime / social / news / meme / photo / product / screenshot / unknown
    pub kind: String,
    /// found / likely / not_found
    pub verdict: String,
    pub summary: String,
    pub sources: Vec<SourceCandidate>,
    pub similar: Vec<SimilarImage>,
    pub queries: Vec<String>,
    pub providers: Vec<ProviderStatus>,
    pub usage: ImageUsage,
}

#[derive(Clone, Debug, Default)]
struct VisionAnalysis {
    description: String,
    visible_text: Vec<String>,
    entities: Vec<String>,
    kind: String,
    queries: Vec<String>,
}

#[derive(Clone, Debug)]
struct ReverseHit {
    title: String,
    site: String,
    url: String,
    image_url: String,
    thumbnail: String,
}

#[derive(Clone, Debug)]
struct AnimeHit {
    title: String,
    site: String,
    url: String,
    image_url: String,
    thumbnail: String,
    episode: String,
    at: String,
    similarity: f64,
}

#[derive(Clone, Debug, Default)]
struct Synthesis {
    verdict: String,
    summary: String,
    sources: Vec<SynthesisSource>,
    next_queries: Vec<String>,
}

#[derive(Clone, Debug)]
struct SynthesisSource {
    title: String,
    url: String,
    site: String,
    confidence: String,
    kind: String,
    evidence: String,
}

struct PreparedImage {
    vision_data_url: String,
    upload_data_url: String,
    upload_bytes: Vec<u8>,
    upload_mime: String,
    width: u32,
    height: u32,
}

pub async fn investigate<F>(
    ai: &AiConfig,
    search: Option<&SearchConfig>,
    bytes: &[u8],
    mime: &str,
    mode: &str,
    progress: F,
) -> Result<ImageInvestigation, String>
where
    F: Fn(InvestigationProgress) + Send + Sync,
{
    let started = Instant::now();
    let prepared = prepare_image(bytes, mime);
    let mut providers = Vec::new();
    let mut usage = ImageUsage::default();

    // 1. 视觉分析。它和反查可以并行，但后面的关键词检索需要它的结果。
    progress(InvestigationProgress::new("vision", "running", "正在读图、摘文字、识别主体", 0, 0));
    let vision_started = Instant::now();
    let token = CancellationToken::new();
    let vision_user = if prepared.width > 0 && prepared.height > 0 {
        format!("{VISION_USER}\n这张图的原始尺寸是 {}×{} 像素。", prepared.width, prepared.height)
    } else {
        VISION_USER.to_string()
    };
    let vision_result = tokio::time::timeout(
        Duration::from_secs(VISION_TIMEOUT),
        clients::chat_vision(ai, VISION_SYSTEM, &vision_user, &[prepared.vision_data_url.clone()], &token),
    )
    .await;
    let (vision, vision_ms, vision_status) = match vision_result {
        Ok(Ok(response)) => {
            usage.prompt_tokens += response.prompt_tokens;
            usage.completion_tokens += response.completion_tokens;
            let parsed = parse_vision_analysis(&response.content);
            let detail = if parsed.description.is_empty() {
                "模型返回了内容，但没有解析出结构化描述".to_string()
            } else {
                format!("{} 字描述 · {} 条文字 · {} 个实体", parsed.description.chars().count(), parsed.visible_text.len(), parsed.entities.len())
            };
            progress(InvestigationProgress::new("vision", "done", detail, 1, vision_started.elapsed().as_millis() as u64));
            (parsed, vision_started.elapsed().as_millis() as u64, "ok".to_string())
        }
        Ok(Err(error)) => {
            progress(InvestigationProgress::new("vision", "failed", error.clone(), 0, vision_started.elapsed().as_millis() as u64));
            (VisionAnalysis { description: String::new(), kind: "unknown".into(), ..VisionAnalysis::default() }, vision_started.elapsed().as_millis() as u64, "failed".to_string())
        }
        Err(_) => {
            let detail = format!("读图超时（{}s）", VISION_TIMEOUT);
            progress(InvestigationProgress::new("vision", "failed", detail.clone(), 0, vision_started.elapsed().as_millis() as u64));
            (VisionAnalysis { description: String::new(), kind: "unknown".into(), ..VisionAnalysis::default() }, vision_started.elapsed().as_millis() as u64, "failed".to_string())
        }
    };
    providers.push(ProviderStatus {
        id: "vision".into(),
        label: "读图".into(),
        status: vision_status,
        count: if vision.description.is_empty() { 0 } else { 1 },
        ms: vision_ms,
        error: if vision.description.is_empty() { "视觉模型没有返回可用描述".into() } else { String::new() },
    });

    // 2. 反查服务。360 识图拿同图/相似页面，trace.moe 拿动画场景。
    progress(InvestigationProgress::new("reverse", "running", "360 识图 + trace.moe 正在反查（会先上传图片）", 0, 0));
    let reverse_client = build_client(REVERSE_TIMEOUT);
    let reverse_data = prepared.upload_data_url.clone();
    let trace_bytes = prepared.upload_bytes.clone();
    let trace_mime = prepared.upload_mime.clone();
    let reverse_started = Instant::now();
    let reverse_task = tokio::spawn(async move { search_360(&reverse_client, &reverse_data).await });
    let trace_client = build_client(REVERSE_TIMEOUT);
    let trace_task = tokio::spawn(async move { search_trace_moe(&trace_client, &trace_bytes, &trace_mime).await });
    let reverse_result = tokio::time::timeout(Duration::from_secs(REVERSE_TIMEOUT), reverse_task).await;
    let trace_result = tokio::time::timeout(Duration::from_secs(REVERSE_TIMEOUT), trace_task).await;
    let reverse_ms = reverse_started.elapsed().as_millis() as u64;
    let (mut reverse, reverse_status, reverse_error) = match reverse_result {
        Ok(Ok(Ok(value))) => (value, "ok".to_string(), String::new()),
        Ok(Ok(Err(error))) => (Vec::new(), "failed".to_string(), error),
        Ok(Err(error)) => (Vec::new(), "failed".to_string(), format!("360 识图任务失败：{error}")),
        Err(_) => (Vec::new(), "failed".to_string(), format!("360 识图超时（{}s）", REVERSE_TIMEOUT)),
    };
    let (mut anime, mut trace_status, mut trace_error) = match trace_result {
        Ok(Ok(Ok(value))) => (value, "ok".to_string(), String::new()),
        Ok(Ok(Err(error))) => (Vec::new(), "failed".to_string(), error),
        Ok(Err(error)) => (Vec::new(), "failed".to_string(), format!("trace.moe 任务失败：{error}")),
        Err(_) => (Vec::new(), "failed".to_string(), format!("trace.moe 超时（{}s）", REVERSE_TIMEOUT)),
    };
    // trace.moe 匹配的是画面里的动画帧；社交/新闻截图里的动漫头像也会命中，不能当成截图出处。
    let trace_applicable = matches!(vision.kind.as_str(), "anime" | "meme")
        || (vision.kind == "screenshot" && vision.visible_text.is_empty());
    if !trace_applicable {
        if !anime.is_empty() {
            trace_error = format!("图片类型是 {}，trace.moe 命中的是画面里的动画元素，不采用", vision.kind);
        }
        anime.clear();
        trace_status = "skipped".to_string();
    }
    if reverse.is_empty() && reverse_status == "ok" {
        providers.push(ProviderStatus { id: "360".into(), label: "360 识图".into(), status: "empty".into(), count: 0, ms: reverse_ms, error: "360 没有返回可用结果".into() });
    } else {
        providers.push(ProviderStatus { id: "360".into(), label: "360 识图".into(), status: reverse_status, count: reverse.len(), ms: reverse_ms, error: reverse_error });
    }
    providers.push(ProviderStatus {
        id: "trace".into(),
        label: "trace.moe".into(),
        status: if anime.is_empty() && trace_status == "ok" { "empty".into() } else { trace_status },
        count: anime.len(),
        ms: reverse_ms,
        error: if anime.is_empty() && trace_error.is_empty() { "没有相似度 ≥ 85% 的动画场景".into() } else { trace_error },
    });
    progress(InvestigationProgress::new(
        "reverse",
        if reverse.is_empty() && anime.is_empty() { "empty" } else { "done" },
        format!("360 识图 {} 条 · trace.moe {} 条", reverse.len(), anime.len()),
        reverse.len() + anime.len(),
        reverse_ms,
    ));

    // 3. 用视觉模型给的词做交叉检索。没有检索 Key 就跳过，不阻塞反查结果。
    let mut search_hits: Vec<SearchHit> = Vec::new();
    let mut search_error = String::new();
    let mut search_status = "skipped".to_string();
    let mut search_ms = 0u64;
    let mut queries = vision.queries.clone();
    if queries.len() < MAX_SEARCH_QUERIES {
        if let Some(text) = vision.visible_text.first() {
            let snippet = text.trim().chars().take(36).collect::<String>();
            if !snippet.is_empty() && !queries.iter().any(|q| q == &snippet) {
                queries.push(snippet);
            }
        }
    }
    queries.truncate(MAX_SEARCH_QUERIES);
    if let Some(config) = search {
        if !queries.is_empty() {
            progress(InvestigationProgress::new("search", "running", format!("用 {} 组词交叉检索", queries.len()), 0, 0));
            let search_started = Instant::now();
            let mut usage_total = SearchUsage::default();
            let futures = queries.iter().map(|query| {
                let config = config.clone();
                let query = query.clone();
                async move {
                    let token = CancellationToken::new();
                    tokio::time::timeout(
                        Duration::from_secs(SEARCH_TIMEOUT),
                        clients::search_counted(&config, &query, 5, &token),
                    )
                    .await
                }
            });
            let results = join_all(futures).await;
            let mut failures = 0usize;
            for result in results {
                match result {
                    Ok(Ok((hits, usage))) => {
                        usage_total.calls += usage.calls;
                        usage_total.successful_calls += usage.successful_calls;
                        usage_total.priced_calls += usage.priced_calls;
                        usage_total.estimated_cost += usage.estimated_cost;
                        search_hits.extend(hits);
                    }
                    _ => failures += 1,
                }
            }
            usage.search_calls += usage_total.calls;
            usage.search_cost_cny += usage_total.estimated_cost;
            search_hits = dedupe_search_hits(search_hits);
            search_ms = search_started.elapsed().as_millis() as u64;
            search_status = if search_hits.is_empty() {
                search_error = if failures == queries.len() {
                    "所有检索请求都失败；可能是 Key、限流或网络问题".into()
                } else {
                    "检索已连通，但没有返回结果".into()
                };
                "failed".into()
            } else {
                "ok".into()
            };
            progress(InvestigationProgress::new(
                "search",
                if search_hits.is_empty() { "failed" } else { "done" },
                if search_hits.is_empty() { search_error.clone() } else { format!("{} 条网页线索", search_hits.len()) },
                search_hits.len(),
                search_ms,
            ));
        } else {
            search_error = "视觉模型没有给出检索词".into();
        }
    } else {
        search_error = "没有配置联网检索 Key，只用了反查结果".into();
    }
    providers.push(ProviderStatus {
        id: "search".into(),
        label: "联网检索".into(),
        status: search_status,
        count: search_hits.len(),
        ms: search_ms,
        error: search_error,
    });

    // 4. 让模型汇总。缩略图也在同一段时间里抓，避免串行等待。
    progress(InvestigationProgress::new("synthesis", "running", "正在把反查、检索和画面证据放在一起判断", 0, 0));
    let synthesis_started = Instant::now();
    let user_prompt = synthesis_user_prompt(mode, &vision, &reverse, &anime, &search_hits);
    let synthesis_result = tokio::time::timeout(
        Duration::from_secs(SYNTHESIS_TIMEOUT),
        clients::chat(ai, SYNTHESIS_SYSTEM, &user_prompt, &token),
    )
    .await;
    let (synthesis, synthesis_status, synthesis_error) = match synthesis_result {
        Ok(Ok(response)) => {
            usage.prompt_tokens += response.prompt_tokens;
            usage.completion_tokens += response.completion_tokens;
            let parsed = parse_synthesis(&response.content);
            (parsed, "ok".to_string(), String::new())
        }
        Ok(Err(error)) => {
            progress(InvestigationProgress::new("synthesis", "failed", error.clone(), 0, synthesis_started.elapsed().as_millis() as u64));
            (Synthesis::default(), "failed".to_string(), error)
        }
        Err(_) => {
            let detail = format!("汇总超时（{}s）", SYNTHESIS_TIMEOUT);
            progress(InvestigationProgress::new("synthesis", "failed", detail.clone(), 0, synthesis_started.elapsed().as_millis() as u64));
            (Synthesis::default(), "failed".to_string(), detail)
        }
    };
    let synthesis_ms = synthesis_started.elapsed().as_millis() as u64;
    providers.push(ProviderStatus {
        id: "synthesis".into(),
        label: "汇总".into(),
        status: synthesis_status.clone(),
        count: if synthesis.summary.is_empty() { 0 } else { 1 },
        ms: synthesis_ms,
        error: synthesis_error,
    });

    let thumb_started = Instant::now();
    enrich_thumbnails(&mut reverse, &mut anime).await;
    let _thumb_ms = thumb_started.elapsed().as_millis() as u64;

    let mut sources = merge_sources(&synthesis, &reverse, &anime, &search_hits);
    let similar = build_similar(&reverse, &anime);
    let mut verdict = normalize_verdict(&synthesis.verdict);
    // 360 的结果是“相似/同图页面”的检索索引，不能单独证明原图；没有动画精确命中时最多是 likely。
    if verdict == "found" && anime.is_empty() {
        verdict = "likely".into();
    }
    // 模型明确说没找到时，不把相似页塞进“来源候选”，只保留在相似图里。
    if verdict == "not_found" {
        sources.clear();
    }
    if sources.is_empty() {
        verdict = "not_found".into();
    }
    let summary = if synthesis.summary.trim().is_empty() {
        fallback_summary(&vision, &reverse, &anime, &search_hits, &verdict)
    } else {
        clean_summary(&synthesis.summary)
    };
    let mut queries = vision.queries.clone();
    for query in synthesis.next_queries {
        if !queries.iter().any(|item| item == &query) {
            queries.push(query);
        }
    }
    queries.truncate(8);

    usage.total_ms = started.elapsed().as_millis() as u64;
    progress(InvestigationProgress::new(
        "synthesis",
        if synthesis_status == "ok" { "done" } else { "failed" },
        if verdict == "not_found" { "没有找到可靠来源".to_string() } else { format!("{} 个来源候选 · {} 张相似图", sources.len(), similar.len()) },
        sources.len() + similar.len(),
        synthesis_ms,
    ));

    Ok(ImageInvestigation {
        description: vision.description,
        visible_text: vision.visible_text,
        entities: vision.entities,
        kind: if vision.kind.is_empty() { "unknown".into() } else { vision.kind },
        verdict,
        summary,
        sources,
        similar,
        queries,
        providers,
        usage,
    })
}

const VISION_SYSTEM: &str = r#"你是图片溯源助手。先只做“看”，不要猜出处。
要求：
1. 逐字摘录图片里所有可读文字，包括标题、正文、用户名、时间、点赞数、按钮、网址、水印、弹幕；保留原文，不要翻译。
2. 识别平台/应用界面、人物或角色、作品、产品、地点、标志、界面元素。
3. 判断图片类型：anime（动画截图/插画）、social（社交平台截图）、news（新闻/文章截图）、meme（表情包）、photo（照片）、product（商品）、screenshot（其他截图）、unknown。
4. 给出 4 到 6 条最有区分度的检索词，优先用画面里独有的文字、用户名、台词、作品名；同时给中文和英文，不要只写“动漫”“美女”这类泛词。
5. 不要声称已经找到来源；不要编造人物身份、地点、时间。
只输出 JSON：
{"description":"80-160 字客观描述","visible_text":["逐字文字"],"entities":["人物/作品/产品/地点/标志"],"kind":"anime|social|news|meme|photo|product|screenshot|unknown","queries":["检索词"],"source_hints":["仅根据画面能看出的线索"]}"#;

const VISION_USER: &str = "请分析这张图片，只输出约定的 JSON。";

const SYNTHESIS_SYSTEM: &str = r#"你是图片溯源分析员。你会收到：视觉分析、360 识图的相似/同图页面、trace.moe 的动画场景匹配、联网检索结果。
规则：
1. 只依据给出的证据，不要编造 URL、标题、作者或“原图出处”。
2. 360 识图返回的是相似图或包含该图的页面，不等于原图；不能仅凭 360 结果说“确认原图”。
3. trace.moe 只有 similarity ≥ 0.85 才算可信的动画场景匹配。
4. 联网检索只是关键词命中，必须和画面文字/实体对得上，才能作为候选。
5. 明确区分：reverse（反查直接命中）、anime（动画场景）、search（关键词候选）。如果多个独立检索结果在角色、作品、服饰、题材上逐项吻合，即使没有原图 URL，也可以判 likely；只有泛泛同类、对不上具体特征时才判 not_found。
6. 找不到可靠来源就写 not_found，不要用泛泛的搜索结果凑数。
7. sources 只填证据里真实出现的 URL；没有 URL 的候选不要写。
8. 不要把检索结果里的尺寸、文件名、作者当成这张图的属性；只有视觉分析明确给出的信息才能写成这张图的事实。
9. summary 直接给结论和依据，不要以 found / likely / not_found 或“结论：”开头。
只输出 JSON：
{"verdict":"found|likely|not_found","summary":"先给结论，再给依据，120-300 字","sources":[{"title":"","url":"","site":"","confidence":"high|medium|low","kind":"reverse|anime|search","evidence":"这条证据说明了什么"}],"clues":["画面线索"],"next_queries":["如果还要继续查，建议的词"]}"#;

fn synthesis_user_prompt(
    mode: &str,
    vision: &VisionAnalysis,
    reverse: &[ReverseHit],
    anime: &[AnimeHit],
    search: &[SearchHit],
) -> String {
    let focus = if mode == "similar" {
        "用户的主要目标是找相似图片；来源候选仍然要给，但相似图要优先。"
    } else {
        "用户的主要目标是查这张图从哪来；优先判断原始页面/作者/作品，相似图只作为辅助。"
    };
    format!(
        "{focus}\n\n视觉分析：\n{}\n\n360 识图：\n{}\n\ntrace.moe：\n{}\n\n联网检索：\n{}",
        json!({
            "description": vision.description,
            "visible_text": vision.visible_text,
            "entities": vision.entities,
            "kind": vision.kind,
            "queries": vision.queries,
        }),
        serde_json::to_string_pretty(&json!(reverse.iter().take(12).map(|hit| json!({
            "title": hit.title,
            "site": hit.site,
            "url": hit.url,
            "image_url": hit.image_url,
        })).collect::<Vec<_>>())).unwrap_or_default(),
        serde_json::to_string_pretty(&json!(anime.iter().take(6).map(|hit| json!({
            "title": hit.title,
            "site": hit.site,
            "url": hit.url,
            "episode": hit.episode,
            "at": hit.at,
            "similarity": hit.similarity,
        })).collect::<Vec<_>>())).unwrap_or_default(),
        serde_json::to_string_pretty(&json!(search.iter().take(18).map(|hit| json!({
            "title": hit.title,
            "url": hit.url,
            "snippet": hit.content.chars().take(260).collect::<String>(),
        })).collect::<Vec<_>>())).unwrap_or_default(),
    )
}

fn prepare_image(bytes: &[u8], mime: &str) -> PreparedImage {
    let vision_data_url = data_url(bytes, mime);
    let fallback = PreparedImage {
        vision_data_url: vision_data_url.clone(),
        upload_data_url: vision_data_url.clone(),
        upload_bytes: bytes.to_vec(),
        upload_mime: mime.to_string(),
        width: 0,
        height: 0,
    };
    let Ok(decoded) = image::load_from_memory(bytes) else { return fallback };
    let (width, height) = decoded.dimensions();
    let resized = resize_to_max(decoded, MAX_UPLOAD_EDGE);
    let rgb = flatten_alpha(resized);
    let mut out = Vec::new();
    if JpegEncoder::new_with_quality(&mut out, 86).encode_image(&rgb).is_err() {
        return fallback;
    }
    let data = data_url(&out, "image/jpeg");
    PreparedImage {
        vision_data_url,
        upload_data_url: data,
        upload_bytes: out,
        upload_mime: "image/jpeg".into(),
        width,
        height,
    }
}

fn resize_to_max(image: DynamicImage, max_edge: u32) -> DynamicImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height).max(1);
    if longest <= max_edge {
        return image;
    }
    let scale = max_edge as f32 / longest as f32;
    let next_width = ((width as f32 * scale).round() as u32).max(1);
    let next_height = ((height as f32 * scale).round() as u32).max(1);
    image.resize(next_width, next_height, FilterType::Lanczos3)
}

fn flatten_alpha(image: DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = RgbImage::new(width, height);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = pixel[3] as u32;
        let blend = |channel: u8| ((channel as u32 * alpha + 255 * (255 - alpha)) / 255) as u8;
        rgb.put_pixel(x, y, image::Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]));
    }
    rgb
}

fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn build_client(timeout_secs: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(12))
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent(UA)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

async fn search_360(client: &reqwest::Client, data_url: &str) -> Result<Vec<ReverseHit>, String> {
    let form = Form::new()
        .text("img_url", "")
        .text("img_buf", data_url.to_string())
        .text("submittype", "upload")
        .text("src", "st");
    let response = client
        .post("https://st.so.com/r?src=st&srcsp=home")
        .header(REFERER, "https://st.so.com/")
        .header(ORIGIN, "https://st.so.com")
        .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9")
        .multipart(form)
        .send()
        .await
        .map_err(|error| format!("360 上传失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("360 上传返回 HTTP {}", response.status()));
    }
    let html = response.text().await.map_err(|error| format!("360 返回读取失败：{error}"))?;
    let image_url = {
        let document = Html::parse_document(&html);
        let selector = Selector::parse("[data-imgurl]").map_err(|error| error.to_string())?;
        document
            .select(&selector)
            .filter_map(|element| element.value().attr("data-imgurl"))
            .map(str::trim)
            .find(|value| value.starts_with("http://") || value.starts_with("https://"))
            .ok_or_else(|| "360 没有接受这张图片（没有返回 data-imgurl）".to_string())?
            .to_string()
    };
    let response = client
        .post("https://st.so.com/stu?a=mrecomm&start=0")
        .header(REFERER, "https://st.so.com/r?src=st&srcsp=home")
        .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9")
        .form(&[("img_url", image_url.as_str())])
        .send()
        .await
        .map_err(|error| format!("360 相似图请求失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("360 相似图返回 HTTP {}", response.status()));
    }
    let value: Value = response.json().await.map_err(|error| format!("360 相似图 JSON 解析失败：{error}"))?;
    let hits = parse_360_items(&value);
    if hits.is_empty() {
        return Err("360 没有返回相似图片".into());
    }
    Ok(hits)
}

fn parse_360_items(value: &Value) -> Vec<ReverseHit> {
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    let Some(items) = value.pointer("/data/result").and_then(Value::as_array) else { return hits };
    for item in items {
        let title = item.get("title").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let url = item
            .get("purl")
            .and_then(Value::as_str)
            .or_else(|| item.get("imgurl").and_then(Value::as_str))
            .unwrap_or("")
            .trim()
            .to_string();
        let url = clean_url(&url);
        if url.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        let image_url = item
            .get("imgurl")
            .and_then(Value::as_str)
            .map(clean_url)
            .unwrap_or_default();
        let site = item
            .get("site")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| host_of(&url));
        hits.push(ReverseHit {
            title: if title.is_empty() { site.clone() } else { title },
            site,
            url,
            image_url,
            thumbnail: String::new(),
        });
        if hits.len() >= MAX_REVERSE_HITS {
            break;
        }
    }
    hits
}

async fn search_trace_moe(client: &reqwest::Client, bytes: &[u8], mime: &str) -> Result<Vec<AnimeHit>, String> {
    let part = Part::bytes(bytes.to_vec())
        .file_name("image.jpg")
        .mime_str(mime)
        .map_err(|error| error.to_string())?;
    let form = Form::new().part("image", part);
    let response = client
        .post("https://api.trace.moe/search?anilistInfo=true")
        .multipart(form)
        .send()
        .await
        .map_err(|error| format!("trace.moe 请求失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("trace.moe 返回 HTTP {}", response.status()));
    }
    let value: Value = response.json().await.map_err(|error| format!("trace.moe JSON 解析失败：{error}"))?;
    let hits = parse_trace_items(&value);
    if hits.is_empty() {
        return Err("没有相似度 ≥ 85% 的动画场景".into());
    }
    Ok(hits)
}

fn parse_trace_items(value: &Value) -> Vec<AnimeHit> {
    let mut hits = Vec::new();
    let Some(items) = value.get("result").and_then(Value::as_array) else { return hits };
    for item in items {
        let similarity = item.get("similarity").and_then(Value::as_f64).unwrap_or(0.0);
        if similarity < 0.85 {
            continue;
        }
        let anilist = item.get("anilist");
        let title = anilist
            .and_then(|value| value.get("title"))
            .and_then(|value| {
                value.get("native").and_then(Value::as_str)
                    .or_else(|| value.get("romaji").and_then(Value::as_str))
                    .or_else(|| value.get("english").and_then(Value::as_str))
            })
            .or_else(|| item.get("filename").and_then(Value::as_str))
            .unwrap_or("动画场景")
            .to_string();
        let anilist_id = anilist.and_then(|value| value.get("id")).and_then(Value::as_i64);
        let url = anilist_id
            .map(|id| format!("https://anilist.co/anime/{id}"))
            .unwrap_or_else(|| "https://trace.moe/".to_string());
        let episode = item
            .get("episode")
            .and_then(|value| value.as_i64().map(|v| v.to_string()).or_else(|| value.as_str().map(str::to_string)))
            .unwrap_or_default();
        let from = item.get("from").and_then(Value::as_f64).unwrap_or(0.0);
        let minutes = (from / 60.0).floor() as i64;
        let seconds = (from % 60.0).floor() as i64;
        let at = format!("{minutes:02}:{seconds:02}");
        let image_url = item.get("image").and_then(Value::as_str).map(clean_url).unwrap_or_default();
        hits.push(AnimeHit {
            title,
            site: "trace.moe".into(),
            url,
            image_url,
            thumbnail: String::new(),
            episode,
            at,
            similarity,
        });
        if hits.len() >= 5 {
            break;
        }
    }
    hits
}

fn dedupe_search_hits(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for hit in hits {
        let url = clean_url(&hit.url);
        if url.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        out.push(SearchHit { title: hit.title, url, content: hit.content });
        if out.len() >= 20 {
            break;
        }
    }
    out
}

async fn enrich_thumbnails(reverse: &mut [ReverseHit], anime: &mut [AnimeHit]) {
    let reverse_urls: Vec<String> = reverse.iter().take(12).map(|hit| hit.image_url.clone()).filter(|url| !url.is_empty()).collect();
    let reverse_futures = reverse_urls.iter().map(|url| async move {
        tokio::time::timeout(Duration::from_secs(8), web_fetch::fetch_image_data_url(url))
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|data| shrink_data_url(&data))
    });
    let reverse_thumbs = join_all(reverse_futures).await;
    let mut thumb_index = 0;
    for hit in reverse.iter_mut().take(12) {
        if hit.image_url.is_empty() {
            continue;
        }
        if let Some(Some(thumb)) = reverse_thumbs.get(thumb_index) {
            hit.thumbnail = thumb.clone();
        }
        thumb_index += 1;
    }
    let anime_urls: Vec<String> = anime.iter().take(3).map(|hit| hit.image_url.clone()).filter(|url| !url.is_empty()).collect();
    let anime_futures = anime_urls.iter().map(|url| async move {
        tokio::time::timeout(Duration::from_secs(8), web_fetch::fetch_image_data_url(url))
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|data| shrink_data_url(&data))
    });
    let anime_thumbs = join_all(anime_futures).await;
    let mut thumb_index = 0;
    for hit in anime.iter_mut().take(3) {
        if hit.image_url.is_empty() {
            continue;
        }
        if let Some(Some(thumb)) = anime_thumbs.get(thumb_index) {
            hit.thumbnail = thumb.clone();
        }
        thumb_index += 1;
    }
}

fn shrink_data_url(data: &str) -> Option<String> {
    let (_, encoded) = data.split_once(',')?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let image = resize_to_max(image, 320);
    let rgb = flatten_alpha(image);
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 78).encode_image(&rgb).ok()?;
    Some(data_url(&out, "image/jpeg"))
}

fn merge_sources(
    synthesis: &Synthesis,
    reverse: &[ReverseHit],
    anime: &[AnimeHit],
    search: &[SearchHit],
) -> Vec<SourceCandidate> {
    let mut allowed = HashSet::new();
    for hit in reverse {
        allowed.insert(hit.url.clone());
    }
    for hit in anime {
        allowed.insert(hit.url.clone());
    }
    for hit in search {
        allowed.insert(hit.url.clone());
    }
    let mut thumbnails: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for hit in reverse {
        thumbnails.insert(hit.url.clone(), hit.thumbnail.clone());
    }
    for hit in anime {
        thumbnails.insert(hit.url.clone(), hit.thumbnail.clone());
    }
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    for source in &synthesis.sources {
        let url = clean_url(&source.url);
        if url.is_empty() || !allowed.contains(&url) || !seen.insert(url.clone()) {
            continue;
        }
        let site = if source.site.trim().is_empty() { host_of(&url) } else { source.site.trim().to_string() };
        sources.push(SourceCandidate {
            title: if source.title.trim().is_empty() { site.clone() } else { source.title.trim().to_string() },
            url: url.clone(),
            site,
            confidence: normalize_confidence(&source.confidence),
            kind: normalize_kind(&source.kind),
            evidence: source.evidence.trim().chars().take(300).collect(),
            thumbnail: thumbnails.get(&url).cloned().unwrap_or_default(),
        });
    }
    for hit in anime {
        if sources.iter().any(|source| source.url == hit.url) {
            continue;
        }
        let episode = if hit.episode.is_empty() { String::new() } else { format!(" · 第 {} 集", hit.episode) };
        sources.push(SourceCandidate {
            title: hit.title.clone(),
            url: hit.url.clone(),
            site: hit.site.clone(),
            confidence: "high".into(),
            kind: "anime".into(),
            evidence: format!("trace.moe 场景匹配 {:.0}%{episode} · {}", hit.similarity * 100.0, hit.at),
            thumbnail: hit.thumbnail.clone(),
        });
    }
    sources.truncate(10);
    sources
}

fn build_similar(reverse: &[ReverseHit], anime: &[AnimeHit]) -> Vec<SimilarImage> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for hit in anime {
        if !seen.insert(hit.url.clone()) {
            continue;
        }
        out.push(SimilarImage {
            title: format!("{} · {}{}", hit.title, hit.at, if hit.episode.is_empty() { String::new() } else { format!(" · 第 {} 集", hit.episode) }),
            url: hit.url.clone(),
            site: hit.site.clone(),
            image_url: hit.image_url.clone(),
            thumbnail: hit.thumbnail.clone(),
            provider: "trace.moe".into(),
        });
    }
    for hit in reverse {
        if !seen.insert(hit.url.clone()) {
            continue;
        }
        out.push(SimilarImage {
            title: hit.title.clone(),
            url: hit.url.clone(),
            site: hit.site.clone(),
            image_url: hit.image_url.clone(),
            thumbnail: hit.thumbnail.clone(),
            provider: "360".into(),
        });
        if out.len() >= 12 {
            break;
        }
    }
    out
}

fn fallback_summary(
    vision: &VisionAnalysis,
    reverse: &[ReverseHit],
    anime: &[AnimeHit],
    search: &[SearchHit],
    verdict: &str,
) -> String {
    if verdict == "not_found" {
        return "没有找到能确认这张图来源的结果。下面保留了反查和检索线索，可以打开核对；不要把它们当成已确认出处。".into();
    }
    let mut parts = Vec::new();
    if !vision.description.is_empty() {
        parts.push(format!("画面：{}", vision.description.chars().take(160).collect::<String>()));
    }
    if !reverse.is_empty() {
        parts.push(format!("360 识图返回 {} 个同图/相似页面", reverse.len()));
    }
    if !anime.is_empty() {
        parts.push(format!("trace.moe 命中 {} 个动画场景", anime.len()));
    }
    if !search.is_empty() {
        parts.push(format!("关键词检索返回 {} 条网页线索", search.len()));
    }
    parts.join("；") + "。这些是线索，不是逐像素核验过的出处。"
}

fn normalize_verdict(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "found" => "found".into(),
        "not_found" | "not-found" | "none" => "not_found".into(),
        _ => "likely".into(),
    }
}

fn clean_summary(value: &str) -> String {
    let mut text = value.trim();
    loop {
        let lower = text.to_ascii_lowercase();
        let mut matched = false;
        for prefix in [
            "结论：not_found，", "结论：not_found。", "结论：not_found",
            "结论：likely，", "结论：likely。", "结论：likely",
            "结论：found，", "结论：found。", "结论：found",
            "结论：", "结论:",
            "not_found:", "not_found：", "likely:", "likely：", "found:", "found：",
        ] {
            if lower.starts_with(prefix) {
                text = &text[prefix.len()..];
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
        text = text.trim_start_matches(|c: char| matches!(c, '：' | ':' | '，' | ',' | '。' | ' '));
    }
    text.chars().take(1600).collect()
}

fn normalize_confidence(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => "high".into(),
        "low" => "low".into(),
        _ => "medium".into(),
    }
}

fn normalize_kind(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "reverse" => "reverse".into(),
        "anime" => "anime".into(),
        "search" => "search".into(),
        _ => "reverse".into(),
    }
}

fn clean_url(value: &str) -> String {
    let value = value.trim();
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else {
        String::new()
    }
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_string))
        .unwrap_or_default()
}

fn parse_vision_analysis(content: &str) -> VisionAnalysis {
    let Some(value) = json_block(content) else {
        return VisionAnalysis {
            description: content.trim().chars().take(800).collect(),
            kind: "unknown".into(),
            ..VisionAnalysis::default()
        };
    };
    VisionAnalysis {
        description: value.get("description").and_then(Value::as_str).unwrap_or("").trim().chars().take(800).collect(),
        visible_text: string_list(value.get("visible_text"), 20, 240),
        entities: string_list(value.get("entities"), 20, 120),
        kind: value.get("kind").and_then(Value::as_str).unwrap_or("unknown").trim().to_string(),
        queries: string_list(value.get("queries"), 6, 120),
    }
}

fn parse_synthesis(content: &str) -> Synthesis {
    let Some(value) = json_block(content) else {
        return Synthesis {
            verdict: "likely".into(),
            summary: content.trim().chars().take(1200).collect(),
            ..Synthesis::default()
        };
    };
    let sources = value
        .get("sources")
        .or_else(|| value.get("source_candidates"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = clean_url(item.get("url").and_then(Value::as_str).unwrap_or(""));
                    if url.is_empty() {
                        return None;
                    }
                    Some(SynthesisSource {
                        title: item.get("title").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                        site: item.get("site").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                        confidence: item.get("confidence").and_then(Value::as_str).unwrap_or("medium").to_string(),
                        kind: item.get("kind").and_then(Value::as_str).unwrap_or("reverse").to_string(),
                        evidence: item.get("evidence").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                        url,
                    })
                })
                .take(12)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Synthesis {
        verdict: value.get("verdict").and_then(Value::as_str).unwrap_or("likely").to_string(),
        summary: value.get("summary").and_then(Value::as_str).unwrap_or("").trim().chars().take(1600).collect(),
        sources,
        next_queries: string_list(value.get("next_queries"), 6, 120),
    }
}

fn json_block(content: &str) -> Option<Value> {
    let trimmed = content.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&trimmed[start..=end]).ok()
}

fn string_list(value: Option<&Value>, max_items: usize, max_chars: usize) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|item| item.trim().chars().take(max_chars).collect::<String>())
                .filter(|item| !item.is_empty())
                .take(max_items)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vision_json_with_fence() {
        let analysis = parse_vision_analysis(
            "```json\n{\"description\":\"一张评论截图\",\"visible_text\":[\"2026-11-13\"],\"entities\":[\"B站\"],\"kind\":\"social\",\"queries\":[\"评论 原文\"]}\n```",
        );
        assert_eq!(analysis.kind, "social");
        assert_eq!(analysis.visible_text, vec!["2026-11-13"]);
        assert_eq!(analysis.queries, vec!["评论 原文"]);
    }

    #[test]
    fn parses_synthesis_and_rejects_non_http_urls() {
        let synthesis = parse_synthesis(
            "{\"verdict\":\"likely\",\"summary\":\"有线索\",\"sources\":[{\"title\":\"页面\",\"url\":\"https://example.com/a\",\"confidence\":\"high\",\"kind\":\"reverse\",\"evidence\":\"同图\"},{\"title\":\"坏链接\",\"url\":\"javascript:alert(1)\"}],\"next_queries\":[\"再查\"]}",
        );
        assert_eq!(synthesis.verdict, "likely");
        assert_eq!(synthesis.sources.len(), 1);
        assert_eq!(synthesis.sources[0].url, "https://example.com/a");
        assert_eq!(synthesis.next_queries, vec!["再查"]);
    }

    #[test]
    fn parses_360_items_and_dedupes() {
        let value = json!({
            "data": {
                "result": [
                    {"title": "页面 A", "site": "a.com", "purl": "https://a.com/1", "imgurl": "https://img.a.com/1.jpg"},
                    {"title": "重复", "site": "a.com", "purl": "https://a.com/1", "imgurl": "https://img.a.com/2.jpg"},
                    {"title": "页面 B", "site": "b.com", "purl": "http://b.com/2", "imgurl": ""}
                ]
            }
        });
        let hits = parse_360_items(&value);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "页面 A");
        assert_eq!(hits[1].url, "http://b.com/2");
    }

    #[test]
    fn trace_items_require_high_similarity() {
        let value = json!({
            "result": [
                {"similarity": 0.93, "anilist": {"id": 1, "title": {"native": "作品"}}, "episode": 2, "from": 65.0, "image": "https://trace.moe/a.jpg"},
                {"similarity": 0.4, "anilist": {"id": 2, "title": {"native": "噪声"}}, "episode": 1, "from": 1.0, "image": ""}
            ]
        });
        let hits = parse_trace_items(&value);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "作品");
        assert_eq!(hits[0].at, "01:05");
        assert_eq!(hits[0].episode, "2");
    }

    #[test]
    fn data_url_round_trip() {
        let data = data_url(b"hello", "text/plain");
        assert!(data.starts_with("data:text/plain;base64,"));
        assert!(shrink_data_url(&data).is_none());
    }

    #[test]
    fn summary_prefix_is_removed() {
        assert_eq!(clean_summary("结论：not_found，没有找到来源"), "没有找到来源");
        assert_eq!(clean_summary("likely：找到线索"), "找到线索");
        assert_eq!(clean_summary("正常结论：这张图..."), "正常结论：这张图...");
    }

    #[tokio::test]
    #[ignore = "live network probe"]
    async fn live_investigate_probe() {
        let image_path = std::env::var("PROBE_IMAGE").expect("PROBE_IMAGE");
        let key = std::env::var("PROBE_AI_KEY").expect("PROBE_AI_KEY");
        let search_key = std::env::var("PROBE_SEARCH_KEY").expect("PROBE_SEARCH_KEY");
        let bytes = std::fs::read(&image_path).expect("read image");
        let ai = AiConfig {
            provider: "deepseek".into(),
            protocol: "openai".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            model: std::env::var("PROBE_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into()),
            api_key: key,
            retries: 2,
        };
        let search = SearchConfig { provider: "deepseek".into(), api_key: search_key };
        let result = investigate(&ai, Some(&search), &bytes, "image/jpeg", "source", |_| {})
            .await
            .expect("investigate");
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        assert!(!result.similar.is_empty() || !result.sources.is_empty());
    }
}
