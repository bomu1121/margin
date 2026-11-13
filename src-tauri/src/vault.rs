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
}
