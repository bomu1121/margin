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

const SYSTEM: &str = r#"你是历史地图定位器：把一篇笔记压缩成「年份 + 地点」两个地图参数。
只输出 JSON，不要 Markdown、解释或代码围栏：
{"year":23,"place":"昆阳","person":"刘秀","event":"昆阳之战","confidence":0.86,"reason":"笔记标题与正文明确指向公元23年昆阳之战"}

规则：
1. year 是 -403 到 2026 的整数，公元前用负数；没有可靠年份写 null，不能写 0。
2. place 是中国古代或现代地名（如 昆阳、长安、蓝关、赤壁、应天府），用于地图地名解析；找不到写空字符串。
3. person 是笔记的中心人物；event 是中心事件。找不到写空字符串。
4. 只根据笔记内容推断，不补充外部知识，不猜测；不确定就降低 confidence。
5. confidence 是 0–1 的小数；reason 不超过 60 个汉字，说明依据。
6. 不是历史内容时 year=null、place=""、confidence=0。"#;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapContext {
    #[serde(default)]
    pub year: Option<i64>,
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
    let year = value.get("year").and_then(serde_json::Value::as_i64);
    let year = match year {
        Some(0) | None => None,
        Some(year) if (-403..=2026).contains(&year) => Some(year),
        Some(year) => return Err(format!("模型返回的年份超出范围：{year}")),
    };
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
        place: read("place", 40),
        person: read("person", 40),
        event: read("event", 60),
        confidence,
        reason: read("reason", 120),
        cached: false,
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

/// Resolve one note. The caller owns the AI config; this function caches by
/// note content hash so repeated clicks after an edit do not spend tokens.
pub async fn resolve(
    data_dir: &Path,
    note: &NoteDetail,
    ai: &AiConfig,
    cached_only: bool,
) -> Result<MapContext, String> {
    if !is_historical(note) {
        return Err("这篇笔记不是历史内容，地图没有可定位的年代".to_string());
    }
    let hash = content_hash(note);
    let mut cache = load_cache(data_dir);
    if let Some(entry) = cache.get(&note.meta.id) {
        if entry.hash == hash {
            let mut context = entry.context.clone();
            context.cached = true;
            return Ok(context);
        }
    }
    if cached_only {
        return Err("没有可复用的 AI 地图定位".to_string());
    }

    let token = CancellationToken::new();
    let response = clients::chat(ai, SYSTEM, &build_user_prompt(note), &token).await?;
    let context = parse_context(&response.content)?;
    cache.insert(
        note.meta.id.clone(),
        CacheEntry {
            hash,
            context: context.clone(),
        },
    );
    save_cache(data_dir, &cache)?;
    Ok(context)
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
    }

    #[test]
    fn history_gate_rejects_non_history_notes() {
        assert!(is_historical(&note(vec!["领域/历史"], vec![])));
        assert!(is_historical(&note(vec![], vec!["东汉初"])));
        assert!(!is_historical(&note(vec!["领域/半导体"], vec![])));
    }
}
