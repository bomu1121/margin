use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const SKIP_DIRS: &[&str] = &[
    ".obsidian",
    ".trash",
    ".git",
    "node_modules",
    "target",
    "Attachments",
    "90-Templates",
];

#[derive(Debug, Clone)]
pub struct ParsedNote {
    pub id: String,
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
    pub body: String,
    pub mtime: i64,
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

pub fn parse_file_at(path: &Path, root: &Path, mtime: i64) -> Option<ParsedNote> {
    let content = fs::read_to_string(path).ok()?;
    let content = content.trim_start_matches('\u{feff}').to_string();
    let (scalars, lists, body) = split_front_matter(&content);

    let rel_path = rel_path(path, root);

    let mut title = scalars.get("标题").cloned().unwrap_or_default();
    if title.is_empty() {
        title = first_h1(&body).unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名".to_string())
        });
    }

    let tags = lists.get("tags").cloned().unwrap_or_default();
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

    let run_id = path
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| name.starts_with("run_"))
        .unwrap_or_default();

    let body = strip_leading_h1(&body);

    Some(ParsedNote {
        id: note_id(&rel_path),
        path: display_path(path),
        rel_path,
        title: title.trim().to_string(),
        up: up.trim().to_string(),
        date: scalars.get("创建日期").cloned().unwrap_or_default().trim().to_string(),
        domain,
        tags,
        people: lists.get("人物").cloned().unwrap_or_default(),
        events: lists.get("事件").cloned().unwrap_or_default(),
        periods: lists.get("时期").cloned().unwrap_or_default(),
        chars: body.chars().count(),
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

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let first = value.chars().next().unwrap();
        let last = value.chars().last().unwrap();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return value[1..value.len() - 1].to_string();
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
