use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use uuid::Uuid;
use walkdir::WalkDir;

const SKIP_DIRS: &[&str] = &[
    ".obsidian",
    ".trash",
    ".yebian",
    ".git",
    "node_modules",
    "target",
    "Attachments",
    "90-Templates",
];

#[derive(Debug, Clone)]
pub struct ParsedNote {
    pub id: String,
    /// The `id:` key found in front matter, if the file carries one. Notes
    /// without it fall back to a hash of the relative path, which changes when
    /// the file moves — see `ensure_id` for how a move preserves identity.
    pub front_id: Option<String>,
    pub path: String,
    pub rel_path: String,
    pub title: String,
    pub up: String,
    pub date: String,
    pub domain: String,
    pub tags: Vec<String>,
    pub people: Vec<String>,
    pub events: Vec<String>,
    pub periods: Vec<String>,
    /// Every front-matter list key, including 公司/概念/custom fields. Kept
    /// only for in-memory matching; the index still stores the fixed columns.
    pub attributes: std::collections::HashMap<String, Vec<String>>,
    pub body: String,
    pub mtime: i64,
    pub mtime_ns: i64,
    pub size: i64,
    pub content_hash: String,
    pub run_id: String,
    pub chars: usize,
}

pub fn note_id(rel_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(rel_path.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

pub fn walk_notes(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref())
        })
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_md = path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        if is_md {
            out.push(path.to_path_buf());
        }
    }
    out.sort();
    out
}

pub fn rel_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Nanosecond precision is not needed, but seconds are too coarse: an editor
/// that saves twice inside the same second would be skipped by the incremental
/// index. Milliseconds are enough to separate human edits.
pub fn mtime_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub fn mtime_ns(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as i64)
        .unwrap_or(0)
}

pub fn file_size(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .map(|meta| meta.len() as i64)
        .unwrap_or(0)
}

fn front_flag(scalars: &std::collections::HashMap<String, String>, key: &str) -> bool {
    scalars
        .get(key)
        .map(|value| {
            let value = value.trim().trim_matches('"').trim_matches('\'').to_ascii_lowercase();
            matches!(value.as_str(), "true" | "1" | "yes" | "是")
        })
        .unwrap_or(false)
}

/// Whether a Markdown note carries the hidden marker. Sync uses this to keep a
/// stale remote copy from silently un-hiding a note that is hidden locally.
pub fn content_is_hidden(content: &str) -> bool {
    let content = content.trim_start_matches('\u{feff}');
    let (scalars, _, _) = split_front_matter(content);
    front_flag(&scalars, "隐藏") || front_flag(&scalars, "hidden")
}

pub fn parse_file_at(path: &Path, root: &Path, mtime: i64) -> Option<ParsedNote> {
    let content = fs::read_to_string(path).ok()?;
    let content = content.trim_start_matches('\u{feff}').to_string();
    let (scalars, lists, body) = split_front_matter(&content);
    if front_flag(&scalars, "隐藏") || front_flag(&scalars, "hidden") {
        return None;
    }

    let rel_path = rel_path(path, root);

    let mut title = scalars.get("标题").cloned().unwrap_or_default();
    if title.is_empty() {
        title = first_h1(&body).unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名".to_string())
        });
    }

    let mut tags = lists.get("tags").cloned().unwrap_or_default();
    // Legacy auto jobs wrote this tag; auto identity now lives on the job,
    // so keep it out of the index even when restoring an old artifact.
    tags.retain(|tag| tag != "自动制作");
    let up = scalars
        .get("作者")
        .cloned()
        .or_else(|| {
            tags.iter()
                .find(|tag| tag.starts_with("UP主名称/") || tag.starts_with("UP主/"))
                .map(|tag| {
                    tag.split_once('/')
                        .map(|(_, value)| value.to_string())
                        .unwrap_or_else(|| tag.clone())
                })
        })
        .unwrap_or_default();
    let domain = tags
        .iter()
        .find(|tag| tag.starts_with("领域/"))
        .map(|tag| {
            tag.split_once('/')
                .map(|(_, value)| value.to_string())
                .unwrap_or_else(|| "未分类".to_string())
        })
        .unwrap_or_else(|| "未分类".to_string());

    // B站笔记的 UP 主已经在 `作者` 里；把它作为派生标签提供给标签台/图谱，
    // 不要求每篇 front matter 再重复一条 `UP主/...`。已有该标签的旧笔记不重复。
    let is_bili = scalars
        .get("来源")
        .map(|value| value.contains("B站"))
        .unwrap_or(false)
        || scalars
            .get("原始链接")
            .map(|value| value.contains("bilibili.com"))
            .unwrap_or(false);
    if is_bili
        && !up.trim().is_empty()
        && !tags
            .iter()
            .any(|tag| tag.starts_with("UP主/") || tag.starts_with("UP主名称/"))
    {
        tags.push(format!("UP主/{}", up.trim()));
    }

    let run_id = path
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| name.starts_with("run_"))
        .unwrap_or_default();

    let body = strip_leading_h1(&body);
    let front_id = scalars
        .get("id")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let content_hash = body_hash(&body);
    let people = lists.get("人物").cloned().unwrap_or_default();
    let events = lists.get("事件").cloned().unwrap_or_default();
    let periods = lists.get("时期").cloned().unwrap_or_default();
    Some(ParsedNote {
        id: front_id.clone().unwrap_or_else(|| note_id(&rel_path)),
        front_id,
        path: display_path(path),
        rel_path,
        title: title.trim().to_string(),
        up: up.trim().to_string(),
        date: scalars.get("创建日期").cloned().unwrap_or_default().trim().to_string(),
        domain,
        tags,
        people,
        events,
        periods,
        attributes: lists,
        mtime_ns: mtime_ns(path),
        size: file_size(path),
        content_hash,
        chars: plain_text(&body).chars().count(),
        body,
        mtime,
        run_id,
    })
}

fn split_front_matter(content: &str) -> (std::collections::HashMap<String, String>, std::collections::HashMap<String, Vec<String>>, String) {
    use std::collections::HashMap;

    let mut scalars = HashMap::new();
    let mut lists: HashMap<String, Vec<String>> = HashMap::new();
    let normalized = content.replace("\r\n", "\n");

    if !normalized.starts_with("---\n") {
        return (scalars, lists, normalized);
    }
    let rest = &normalized[4..];
    let Some(end) = rest.find("\n---") else {
        return (scalars, lists, normalized);
    };
    let yaml = &rest[..end];
    let body = rest[end + 4..].trim_start_matches('\n').to_string();

    let mut current_key: Option<String> = None;
    for raw_line in yaml.lines() {
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") {
            if let Some(key) = &current_key {
                lists
                    .entry(key.clone())
                    .or_default()
                    .push(unquote(trimmed[2..].trim()));
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            current_key = Some(key.clone());
            if value.is_empty() {
                lists.entry(key).or_default();
            } else {
                scalars.insert(key, unquote(&value));
                current_key = None;
            }
        }
    }

    (scalars, lists, body)
}

pub(crate) fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let first = value.chars().next().unwrap();
        let last = value.chars().last().unwrap();
        if first == '"' && last == '"' {
            // `outbox::yaml` writes titles with serde_json escaping, so a title containing
            // ASCII quotes is stored as \" inside a double-quoted YAML scalar. Parse it back
            // instead of leaving the backslashes in the note title.
            if let Ok(parsed) = serde_json::from_str::<String>(value) {
                return parsed;
            }
            return value[1..value.len() - 1].to_string();
        }
        if first == '\'' && last == '\'' {
            return value[1..value.len() - 1].replace("''", "'");
        }
    }
    value.to_string()
}

/// Tauri's resource directory can come back with the Win32 extended-length
/// prefix. Strip it for display/copy; the filesystem calls still use the
/// original Path.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    text.strip_prefix("\\\\?\\").unwrap_or(&text).to_string()
}

fn first_h1(body: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("# ") {
            return Some(rest.trim().to_string());
        }
        if !line.is_empty() && !line.starts_with('#') {
            break;
        }
    }
    None
}

fn strip_leading_h1(body: &str) -> String {
    let mut lines = body.lines();
    let mut skipped = false;
    let mut out = Vec::new();
    for line in lines.by_ref() {
        if !skipped {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("# ") {
                skipped = true;
                continue;
            }
            skipped = true;
        }
        out.push(line);
    }
    out.join("\n").trim_start_matches('\n').to_string()
}

/// Every file in the vault (not just Markdown), used by the link rewriter to
/// resolve targets and to decide which relative paths need recomputing.
pub fn walk_all_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref())
        })
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            out.push(entry.path().to_path_buf());
        }
    }
    out.sort();
    out
}

/// Folders in the vault, relative and '/'-separated. Empty folders are
/// included: once the sidebar can create them, hiding them again would make
/// the tree disagree with the filesystem.
pub fn walk_folders(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref())
        })
        .filter_map(Result::ok)
    {
        if entry.file_type().is_dir() {
            out.push(rel_path(entry.path(), root));
        }
    }
    out.sort();
    out
}

/// Content fingerprint used to recognise a file that was moved outside the
/// app (Explorer, sync client) and therefore lost its path-derived id.
pub fn body_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.replace("\r\n", "\n").trim().as_bytes());
    hex::encode(hasher.finalize())[..32].to_string()
}

/// Text projection of a note body for search and character counts.
///
/// ` ```html ` visual blocks are presentation, not prose: style/script
/// contents and tags must not inflate the word count or the FTS index. Other
/// Markdown (including code fences) is left untouched.
pub fn plain_text(body: &str) -> String {
    let body = strip_html_comments(body);
    let mut out = String::with_capacity(body.len());
    let mut html_block: Option<Vec<&str>> = None;
    for line in body.lines() {
        if html_block.is_some() {
            if line.trim() == "```" {
                let block = html_block.take().unwrap_or_default().join("\n");
                out.push_str(&strip_html_tags(&block));
                out.push('\n');
            } else {
                if let Some(lines) = html_block.as_mut() {
                    lines.push(line);
                }
            }
            continue;
        }
        if line.trim().eq_ignore_ascii_case("```html") {
            html_block = Some(Vec::new());
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if let Some(lines) = html_block {
        out.push_str(&strip_html_tags(&lines.join("\n")));
        out.push('\n');
    }
    out
}

/// Hidden authoring metadata (for example `yebian-claims`) lives in HTML
/// comments. It must not become note text for counts or FTS.
fn strip_html_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let Some(end) = after.find("-->") else {
            return out;
        };
        rest = &after[end + 3..];
    }
    out.push_str(rest);
    out
}

fn strip_html_tags(html: &str) -> String {
    let chars: Vec<char> = html.chars().collect();
    let mut out = String::with_capacity(html.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '<' {
            let head: String = chars[index..(index + 7).min(chars.len())]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase();
            let (close, skip) = if head.starts_with("<style") {
                ("</style>", true)
            } else if head.starts_with("<script") {
                ("</script>", true)
            } else {
                (">", false)
            };
            let mut end = index;
            while end < chars.len() {
                let stop = (end + close.len()).min(chars.len());
                let window: String = chars[end..stop].iter().collect::<String>().to_ascii_lowercase();
                if window == close {
                    break;
                }
                end += 1;
            }
            if end < chars.len() {
                index = end + close.len();
            } else {
                index += 1;
            }
            if !skip {
                out.push(' ');
            }
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Check `plain_text` on the shapes the note engine writes.
#[cfg(test)]
mod plain_text_tests {
    use super::plain_text;

    #[test]
    fn html_visual_block_does_not_count_css_or_tags() {
        let body = "# 标题\n\n正文一段。\n\n```html\n<style>.card{color:#fff}</style>\n<div class='card'>卡片文字</div>\n<script>alert(1)</script>\n```\n\n结尾。\n";
        let text = plain_text(body);
        assert!(text.contains("卡片文字"), "{text}");
        assert!(text.contains("正文一段"), "{text}");
        assert!(!text.contains("color"), "{text}");
        assert!(!text.contains("alert"), "{text}");
        assert!(!text.contains("<div"), "{text}");
    }

    #[test]
    fn markdown_code_fences_are_kept() {
        let body = "示例：\n\n```ts\nconst value = 1;\n```\n";
        let text = plain_text(body);
        assert!(text.contains("const value = 1;"), "{text}");
    }

    #[test]
    fn hidden_html_comments_are_not_text() {
        let body = "正文。\n<!-- yebian-claims\n[{\"quote\":\"曹操\",\"kind\":\"待查\"}]\n-->\n结尾。\n";
        let text = plain_text(body);
        assert!(text.contains("正文。"), "{text}");
        assert!(text.contains("结尾。"), "{text}");
        assert!(!text.contains("yebian-claims"), "{text}");
        assert!(!text.contains("quote"), "{text}");
        assert!(!text.contains("曹操"), "{text}");
    }

    #[test]
    fn unclosed_html_comment_drops_its_body() {
        let body = "正文。\n<!-- yebian-claims\n[{\"quote\":\"曹操\"}]\n";
        let text = plain_text(body);
        assert!(text.contains("正文。"), "{text}");
        assert!(!text.contains("曹操"), "{text}");
    }
}

/// The `id:` value in front matter, if any.
pub fn front_id_of(content: &str) -> Option<String> {
    let normalized = content.replace("\r\n", "\n");
    if !normalized.starts_with("---\n") {
        return None;
    }
    let rest = &normalized[4..];
    let end = rest.find("\n---")?;
    for line in rest[..end].lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == "id" {
                let value = unquote(value.trim());
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

/// Writes `id:` into front matter unless the file already has one. This is how
/// a move keeps a note's identity: inject the *current* id first, then rename.
/// Returns true when the file was rewritten.
pub fn ensure_id(path: &Path, id: &str) -> Result<bool, String> {
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    if front_id_of(&raw).is_some() {
        return Ok(false);
    }
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let bom = if raw.starts_with('\u{feff}') { "\u{feff}" } else { "" };
    let without_bom = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    let updated = if let Some(body) = without_bom.strip_prefix("---") {
        // Keep the opening delimiter byte-for-byte, insert the new key after it.
        let (head, tail) = body.split_once('\n').unwrap_or(("", body));
        if head.trim().is_empty() {
            format!("{bom}---{}{eol}id: {id}{eol}{tail}", head.trim_end_matches('\r'))
        } else {
            // Malformed front matter; fall back to prepending a full block.
            format!("{bom}---{eol}id: {id}{eol}---{eol}{eol}{without_bom}")
        }
    } else {
        format!("{bom}---{eol}id: {id}{eol}---{eol}{eol}{without_bom}")
    };

    write_atomic(path, &updated)?;
    Ok(true)
}

/// Writes through a sibling temp file so a crash mid-write cannot truncate the
/// user's note. `rename` replaces the destination atomically on both platforms
/// we ship (same volume).
pub fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "文件没有父目录".to_string())?;
    let temp = parent.join(format!(
        ".yebian-{}.tmp",
        &Uuid::new_v4().simple().to_string()[..12]
    ));
    fs::write(&temp, content).map_err(|error| error.to_string())?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })?;
    Ok(())
}

/// Overwrites (or inserts) the front-matter `id`. Used when a duplicate id is
/// detected and one of the two files has to be re-keyed.
pub fn write_id(path: &Path, id: &str) -> Result<bool, String> {
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    if front_id_of(&raw).as_deref() == Some(id) {
        return Ok(false);
    }
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let bom = if raw.starts_with('\u{feff}') { "\u{feff}" } else { "" };
    let content = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    let updated = if let Some(rest) = content
        .strip_prefix("---\r\n")
        .or_else(|| content.strip_prefix("---\n"))
    {
        let body_start = content.len() - rest.len();
        match rest.find("\n---") {
            Some(relative_end) => {
                let front_end = body_start + relative_end + 1;
                let front = &content[body_start..front_end];
                let mut offset = body_start;
                let mut found: Option<(usize, usize)> = None;
                for line in front.split_inclusive('\n') {
                    let key = line.split(':').next().unwrap_or("").trim();
                    if key == "id" {
                        found = Some((offset, offset + line.len()));
                        break;
                    }
                    offset += line.len();
                }
                match found {
                    Some((start, end)) => format!(
                        "{bom}{}id: {id}{eol}{}",
                        &content[..start],
                        &content[end..]
                    ),
                    None => format!(
                        "{bom}{}id: {id}{eol}{}",
                        &content[..body_start],
                        &content[body_start..]
                    ),
                }
            }
            None => format!("{bom}---{eol}id: {id}{eol}---{eol}{eol}{content}"),
        }
    } else {
        format!("{bom}---{eol}id: {id}{eol}---{eol}{eol}{content}")
    };

    write_atomic(path, &updated)?;
    Ok(true)
}

const MAP_FRONT_KEYS: &[&str] = &[
    "地图年份",
    "地图地点",
    "地图人物",
    "地图事件",
    "地图来源",
    "地图置信度",
    "地图依据",
    "地图内容哈希",
];

/// The small, flat map context that is written into a note's front matter.
/// Flat keys keep `split_front_matter` (and YAML in general) boring: no nested
/// mapping, no array, and every value is a scalar the existing parser handles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapFields {
    /// First valid year. Kept for callers that need a single anchor.
    pub year: Option<i64>,
    /// Raw route text, e.g. `23` or `1079,1080,1094`.
    pub year_text: String,
    pub place: String,
    pub person: String,
    pub event: String,
    pub source: String,
    pub confidence: Option<f64>,
    pub reason: String,
    /// Hash of the note text the context was derived from. Empty on old notes.
    pub content_hash: String,
}

fn map_scalar(value: &str) -> String {
    value.replace(['\r', '\n'], " ").trim().to_string()
}

/// Parse a comma-separated year route. Invalid tokens are dropped; callers that
/// need validation should use `validate_year_route` first.
pub fn parse_year_route(value: &str) -> Vec<i64> {
    value
        .replace('，', ",")
        .split(',')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .filter(|year| *year != 0 && (-403..=2026).contains(year))
        .collect()
}

/// Strict route parser used before writing a note: every non-empty token must
/// be a valid year, otherwise a typo would silently become a different route.
pub fn validate_year_route(value: &str) -> Result<Vec<i64>, String> {
    let mut years = Vec::new();
    for part in value.replace('，', ",").split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let year = part
            .parse::<i64>()
            .map_err(|_| format!("年份不是整数：{part}"))?;
        if year == 0 || !(-403..=2026).contains(&year) {
            return Err(format!("年份超出范围：{year}"));
        }
        years.push(year);
    }
    if years.is_empty() {
        return Err("至少需要一个年份".to_string());
    }
    Ok(years)
}

fn push_map_text(out: &mut Vec<String>, key: &str, value: &str) -> Result<(), String> {
    let value = map_scalar(value);
    if value.is_empty() {
        return Ok(());
    }
    let quoted = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    out.push(format!("{key}: {quoted}"));
    Ok(())
}

/// Read `地图*` fields from a note file. Returns `None` when neither a year nor
/// a place is present, so an empty/stale block does not count as a saved map.
pub fn read_map_fields(path: &Path) -> Option<MapFields> {
    let raw = fs::read_to_string(path).ok()?;
    let content = raw.trim_start_matches('\u{feff}');
    let (scalars, _, _) = split_front_matter(content);
    let year_text = map_scalar(scalars.get("地图年份").map(String::as_str).unwrap_or(""));
    let year = parse_year_route(&year_text).first().copied();
    let place = map_scalar(scalars.get("地图地点").map(String::as_str).unwrap_or(""));
    if year.is_none() && place.is_empty() {
        return None;
    }
    let confidence = scalars
        .get("地图置信度")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .map(|value| value.clamp(0.0, 1.0));
    Some(MapFields {
        year,
        year_text,
        place,
        person: map_scalar(scalars.get("地图人物").map(String::as_str).unwrap_or("")),
        event: map_scalar(scalars.get("地图事件").map(String::as_str).unwrap_or("")),
        source: {
            let source = map_scalar(scalars.get("地图来源").map(String::as_str).unwrap_or(""));
            if source.is_empty() { "manual".to_string() } else { source }
        },
        confidence,
        reason: map_scalar(scalars.get("地图依据").map(String::as_str).unwrap_or("")),
        content_hash: map_scalar(scalars.get("地图内容哈希").map(String::as_str).unwrap_or("")),
    })
}

/// Replace the `地图*` block while preserving all other front-matter keys and
/// the body. Existing map keys are removed first, so clearing `place` does not
/// leave a stale value behind.
pub fn write_map_fields(path: &Path, fields: &MapFields) -> Result<(), String> {
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let bom = if raw.starts_with('\u{feff}') { "\u{feff}" } else { "" };
    let content = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
    let normalized = content.replace("\r\n", "\n");

    let mut map_lines: Vec<String> = Vec::new();
    let year_text = {
        let raw = map_scalar(&fields.year_text);
        if raw.is_empty() {
            fields
                .year
                .filter(|year| *year != 0 && (-403..=2026).contains(year))
                .map(|year| year.to_string())
                .unwrap_or_default()
        } else {
            raw
        }
    };
    if !parse_year_route(&year_text).is_empty() {
        // Single years stay numeric for old notes/tools; routes are quoted so
        // YAML never mistakes 1079,1080 for a mapping or flow sequence.
        if year_text.parse::<i64>().is_ok() {
            map_lines.push(format!("地图年份: {year_text}"));
        } else {
            let quoted = serde_json::to_string(&year_text).map_err(|error| error.to_string())?;
            map_lines.push(format!("地图年份: {quoted}"));
        }
    }
    push_map_text(&mut map_lines, "地图地点", &fields.place)?;
    // Without a year or place there is no map to open; clear the whole block
    // rather than leaving a source/confidence remnant that reads as saved.
    if !map_lines.is_empty() {
        push_map_text(&mut map_lines, "地图人物", &fields.person)?;
        push_map_text(&mut map_lines, "地图事件", &fields.event)?;
        push_map_text(&mut map_lines, "地图来源", &fields.source)?;
        if let Some(confidence) = fields.confidence.map(|value| value.clamp(0.0, 1.0)) {
            let rounded = (confidence * 100.0).round() / 100.0;
            map_lines.push(format!("地图置信度: {rounded}"));
        }
        push_map_text(&mut map_lines, "地图依据", &fields.reason)?;
        push_map_text(&mut map_lines, "地图内容哈希", &fields.content_hash)?;
    }

    let updated = if let Some(rest) = normalized.strip_prefix("---\n") {
        let Some(end) = rest.find("\n---") else {
            return Err("front matter 不完整，未写入地图上下文".to_string());
        };
        let yaml = &rest[..end];
        let tail = &rest[end + 4..];
        let mut kept: Vec<&str> = yaml
            .lines()
            .filter(|line| {
                let key = line.split_once(':').map(|(key, _)| key.trim()).unwrap_or("");
                !MAP_FRONT_KEYS.contains(&key)
            })
            .collect();
        while kept.last().is_some_and(|line| line.trim().is_empty()) {
            kept.pop();
        }
        let mut next_yaml = kept.join("\n");
        if !map_lines.is_empty() {
            if !next_yaml.is_empty() {
                next_yaml.push('\n');
            }
            next_yaml.push_str(&map_lines.join("\n"));
        }
        if next_yaml.is_empty() {
            format!("{bom}---{eol}---{tail}")
        } else {
            format!(
                "{bom}---{eol}{}{eol}---{tail}",
                next_yaml.replace('\n', eol)
            )
        }
    } else if map_lines.is_empty() {
        format!("{bom}{content}")
    } else {
        format!(
            "{bom}---{eol}{}{eol}---{eol}{eol}{content}",
            map_lines.join(&eol.to_string())
        )
    };

    write_atomic(path, &updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_file(front: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("yebian-vault-tags-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("note.md");
        std::fs::write(&path, format!("---\n{front}\n---\n\n正文")).unwrap();
        (root, path)
    }

    #[test]
    fn bili_author_is_exposed_as_derived_up_tag() {
        let (root, path) = note_file("标题: 测试\n来源: 文本?\n作者: 不必\n");
        std::fs::write(
            &path,
            "---\n标题: 测试\n来源: B站\n作者: 小丑朱迪\n---\n\n正文",
        )
        .unwrap();
        let parsed = parse_file_at(&path, &root, 0).unwrap();
        assert_eq!(parsed.up, "小丑朱迪");
        assert!(parsed.tags.iter().any(|tag| tag == "UP主/小丑朱迪"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_auto_tag_is_hidden_from_the_index() {
        let (root, path) = note_file("标题: 测试
来源: B站
作者: 小丑朱迪
tags:
  - \"自动制作\"
");
        let parsed = parse_file_at(&path, &root, 0).unwrap();
        assert!(!parsed.tags.iter().any(|tag| tag == "自动制作"));
        assert!(parsed.tags.iter().any(|tag| tag == "UP主/小丑朱迪"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn non_bili_author_is_not_an_up_tag() {
        let (root, path) = note_file("标题: 测试\n来源: 文本\n作者: 某作者\n");
        let parsed = parse_file_at(&path, &root, 0).unwrap();
        assert_eq!(parsed.up, "某作者");
        assert!(!parsed.tags.iter().any(|tag| tag.starts_with("UP主/")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn hidden_note_is_not_indexed() {
        let (root, path) = note_file("标题: 测试\n隐藏: true\n");
        assert!(parse_file_at(&path, &root, 0).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn hidden_marker_is_detected_in_raw_markdown() {
        assert!(content_is_hidden("---\n隐藏: true\n标题: 测试\n---\n\n正文"));
        assert!(content_is_hidden("\u{feff}---\nhidden: \"是\"\n---\n"));
        assert!(content_is_hidden("---\nhidden: true\n---\n"));
        assert!(!content_is_hidden("---\n隐藏: false\n---\n"));
        assert!(!content_is_hidden("没有 frontmatter"));
    }

    #[test]
    fn escaped_quotes_in_front_matter_title_are_unescaped() {
        let (root, path) = note_file("标题: \"黄仁勋\\\"辞职十一天\\\"后官复原职\"\n");
        let parsed = parse_file_at(&path, &root, 0).unwrap();
        assert_eq!(parsed.title, "黄仁勋\"辞职十一天\"后官复原职");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn single_quoted_yaml_uses_doubled_quote_escape() {
        assert_eq!(unquote("'it''s'"), "it's");
        assert_eq!(unquote("\"plain\""), "plain");
        assert_eq!(unquote("unquoted"), "unquoted");
    }

    #[test]
    fn map_fields_round_trip_with_crlf_and_bom() {
        let root = std::env::temp_dir().join(format!(
            "yebian-map-fields-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("note.md");
        std::fs::write(
            &path,
            "\u{feff}---\r\n标题: \"测试\"\r\ntags:\r\n  - \"领域/历史\"\r\n---\r\n\r\n正文\r\n",
        )
        .unwrap();
        let fields = MapFields {
            year: Some(23),
            year_text: "23".into(),
            place: "昆阳".into(),
            person: "刘秀".into(),
            event: "昆阳之战".into(),
            source: "ai".into(),
            confidence: Some(0.86),
            reason: "标题与正文明确指向昆阳之战".into(),
            content_hash: String::new(),
        };
        write_map_fields(&path, &fields).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with('\u{feff}'));
        assert!(text.contains("标题: \"测试\""));
        assert!(text.contains("tags:\r\n  - \"领域/历史\""));
        assert!(text.contains("地图年份: 23"));
        assert!(text.contains("地图地点: \"昆阳\""));
        assert!(text.contains("\r\n"));
        assert!(text.contains("正文"));
        assert_eq!(read_map_fields(&path).unwrap(), fields);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn map_fields_replace_and_clear_without_touching_other_keys() {
        let root = std::env::temp_dir().join(format!(
            "yebian-map-replace-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("note.md");
        std::fs::write(
            &path,
            "---\n标题: \"测试\"\n地图年份: 200\n地图地点: \"赤壁\"\n作者: \"某人\"\n---\n\n正文\n",
        )
        .unwrap();
        let fields = MapFields {
            year: Some(208),
            year_text: "208".into(),
            place: String::new(),
            person: String::new(),
            event: String::new(),
            source: "manual".into(),
            confidence: Some(1.0),
            reason: String::new(),
            content_hash: String::new(),
        };
        write_map_fields(&path, &fields).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("地图年份: 208"));
        assert!(!text.contains("地图地点"));
        assert!(text.contains("标题: \"测试\""));
        assert!(text.contains("作者: \"某人\""));
        assert_eq!(read_map_fields(&path).unwrap().year, Some(208));

        write_map_fields(&path, &MapFields::default()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("地图年份"));
        assert!(!text.contains("地图来源"));
        assert!(text.contains("标题: \"测试\""));
        assert!(read_map_fields(&path).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn map_fields_keep_multi_stop_year_and_place_routes() {
        let (root, path) = note_file("标题: 测试\n");
        let fields = MapFields {
            year: Some(1079),
            year_text: "1079,1080,1094".into(),
            place: "湖州,黄州,惠州".into(),
            person: "苏轼".into(),
            source: "manual".into(),
            confidence: Some(1.0),
            content_hash: "abc123".into(),
            ..Default::default()
        };
        write_map_fields(&path, &fields).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("地图年份: \"1079,1080,1094\""));
        assert!(text.contains("地图地点: \"湖州,黄州,惠州\""));
        let read = read_map_fields(&path).unwrap();
        assert_eq!(read.year, Some(1079));
        assert_eq!(read.year_text, "1079,1080,1094");
        assert_eq!(read.place, "湖州,黄州,惠州");
        assert_eq!(read.person, "苏轼");
        assert_eq!(read.content_hash, "abc123");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn map_fields_create_front_matter_when_missing() {
        let root = std::env::temp_dir().join(format!(
            "yebian-map-create-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("note.md");
        std::fs::write(&path, "# 标题\n\n正文\n").unwrap();
        let fields = MapFields {
            year: Some(-221),
            place: "咸阳".into(),
            source: "manual".into(),
            confidence: Some(1.0),
            ..Default::default()
        };
        write_map_fields(&path, &fields).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("---\n地图年份: -221\n"));
        assert!(text.contains("# 标题"));
        assert_eq!(read_map_fields(&path).unwrap().place, "咸阳");
        let _ = std::fs::remove_dir_all(root);
    }
}
