//! AI-assisted context resolution for the Jiangshan atlas panel.
//!
//! The map iframe must never receive the note. This module turns the note into
//! a tiny `{ year, place, person, event, confidence, reason }` object, caches it
//! by a content hash, and returns only that object to the desktop host.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::NoteDetail;
use crate::production::clients::{self, AiConfig};
use tokio_util::sync::CancellationToken;

const CACHE_FILE: &str = "map-context-cache.json";
const MAX_BODY_CHARS: usize = 8_000;

const SYSTEM: &str = r#"你是历史地图定位器：把一篇笔记压缩成地图参数。
只输出 JSON，不要 Markdown、解释或代码围栏：
{"year":23,"place":"昆阳","person":"刘秀","event":"昆阳之战","confidence":0.86,"reason":"笔记标题与正文明确指向公元23年昆阳之战"}

规则：
1. year 是 -403 到 2026 的整数，公元前用负数；没有可靠年份写 null，不能写 0。跨年路线可以用逗号字符串，如 "1079,1080,1094"。
2. place 是中国古代或现代地名（如 昆阳、长安、蓝关、赤壁、应天府），找不到写空字符串。多站路线用逗号，如 "湖州,黄州,惠州"；同一年多点只写一个 year 加逗号 place，跨年路线按顺序配对。
3. person 是笔记的中心人物，可以是一个名字或逗号分隔的 2–4 人；event 是中心事件的短名称（如 昆阳之战），找不到写空字符串。
4. 只根据笔记内容推断，不补充外部知识，不猜测；不确定就降低 confidence。
5. confidence 是 0–1 的小数；reason 不超过 60 个汉字，说明依据。
6. 不是历史内容时 year=null、place=""、confidence=0。"#;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapContext {
    #[serde(default)]
    pub year: Option<i64>,
    /// Raw multi-stop route, e.g. `1079,1080,1094`; `year` is the first stop.
    #[serde(default)]
    pub year_text: String,
    #[serde(default)]
    pub place: String,
    #[serde(default)]
    pub person: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub cached: bool,
    /// The note body changed after the saved context was written.
    #[serde(default)]
    pub stale: bool,
    /// Where the values currently shown came from: `note`, `ai` or `cache`.
    #[serde(default)]
    pub source: String,
    /// For a saved note, who produced it: `ai` or `manual`.
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub uncached_input_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub provider_cost: Option<f64>,
    #[serde(default)]
    pub provider_currency: String,
    /// CNY estimate produced by the production price catalog.
    #[serde(default)]
    pub estimated_cost: f64,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub priced: bool,
    #[serde(default)]
    pub model: String,
}

/// The payload of `map_context_save`; the host sends only what the panel can
/// see, and Rust validates ranges before touching the note file.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapContextInput {
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub year_text: Option<String>,
    #[serde(default)]
    pub place: String,
    #[serde(default)]
    pub person: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheEntry {
    hash: String,
    context: MapContext,
}

fn cache_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(CACHE_FILE)
}

fn load_cache(data_dir: &Path) -> BTreeMap<String, CacheEntry> {
    let path = cache_path(data_dir);
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_cache(data_dir: &Path, cache: &BTreeMap<String, CacheEntry>) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
    let target = cache_path(data_dir);
    let temp = data_dir.join(format!("{CACHE_FILE}.tmp"));
    let text = serde_json::to_string_pretty(cache).map_err(|error| error.to_string())?;
    let mut file = std::fs::File::create(&temp).map_err(|error| error.to_string())?;
    file.write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    std::fs::rename(&temp, &target).map_err(|error| error.to_string())?;
    Ok(())
}

fn content_hash(note: &NoteDetail) -> String {
    let meta = &note.meta;
    let mut hasher = Sha256::new();
    for value in [
        meta.title.as_str(),
        note.body.as_str(),
        meta.tags.join("|").as_str(),
        meta.periods.join("|").as_str(),
        meta.people.join("|").as_str(),
        meta.events.join("|").as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

fn is_historical(note: &NoteDetail) -> bool {
    let meta = &note.meta;
    meta.tags.iter().any(|tag| tag == "领域/历史")
        || !meta.periods.is_empty()
}

fn parse_context(content: &str) -> Result<MapContext, String> {
    let raw = clients::strip_fence(content);
    let start = raw.find('{').ok_or_else(|| "模型没有返回 JSON".to_string())?;
    let end = raw.rfind('}').ok_or_else(|| "模型没有返回完整 JSON".to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&raw[start..=end]).map_err(|error| format!("模型 JSON 无法解析：{error}"))?;
    let year_text = match value.get("year") {
        Some(serde_json::Value::Number(number)) => {
            number.as_i64().map(|year| year.to_string()).unwrap_or_default()
        }
        Some(serde_json::Value::String(text)) => text.trim().to_string(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(serde_json::Value::as_i64)
            .map(|year| year.to_string())
            .collect::<Vec<_>>()
            .join(","),
        _ => String::new(),
    };
    let year_text = if year_text == "0" { String::new() } else { year_text };
    let years = if year_text.is_empty() {
        Vec::new()
    } else {
        crate::vault::validate_year_route(&year_text)
            .map_err(|error| format!("模型返回的年份无效：{error}"))?
    };
    let year = years.first().copied();
    let read = |key: &str, max: usize| -> String {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .chars()
            .take(max)
            .collect()
    };
    let confidence = value
        .get("confidence")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    Ok(MapContext {
        year,
        year_text,
        place: read("place", 120),
        person: read("person", 80),
        event: read("event", 80),
        confidence,
        reason: read("reason", 120),
        ..Default::default()
    })
}

fn build_user_prompt(note: &NoteDetail) -> String {
    let meta = &note.meta;
    let body: String = note.body.chars().take(MAX_BODY_CHARS).collect();
    format!(
        "标题：{}\n标签：{}\n时期：{}\n人物：{}\n事件：{}\n\n正文：\n{}",
        meta.title,
        meta.tags.join("、"),
        meta.periods.join("、"),
        meta.people.join("、"),
        meta.events.join("、"),
        body
    )
}

/// Read the context saved in the note's own front matter. This is the durable
/// copy: it travels with the note through sync and does not cost a model call.
pub fn from_note(note: &NoteDetail) -> Option<MapContext> {
    let fields = crate::vault::read_map_fields(Path::new(&note.meta.path))?;
    let stale = !fields.content_hash.is_empty() && fields.content_hash != content_hash(note);
    Some(MapContext {
        stale,
        year: fields.year,
        year_text: fields.year_text,
        place: fields.place,
        person: fields.person,
        event: fields.event,
        confidence: fields.confidence.unwrap_or(0.0),
        reason: fields.reason,
        cached: false,
        source: "note".to_string(),
        origin: fields.source,
        ..Default::default()
    })
}

fn cached_context(data_dir: &Path, note: &NoteDetail) -> Option<MapContext> {
    let cache = load_cache(data_dir);
    let entry = cache.get(&note.meta.id)?;
    if entry.hash != content_hash(note) {
        return None;
    }
    let mut context = entry.context.clone();
    context.cached = true;
    context.source = "cache".to_string();
    context.prompt_tokens = 0;
    context.uncached_input_tokens = 0;
    context.completion_tokens = 0;
    context.cached_tokens = 0;
    context.cache_write_tokens = 0;
    context.reasoning_tokens = 0;
    context.duration_ms = 0;
    context.provider_cost = None;
    context.provider_currency.clear();
    context.estimated_cost = 0.0;
    context.currency.clear();
    context.priced = false;
    Some(context)
}

/// Resolve one note in this order: saved note fields, cached AI result, then a
/// live model call. The note wins so a reviewed value is never silently
/// replaced by a fresh guess.
pub async fn resolve(
    data_dir: &Path,
    note: &NoteDetail,
    ai: Option<&AiConfig>,
    cached_only: bool,
) -> Result<MapContext, String> {
    if let Some(context) = from_note(note) {
        return Ok(context);
    }
    if !is_historical(note) {
        return Err("这篇笔记不是历史内容，地图没有可定位的年代".to_string());
    }
    if let Some(context) = cached_context(data_dir, note) {
        return Ok(context);
    }
    if cached_only {
        return Err("没有可复用的 AI 地图定位".to_string());
    }
    let ai = ai.ok_or_else(|| "还没有配置 AI 模型，请先在设置里填写 API Key".to_string())?;

    let token = CancellationToken::new();
    let started = std::time::Instant::now();
    let response = clients::chat(ai, SYSTEM, &build_user_prompt(note), &token).await?;
    let duration_ms = started.elapsed().as_millis() as u64;
    let mut context = parse_context(&response.content)?;
    context.source = "ai".to_string();
    context.origin = "ai".to_string();
    context.prompt_tokens = response.prompt_tokens;
    context.uncached_input_tokens = response.uncached_input_tokens;
    context.completion_tokens = response.completion_tokens;
    context.cached_tokens = response.cached_tokens;
    context.cache_write_tokens = response.cache_write_tokens;
    context.reasoning_tokens = response.reasoning_tokens;
    context.duration_ms = duration_ms;
    context.provider_cost = response.provider_cost;
    context.provider_currency = response.provider_currency.unwrap_or_default();
    context.model = ai.model.clone();
    if context.year.is_some() || !context.place.is_empty() {
        let mut cache = load_cache(data_dir);
        cache.insert(
            note.meta.id.clone(),
            CacheEntry {
                hash: content_hash(note),
                context: context.clone(),
            },
        );
        save_cache(data_dir, &cache)?;
    }
    Ok(context)
}

fn clean_field(value: &str, max: usize) -> String {
    value.replace(['\r', '\n'], " ").trim().chars().take(max).collect()
}

/// Persist a reviewed context into the note front matter. This is deliberately
/// a separate command: AI results and manual edits only become durable when the
/// user clicks "存入笔记".
pub fn save_note(note: &NoteDetail, input: MapContextInput) -> Result<MapContext, String> {
    let year_text = input
        .year_text
        .as_deref()
        .map(|value| clean_field(value, 120).replace('，', ","))
        .filter(|value| !value.is_empty())
        .or_else(|| input.year.map(|year| year.to_string()))
        .unwrap_or_default();
    let years = crate::vault::validate_year_route(&year_text)?;
    let year = years.first().copied();
    let place = clean_field(&input.place, 180)
        .replace('，', ",")
        .replace('；', ";");
    if note.meta.path.trim().is_empty() {
        return Err("笔记没有可写入的文件路径".to_string());
    }
    let source = match input.source.as_deref() {
        Some("ai") => "ai",
        _ => "manual",
    };
    let confidence = input
        .confidence
        .unwrap_or(if source == "ai" { 0.0 } else { 1.0 })
        .clamp(0.0, 1.0);
    let fields = crate::vault::MapFields {
        year,
        year_text: year_text.clone(),
        place: place.clone(),
        person: clean_field(&input.person, 120),
        event: clean_field(&input.event, 80),
        source: source.to_string(),
        confidence: Some(confidence),
        reason: clean_field(&input.reason, 120),
        content_hash: content_hash(note),
    };
    crate::vault::write_map_fields(Path::new(&note.meta.path), &fields)?;
    Ok(MapContext {
        year,
        year_text,
        place,
        person: fields.person,
        event: fields.event,
        confidence,
        reason: fields.reason,
        cached: false,
        source: "note".to_string(),
        origin: source.to_string(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NoteDetail, NoteMeta};

    fn note(tags: Vec<&str>, periods: Vec<&str>) -> NoteDetail {
        NoteDetail {
            meta: NoteMeta {
                id: "n1".into(),
                path: String::new(),
                rel_path: String::new(),
                title: "刘秀的军事能力：昆阳冲锋".into(),
                up: String::new(),
                date: String::new(),
                domain: String::new(),
                tags: tags.into_iter().map(str::to_string).collect(),
                events: vec!["昆阳之战".into()],
                people: vec!["刘秀".into()],
                periods: periods.into_iter().map(str::to_string).collect(),
                run_id: String::new(),
                mtime: 0,
                chars: 0,
                annotation_count: 0,
                is_new: false,
            },
            body: "公元23年，刘秀在昆阳击败王莽军。".into(),
            artifact: None,
        }
    }

    #[test]
    fn parses_and_clamps_model_json() {
        let context = parse_context(
            r#"```json
            {"year":23,"place":"昆阳","person":"刘秀","event":"昆阳之战","confidence":1.4,"reason":"正文明确"}
            ```"#,
        )
        .unwrap();
        assert_eq!(context.year, Some(23));
        assert_eq!(context.place, "昆阳");
        assert_eq!(context.confidence, 1.0);
        assert!(parse_context(r#"{"year":9999}"#).is_err());
        assert_eq!(parse_context(r#"{"year":0,"place":""}"#).unwrap().year, None);
        let route = parse_context(
            r#"{"year":"1079,1080,1094","place":"湖州,黄州,惠州","confidence":0.8}"#,
        )
        .unwrap();
        assert_eq!(route.year, Some(1079));
        assert_eq!(route.year_text, "1079,1080,1094");
        assert_eq!(route.place, "湖州,黄州,惠州");
    }

    #[test]
    fn history_gate_rejects_non_history_notes() {
        assert!(is_historical(&note(vec!["领域/历史"], vec![])));
        assert!(is_historical(&note(vec![], vec!["东汉初"])));
        assert!(!is_historical(&note(vec!["领域/半导体"], vec![])));
    }

    fn note_on_disk(label: &str) -> (std::path::PathBuf, NoteDetail) {
        let root = std::env::temp_dir().join(format!(
            "yebian-map-context-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("note.md");
        std::fs::write(
            &path,
            "---\n标题: 刘秀的军事能力\n  \ntags:\n  - \"领域/历史\"\n---\n\n公元23年，刘秀在昆阳击败王莽军。\n",
        )
        .unwrap();
        let mut detail = note(vec!["领域/历史"], vec![]);
        detail.meta.path = path.to_string_lossy().to_string();
        detail.meta.rel_path = "note.md".into();
        (root, detail)
    }

    #[test]
    fn saved_note_context_is_durable_and_wins() {
        let (root, detail) = note_on_disk("save");
        let saved = save_note(
            &detail,
            MapContextInput {
                year: Some(23),
                year_text: None,
                place: "昆阳".into(),
                person: "刘秀".into(),
                event: "昆阳之战".into(),
                source: Some("ai".into()),
                confidence: Some(0.86),
                reason: "标题与正文明确指向昆阳之战".into(),
            },
        )
        .unwrap();
        assert_eq!(saved.source, "note");
        assert_eq!(saved.origin, "ai");
        assert_eq!(saved.year, Some(23));
        assert_eq!(saved.place, "昆阳");
        assert!((saved.confidence - 0.86).abs() < 0.001);

        let text = std::fs::read_to_string(&detail.meta.path).unwrap();
        assert!(text.contains("地图年份: 23"));
        assert!(text.contains("地图地点: \"昆阳\""));
        assert!(text.contains("地图内容哈希:"));
        assert!(text.contains("标题: 刘秀的军事能力"));
        assert!(text.contains("tags:"));
        let read = from_note(&detail).unwrap();
        assert_eq!(read.year, Some(23));
        assert_eq!(read.place, "昆阳");
        assert_eq!(read.origin, "ai");
        assert!(!read.stale);

        let mut changed = note(vec!["领域/历史"], vec![]);
        changed.meta.path = detail.meta.path.clone();
        changed.body = "正文已经改过了。".into();
        assert!(from_note(&changed).unwrap().stale);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn save_note_persists_multi_stop_routes() {
        let (root, detail) = note_on_disk("route");
        let saved = save_note(
            &detail,
            MapContextInput {
                year: None,
                year_text: Some("1079,1080,1094".into()),
                place: "湖州,黄州,惠州".into(),
                person: "苏轼".into(),
                source: Some("manual".into()),
                confidence: Some(1.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(saved.year, Some(1079));
        assert_eq!(saved.year_text, "1079,1080,1094");
        assert_eq!(saved.place, "湖州,黄州,惠州");
        let text = std::fs::read_to_string(&detail.meta.path).unwrap();
        assert!(text.contains("地图年份: \"1079,1080,1094\""));
        assert!(text.contains("地图地点: \"湖州,黄州,惠州\""));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn save_note_validates_year_and_requires_an_anchor() {
        let (root, detail) = note_on_disk("validate");
        assert!(save_note(
            &detail,
            MapContextInput {
                year: Some(9999),
                place: "昆阳".into(),
                ..Default::default()
            }
        )
        .is_err());
        assert!(save_note(&detail, MapContextInput::default()).is_err());
        assert!(save_note(
            &detail,
            MapContextInput {
                year: None,
                place: "昆阳".into(),
                ..Default::default()
            }
        )
        .is_err());

        let saved = save_note(
            &detail,
            MapContextInput {
                year: Some(23),
                place: String::new(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(saved.origin, "manual");
        assert!((saved.confidence - 1.0).abs() < 0.001);
        let _ = std::fs::remove_dir_all(root);
    }
}
