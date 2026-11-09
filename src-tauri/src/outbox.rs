//! Durable local delivery for ScribeFlow artifacts.
//!
//! ScribeFlow writes an immutable package under `.yebian/outbox/`. This module
//! validates it, serializes the note (front matter belongs to yebian), writes
//! the note atomically, records the artifact in `.yebian/artifacts/`, and marks
//! the package ACKed. A package that was never ACKed is retried on the next
//! drain; there is no direct writer into the user's note folders.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::artifact::{
    registry_for_note, ArtifactFile, ArtifactSource, StudioArtifact, OUTBOX_DIR, REGISTRY_DIR,
    STUDIO_FOLDER,
};
use crate::vault;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutboxReport {
    pub published: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

pub fn drain(root: &Path) -> OutboxReport {
    let mut report = OutboxReport::default();
    let outbox = root.join(OUTBOX_DIR);
    let Ok(artifacts) = fs::read_dir(&outbox) else {
        return report;
    };
    for artifact_entry in artifacts.filter_map(Result::ok) {
        if !artifact_entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(revisions) = fs::read_dir(artifact_entry.path()) else {
            continue;
        };
        for revision_entry in revisions.filter_map(Result::ok) {
            if !revision_entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                continue;
            }
            let package = revision_entry.path();
            if package.join("ACK").exists() {
                report.skipped += 1;
                continue;
            }
            if !package.join("READY").exists() {
                report.skipped += 1;
                continue;
            }
            match publish_package(root, &package) {
                Ok(Some(note)) => {
                    report.published += 1;
                    report.notes.push(note);
                }
                Ok(None) => report.skipped += 1,
                Err(error) => report.errors.push(format!("{}: {error}", package.display())),
            }
        }
    }
    report
}

fn publish_package(root: &Path, package: &Path) -> Result<Option<String>, String> {
    let manifest_path = package.join("artifact.json");
    let body_path = package.join("note.md");
    if !manifest_path.is_file() || !body_path.is_file() {
        return Err("package 缺少 artifact.json 或 note.md".into());
    }
    if fs::metadata(&manifest_path)
        .map(|meta| meta.len() > 2 * 1024 * 1024)
        .unwrap_or(true)
    {
        return Err("artifact.json 超过 2MB".into());
    }
    if fs::metadata(&body_path)
        .map(|meta| meta.len() > 20 * 1024 * 1024)
        .unwrap_or(true)
    {
        return Err("note.md 超过 20MB".into());
    }
    let text = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    let file: ArtifactFile = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    if !file.schema.starts_with("yebian.artifact") {
        return Err(format!("不认识的 artifact schema：{}", file.schema));
    }
    if file.note_id.trim().is_empty() {
        return Err("artifact 缺少 noteId".into());
    }
    if file.revision < 1 {
        return Err("artifact revision 必须 >= 1".into());
    }
    let body = fs::read_to_string(&body_path).map_err(|error| error.to_string())?;

    // A previous drain may have written the note but failed before ACK. If the
    // registry already has this revision and the note is still there, only ACK.
    if let Some(previous) = registry_for_note(root, &file.note_id) {
        if previous.revision >= file.revision && root.join(&previous.note_rel_path).is_file() {
            write_ack(package, &previous.note_rel_path, file.revision)?;
            return Ok(None);
        }
    }

    let target = target_path(root, &file)?;
    let rel = target
        .strip_prefix(root)
        .unwrap_or(&target)
        .to_string_lossy()
        .replace('\\', "/");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let markdown = render_note(&file, &body);
    if file.conflict_action == "overwrite" && target.is_file() {
        snapshot_note(root, &target, &file.note_id)?;
    }
    vault::write_atomic(&target, &markdown)?;

    let artifact = StudioArtifact {
        artifact_id: if file.artifact_id.is_empty() {
            file.note_id.clone()
        } else {
            file.artifact_id.clone()
        },
        note_id: file.note_id.clone(),
        note_rel_path: rel.clone(),
        title: title_of(&file),
        status: "published".into(),
        revision: file.revision,
        generated_at: file.generated_at.clone(),
        source: file.source.clone(),
        run: file.run.clone(),
        usage: file.usage.clone(),
        assets: file.assets.clone(),
        note_exists: true,
        annotation_count: 0,
        derived: false,
        error: None,
    };
    write_registry(root, &artifact)?;
    write_ack(package, &rel, file.revision)?;
    Ok(Some(rel))
}

pub(crate) fn snapshot_note(root: &Path, target: &Path, note_id: &str) -> Result<(), String> {
    let dir = root.join(".yebian").join("versions").join(safe_component(note_id));
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    fs::copy(target, dir.join(format!("manual-{stamp}.md"))).map_err(|error| error.to_string())?;
    Ok(())
}

fn target_path(root: &Path, file: &ArtifactFile) -> Result<PathBuf, String> {
    if let Some(previous) = registry_for_note(root, &file.note_id) {
        if !previous.note_rel_path.is_empty() {
            let path = root.join(&previous.note_rel_path);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    let folder_rel = if file.folder.trim().is_empty() {
        STUDIO_FOLDER.to_string()
    } else {
        file.folder.trim_matches('/').to_string()
    };
    let folder = root.join(&folder_rel);
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let stem = safe_file_stem(&title_of(file));
    let mut path = folder.join(format!("{stem}.md"));
    if !path.exists() {
        return Ok(path);
    }
    if vault::front_id_of(&fs::read_to_string(&path).unwrap_or_default()).as_deref() == Some(file.note_id.as_str()) {
        return Ok(path);
    }
    for suffix in 2..=99 {
        path = folder.join(format!("{stem} ({suffix}).md"));
        if !path.exists() {
            return Ok(path);
        }
        if vault::front_id_of(&fs::read_to_string(&path).unwrap_or_default()).as_deref() == Some(file.note_id.as_str()) {
            return Ok(path);
        }
    }
    Err(format!("同名笔记已排到「{stem} (99).md」，请先整理 {STUDIO_FOLDER}/"))
}

pub(crate) fn generated_note(root: &Path, note_id: &str, revision: i64) -> Result<String, String> {
    let package = root.join(OUTBOX_DIR).join(note_id).join(revision.to_string());
    let manifest = fs::read_to_string(package.join("artifact.json")).map_err(|error| error.to_string())?;
    let file: ArtifactFile = serde_json::from_str(&manifest).map_err(|error| error.to_string())?;
    let body = fs::read_to_string(package.join("note.md")).map_err(|error| error.to_string())?;
    Ok(render_note(&file, &body))
}

fn render_note(file: &ArtifactFile, raw_body: &str) -> String {
    let source = file.source.clone().unwrap_or_default();
    let title = title_of(file);
    let note_type = if file.note_type.is_empty() {
        match source.label.as_str() {
            "文本" => "学习笔记",
            "B站" | "本地文件" => "视频笔记",
            _ => "笔记",
        }
    } else {
        file.note_type.as_str()
    };
    let created = file
        .generated_at
        .get(..10)
        .filter(|value| value.len() == 10)
        .unwrap_or("");
    let mut lines = vec!["---".to_string(), format!("id: {}", yaml(&file.note_id)), format!("标题: {}", yaml(&title))];
    lines.push(format!("类型: {}", yaml(note_type)));
    if !source.label.is_empty() {
        lines.push(format!("来源: {}", yaml(&source.label)));
    }
    if !source.author.is_empty() {
        lines.push(format!("作者: {}", yaml(&source.author)));
    }
    if !source.url.is_empty() {
        lines.push(format!("原始链接: {}", yaml(&source.url)));
    }
    if !source.published_at.is_empty() {
        lines.push(format!("发布日期: {}", yaml(&source.published_at)));
    }
    if !created.is_empty() {
        lines.push(format!("创建日期: {}", yaml(created)));
    }
    if !file.domain.is_empty() {
        lines.push(format!("领域: {}", yaml(&file.domain)));
    }
    let tags: Vec<&String> = file
        .tags
        .iter()
        .filter(|tag| !tag.starts_with("来源/") && !tag.starts_with("UP主") && !tag.starts_with("UP："))
        .collect();
    if !tags.is_empty() {
        lines.push("tags:".into());
        for tag in tags {
            lines.push(format!("  - {}", yaml(tag)));
        }
    }
    let mut entities: Vec<_> = file.entities.iter().collect();
    entities.sort_by(|a, b| a.0.cmp(b.0));
    for (field, values) in entities {
        if values.is_empty() || field.trim().is_empty() {
            continue;
        }
        lines.push(format!("{}:", field.trim()));
        for value in values {
            lines.push(format!("  - {}", yaml(value)));
        }
    }
    lines.push("---".into());
    let body = strip_front_matter(raw_body).trim();
    let body = if body.starts_with("# ") {
        body.to_string()
    } else {
        format!("# {title}\n\n{body}")
    };
    format!("{}\n\n{}\n", lines.join("\n"), body)
}

fn write_registry(root: &Path, artifact: &StudioArtifact) -> Result<(), String> {
    let dir = root.join(REGISTRY_DIR);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let name = safe_component(if artifact.artifact_id.is_empty() {
        &artifact.note_id
    } else {
        &artifact.artifact_id
    });
    let path = dir.join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(artifact).map_err(|error| error.to_string())?;
    vault::write_atomic(&path, &text)
}

fn write_ack(package: &Path, note: &str, revision: i64) -> Result<(), String> {
    let payload = serde_json::json!({ "note": note, "revision": revision, "publishedAt": chrono::Utc::now().to_rfc3339() });
    fs::write(package.join("ACK"), serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn title_of(file: &ArtifactFile) -> String {
    if !file.title.trim().is_empty() {
        return file.title.trim().to_string();
    }
    let stem = file
        .note
        .rsplit('/')
        .next()
        .unwrap_or("未命名")
        .trim_end_matches(".md");
    if stem.is_empty() {
        "未命名".into()
    } else {
        stem.to_string()
    }
}

fn safe_file_stem(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        "笔记".into()
    } else {
        cleaned.chars().take(80).collect()
    }
}

fn safe_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' { ch } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "artifact".into()
    } else {
        cleaned
    }
}

fn yaml(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

pub(crate) fn body_only(markdown: &str) -> &str {
    strip_front_matter(markdown)
}

pub(crate) fn latest_generated(root: &Path, note_id: &str) -> Option<(i64, String)> {
    let dir = root.join(OUTBOX_DIR).join(note_id);
    let entries = fs::read_dir(dir).ok()?;
    let mut latest: Option<(i64, PathBuf)> = None;
    for entry in entries.flatten() {
        let Some(revision) = entry.file_name().to_str().and_then(|value| value.parse::<i64>().ok()) else { continue };
        if latest.as_ref().map(|(current, _)| revision > *current).unwrap_or(true) {
            latest = Some((revision, entry.path()));
        }
    }
    let (revision, path) = latest?;
    let body = fs::read_to_string(path.join("note.md")).ok()?;
    Some((revision, body))
}

fn strip_front_matter(markdown: &str) -> &str {
    let normalized = markdown.strip_prefix('\u{feff}').unwrap_or(markdown);
    if !normalized.starts_with("---\n") && !normalized.starts_with("---\r\n") {
        return normalized;
    }
    let rest = normalized.strip_prefix("---\n").or_else(|| normalized.strip_prefix("---\r\n")).unwrap_or(normalized);
    if let Some(end) = rest.find("\n---") {
        let after = &rest[end + 4..];
        after.trim_start_matches(['\r', '\n'])
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(root: &Path, note_id: &str, title: &str, body: &str, revision: i64) -> PathBuf {
        let dir = root.join(OUTBOX_DIR).join(note_id).join(revision.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("artifact.json"),
            serde_json::to_string(&serde_json::json!({
                "schema": "yebian.artifact.v2",
                "artifactId": note_id,
                "noteId": note_id,
                "revision": revision,
                "status": "ready",
                "title": title,
                "noteType": "学习笔记",
                "tags": ["话题/AI", "UP主/不该写"],
                "entities": {"人物": ["张三"]},
                "source": {"kind": "text", "label": "文本", "author": "测试"},
                "run": {"runId": "run_1", "projectName": "测试工程"},
                "generatedAt": "2026-11-05T10:00:00Z"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(dir.join("note.md"), body).unwrap();
        fs::write(dir.join("READY"), "").unwrap();
        dir
    }

    #[test]
    fn drains_ready_package_and_acks_only_once() {
        let root = std::env::temp_dir().join(format!("yebian-outbox-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&root).unwrap();
        let dir = package(&root, "note-1", "测试笔记", "正文", 1);
        let first = drain(&root);
        assert_eq!(first.published, 1, "{first:?}");
        assert!(dir.join("ACK").exists());
        let note = fs::read_to_string(root.join(STUDIO_FOLDER).join("测试笔记.md")).unwrap();
        assert!(note.contains("id: \"note-1\""));
        assert!(note.contains("来源: \"文本\""));
        assert!(!note.contains("UP主/不该写"));
        let registry = fs::read_to_string(root.join(REGISTRY_DIR).join("note-1.json")).unwrap();
        assert!(registry.contains("\"runId\": \"run_1\""));
        let second = drain(&root);
        assert_eq!(second.skipped, 1, "{second:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_latest_generated_revision_and_body_only() {
        let root = std::env::temp_dir().join(format!("yebian-outbox-latest-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&root).unwrap();
        package(&root, "note-latest", "最新", "第一版", 1);
        package(&root, "note-latest", "最新", "第二版", 3);
        let (revision, body) = latest_generated(&root, "note-latest").unwrap();
        assert_eq!(revision, 3);
        assert!(body.contains("第二版"));
        assert_eq!(body_only("---
id: x
---

正文
"), "正文
");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn renders_a_generated_revision_for_restore() {
        let root = std::env::temp_dir().join(format!("yebian-outbox-version-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&root).unwrap();
        package(&root, "note-version", "旧版", "旧版正文", 1);
        let note = generated_note(&root, "note-version", 1).unwrap();
        assert!(note.contains("id: \"note-version\""));
        assert!(note.contains("# 旧版"));
        assert!(note.contains("旧版正文"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn overwrite_publish_saves_a_manual_snapshot() {
        let root = std::env::temp_dir().join(format!("yebian-outbox-snapshot-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&root).unwrap();
        package(&root, "note-snap", "快照", "第一版", 1);
        drain(&root);
        let note_path = root.join(STUDIO_FOLDER).join("快照.md");
        fs::write(&note_path, "手改后的正文").unwrap();
        let dir = package(&root, "note-snap", "快照", "第二版", 2);
        let manifest = fs::read_to_string(dir.join("artifact.json")).unwrap();
        fs::write(dir.join("artifact.json"), manifest.replace("\"schema\"", "\"conflictAction\": \"overwrite\", \"schema\"")).unwrap();
        let report = drain(&root);
        assert_eq!(report.published, 1, "{report:?}");
        let versions = root.join(".yebian").join("versions").join("note-snap");
        let files = fs::read_dir(&versions).unwrap().flatten().collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        let snapshot = fs::read_to_string(files[0].path()).unwrap();
        assert!(snapshot.contains("手改后的正文"));
        assert!(fs::read_to_string(&note_path).unwrap().contains("第二版"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rerun_keeps_note_and_writes_revision_two() {
        let root = std::env::temp_dir().join(format!("yebian-outbox-rerun-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&root).unwrap();
        package(&root, "note-2", "重跑笔记", "第一版", 1);
        drain(&root);
        package(&root, "note-2", "重跑笔记", "第二版", 2);
        let report = drain(&root);
        assert_eq!(report.published, 1, "{report:?}");
        let note = fs::read_to_string(root.join(STUDIO_FOLDER).join("重跑笔记.md")).unwrap();
        assert!(note.contains("第二版"));
        assert!(note.contains("id: \"note-2\""));
        let _ = fs::remove_dir_all(root);
    }
}
