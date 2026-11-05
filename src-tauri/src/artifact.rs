//! Local ScribeFlow artifacts.
//!
//! ScribeFlow stays a replaceable producer: it writes Markdown into the vault
//! (plus an optional sidecar), and this module only reads that output. Nothing
//! here talks to ScribeFlow's database or canvas.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::db::NoteMeta;
use crate::vault;

// The registry makes provenance survive a move out of ScribeFlow/. See
// docs/SCRIBEFLOW.md and SF-005.
pub const STUDIO_FOLDER: &str = "ScribeFlow";
pub const SCRIBEFLOW_URL: &str = "http://127.0.0.1:8787";
pub const SCRIBEFLOW_UI_URL: &str = "http://127.0.0.1:5173";
pub const SIDECAR_SUFFIX: &str = ".artifact.json";
pub const YEBian_DIR: &str = ".yebian";
pub const OUTBOX_DIR: &str = ".yebian/outbox";
pub const REGISTRY_DIR: &str = ".yebian/artifacts";
const LEGACY_SIDECAR_SUFFIX: &str = ".yebian-artifact.json";
const SKIP_DIRS: &[&str] = &[".git", ".trash", ".yebian", "node_modules", "target", "Attachments", "90-Templates"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSource {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRun {
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactUsage {
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub estimated_cost: f64,
    #[serde(default)]
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactAsset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub bytes: u64,
}

/// The sidecar format. Every field is optional so a partially written or older
/// sidecar degrades to front-matter fields instead of hiding the note.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArtifactFile {
    #[serde(default)]
    pub schema: String,
    #[serde(default, alias = "artifact_id")]
    pub artifact_id: String,
    #[serde(default, alias = "note_id")]
    pub note_id: String,
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub note: String,
    #[serde(default, alias = "note_rel_path")]
    pub note_rel_path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub note_type: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub entities: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub source: Option<ArtifactSource>,
    #[serde(default)]
    pub run: Option<ArtifactRun>,
    #[serde(default, alias = "generated_at")]
    pub generated_at: String,
    #[serde(default)]
    pub usage: Option<ArtifactUsage>,
    #[serde(default)]
    pub assets: Vec<ArtifactAsset>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioArtifact {
    pub artifact_id: String,
    pub note_id: String,
    pub note_rel_path: String,
    pub title: String,
    pub status: String,
    pub revision: i64,
    pub generated_at: String,
    pub source: Option<ArtifactSource>,
    pub run: Option<ArtifactRun>,
    pub usage: Option<ArtifactUsage>,
    pub assets: Vec<ArtifactAsset>,
    pub note_exists: bool,
    pub annotation_count: i64,
    pub derived: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioInfo {
    pub folder: String,
    pub url: String,
    pub running: bool,
}

/// Read a note's artifact for the reading pane. Sidecars win over front matter.
pub fn for_note(root: &Path, note_path: &Path, note: &NoteMeta) -> Option<StudioArtifact> {
    let rel = relative(root, note_path);
    if let Some(mut artifact) = registry_for_note(root, &note.id) {
        artifact.note_rel_path = rel;
        artifact.note_exists = true;
        artifact.annotation_count = note.annotation_count;
        return Some(artifact);
    }
    let adjacent = sidecar_paths(note_path);
    for path in adjacent {
        if !path.exists() {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(file) = serde_json::from_str::<ArtifactFile>(&text) {
                return Some(build(root, Some(path), file, Some(rel), Some(note)));
            }
        }
    }
    let front = front_matter(note_path);
    if !is_artifact_note(&rel, &front) {
        return None;
    }
    Some(build(root, None, file_from_front(&front), Some(rel), Some(note)))
}

/// Scan the dedicated folder and every sidecar in the vault. Notes inside
/// `ScribeFlow/` are accepted even without a sidecar so existing ScribeFlow
/// output works without changing the producer.
pub fn scan(root: &Path, notes: &[NoteMeta]) -> Vec<StudioArtifact> {
    let by_rel: HashMap<&str, &NoteMeta> = notes.iter().map(|note| (note.rel_path.as_str(), note)).collect();
    let by_id: HashMap<&str, &NoteMeta> = notes.iter().map(|note| (note.id.as_str(), note)).collect();
    let mut out: Vec<StudioArtifact> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for mut artifact in read_registry(root) {
        if let Some(note) = by_rel.get(artifact.note_rel_path.as_str()).copied().or_else(|| by_id.get(artifact.note_id.as_str()).copied()) {
            artifact.annotation_count = note.annotation_count;
            artifact.note_exists = true;
        }
        merge(&mut out, &mut seen, artifact);
    }

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
        let file_name = entry.file_name().to_string_lossy();
        if !file_name.ends_with(SIDECAR_SUFFIX) && !file_name.ends_with(LEGACY_SIDECAR_SUFFIX) {
            continue;
        }
        let path = entry.path();
        if fs::metadata(path).map(|meta| meta.len() > 2 * 1024 * 1024).unwrap_or(true) {
            log::warn!("ScribeFlow sidecar too large, ignored: {}", path.display());
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<ArtifactFile>(&text) else {
            log::warn!("invalid ScribeFlow sidecar: {}", path.display());
            continue;
        };
        let note_path = resolve_sidecar_note(root, path, &file);
        let rel = note_path.as_deref().map(|path| relative(root, path)).unwrap_or_default();
        let note = by_rel.get(rel.as_str()).copied().or_else(|| by_id.get(file.note_id.as_str()).copied());
        let artifact = build(root, Some(path.to_path_buf()), file, Some(rel), note);
        merge(&mut out, &mut seen, artifact);
    }

    let folder = root.join(STUDIO_FOLDER);
    if folder.is_dir() {
        for entry in WalkDir::new(&folder)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file()
                || !entry
                    .path()
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                    .unwrap_or(false)
            {
                continue;
            }
            let path = entry.path();
            let rel = relative(root, path);
            if seen.contains_key(&rel) {
                continue;
            }
            let front = front_matter(path);
            let note = by_rel.get(rel.as_str()).copied();
            let artifact = build(root, None, file_from_front(&front), Some(rel), note);
            merge(&mut out, &mut seen, artifact);
        }
    }

    out.sort_by(|a, b| {
        b.generated_at
            .cmp(&a.generated_at)
            .then_with(|| b.note_rel_path.cmp(&a.note_rel_path))
    });
    out
}

pub fn studio_info() -> StudioInfo {
    StudioInfo {
        folder: STUDIO_FOLDER.to_string(),
        url: scribeflow_url(),
        running: port_open(8787),
    }
}

/// Prefer the Vite front end during local development; fall back to the
/// server's static bundle when ScribeFlow is running in production mode.
pub fn scribeflow_url() -> String {
    if port_open(5173) {
        SCRIBEFLOW_UI_URL.to_string()
    } else {
        SCRIBEFLOW_URL.to_string()
    }
}

fn port_open(port: u16) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let Ok(mut addrs) = format!("127.0.0.1:{port}").to_socket_addrs() else {
        return false;
    };
    let Some(socket) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&socket, Duration::from_millis(300)).is_ok()
}

pub(crate) fn read_registry(root: &Path) -> Vec<StudioArtifact> {
    let dir = root.join(REGISTRY_DIR);
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().map(|ext| ext != "json").unwrap_or(true) {
            continue;
        }
        if fs::metadata(&path).map(|meta| meta.len() > 2 * 1024 * 1024).unwrap_or(true) {
            log::warn!("artifact registry file too large, ignored: {}", path.display());
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        match serde_json::from_str::<StudioArtifact>(&text) {
            Ok(artifact) => out.push(artifact),
            Err(error) => log::warn!("invalid artifact registry {}: {error}", path.display()),
        }
    }
    out
}

pub(crate) fn registry_for_note(root: &Path, note_id: &str) -> Option<StudioArtifact> {
    read_registry(root)
        .into_iter()
        .filter(|artifact| artifact.note_id == note_id)
        .max_by_key(|artifact| artifact.revision)
}

fn merge(out: &mut Vec<StudioArtifact>, seen: &mut HashMap<String, usize>, artifact: StudioArtifact) {
    let key = artifact.note_rel_path.clone();
    if let Some(index) = seen.get(&key).copied() {
        if artifact.revision >= out[index].revision {
            out[index] = artifact;
        }
        return;
    }
    seen.insert(key, out.len());
    out.push(artifact);
}

fn build(
    root: &Path,
    sidecar: Option<PathBuf>,
    file: ArtifactFile,
    rel_hint: Option<String>,
    note: Option<&NoteMeta>,
) -> StudioArtifact {
    let note_path = rel_hint
        .as_deref()
        .filter(|rel| !rel.is_empty())
        .map(|rel| root.join(rel))
        .or_else(|| sidecar.as_deref().and_then(|path| resolve_sidecar_note(root, path, &file)));
    let note_rel_path = note_path
        .as_deref()
        .map(|path| relative(root, path))
        .or(rel_hint)
        .unwrap_or_default();
    let note_exists = note_path.as_deref().map(Path::exists).unwrap_or(false);
    let front = note_path.as_deref().map(front_matter).unwrap_or_default();
    let note_id = if !file.note_id.is_empty() {
        file.note_id
    } else if let Some(id) = front.get("id") {
        id.clone()
    } else if let Some(note) = note {
        note.id.clone()
    } else {
        vault::note_id(&note_rel_path)
    };
    let title = if !file.title.is_empty() {
        file.title
    } else if let Some(title) = front.get("标题") {
        title.clone()
    } else if let Some(note) = note {
        note.title.clone()
    } else {
        note_rel_path
            .rsplit('/')
            .next()
            .unwrap_or("未命名")
            .trim_end_matches(".md")
            .to_string()
    };
    let mut source = file.source.unwrap_or_default();
    fill(&mut source.kind, front.get("来源"));
    fill(&mut source.label, front.get("来源"));
    fill(&mut source.author, front.get("作者"));
    fill(&mut source.url, front.get("原始链接"));
    fill(&mut source.published_at, front.get("发布日期"));
    let mut run = file.run.unwrap_or_default();
    fill(&mut run.run_id, front.get("scribeflow_run"));
    if run.run_id.is_empty() {
        if let Some(parent) = note_path.as_deref().and_then(Path::parent) {
            if let Some(name) = parent.file_name().and_then(|name| name.to_str()) {
                if name.starts_with("run_") {
                    run.run_id = name.to_string();
                }
            }
        }
    }
    fill(&mut run.project_id, front.get("scribeflow_project"));
    fill(&mut run.project_name, front.get("scribeflow_project"));
    let generated_at = if !file.generated_at.is_empty() {
        file.generated_at
    } else if let Some(value) = front.get("scribeflow_generated") {
        value.clone()
    } else {
        front.get("创建日期").cloned().unwrap_or_default()
    };
    let status = if !note_exists && (file.status.is_empty() || file.status == "published") {
        "missing".into()
    } else if !file.status.is_empty() {
        file.status
    } else if note_exists {
        "published".into()
    } else {
        "missing".into()
    };
    let artifact_id = if !file.artifact_id.is_empty() {
        file.artifact_id
    } else {
        front
            .get("scribeflow_artifact")
            .cloned()
            .unwrap_or_else(|| note_id.clone())
    };
    StudioArtifact {
        artifact_id,
        note_id,
        note_rel_path,
        title,
        status,
        revision: file.revision.max(1),
        generated_at,
        source: if source.kind.is_empty() && source.url.is_empty() && source.author.is_empty() {
            None
        } else {
            Some(source)
        },
        run: if run.run_id.is_empty()
            && run.project_id.is_empty()
            && run.prompt.is_empty()
            && run.model.is_empty()
        {
            None
        } else {
            Some(run)
        },
        usage: file.usage,
        assets: file.assets,
        note_exists,
        annotation_count: note.map(|note| note.annotation_count).unwrap_or(0),
        derived: sidecar.is_none(),
        error: file.error,
    }
}

fn file_from_front(front: &HashMap<String, String>) -> ArtifactFile {
    let mut file = ArtifactFile::default();
    file.note_id = front.get("id").cloned().unwrap_or_default();
    file.artifact_id = front.get("scribeflow_artifact").cloned().unwrap_or_default();
    file.title = front.get("标题").cloned().unwrap_or_default();
    file.status = if front.contains_key("id") || front.contains_key("原始链接") {
        "published".into()
    } else {
        String::new()
    };
    file
}

fn is_artifact_note(rel: &str, front: &HashMap<String, String>) -> bool {
    rel.starts_with(&format!("{STUDIO_FOLDER}/")) || front.contains_key("scribeflow_artifact")
}

fn resolve_sidecar_note(root: &Path, sidecar: &Path, file: &ArtifactFile) -> Option<PathBuf> {
    let raw = if !file.note.is_empty() {
        file.note.clone()
    } else if !file.note_rel_path.is_empty() {
        file.note_rel_path.clone()
    } else {
        String::new()
    };
    if !raw.is_empty() {
        let candidate = Path::new(&raw);
        if candidate.is_absolute() {
            if candidate.starts_with(root) && (candidate.exists() || candidate.extension().is_some()) {
                return Some(candidate.to_path_buf());
            }
        } else {
            let lexical: Vec<&str> = raw.split(['/', '\\']).collect();
            if !lexical.iter().any(|part| *part == "..") {
                let path = root.join(candidate);
                if path.exists() || path.extension().is_some() {
                    return Some(path);
                }
            }
        }
    }
    let name = sidecar.file_name()?.to_string_lossy();
    for suffix in [SIDECAR_SUFFIX, LEGACY_SIDECAR_SUFFIX] {
        if let Some(stem) = name.strip_suffix(suffix) {
            let candidate = sidecar.with_file_name(stem);
            if candidate.extension().is_some() {
                return Some(candidate);
            }
        }
    }
    let parent = sidecar.parent()?;
    let mut markdown = fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .map(|ext| ext.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if markdown.len() == 1 {
        markdown.pop()
    } else {
        None
    }
}

fn sidecar_paths(note_path: &Path) -> Vec<PathBuf> {
    let parent = match note_path.parent() {
        Some(parent) => parent,
        None => return Vec::new(),
    };
    let name = match note_path.file_name() {
        Some(name) => name.to_string_lossy().to_string(),
        None => return Vec::new(),
    };
    vec![
        parent.join(format!("{name}{SIDECAR_SUFFIX}")),
        parent.join(format!("{name}{LEGACY_SIDECAR_SUFFIX}")),
    ]
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn fill(target: &mut String, value: Option<&String>) {
    if target.is_empty() {
        if let Some(value) = value {
            *target = value.clone();
        }
    }
}

fn front_matter(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(raw) = fs::read_to_string(path) else {
        return out;
    };
    let normalized = raw.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    if !normalized.starts_with("---\n") {
        return out;
    }
    let rest = &normalized[4..];
    let Some(end) = rest.find("\n---") else {
        return out;
    };
    for line in rest[..end].lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('-') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let value = if value.len() >= 2
                && ((value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\'')))
            {
                &value[1..value.len() - 1]
            } else {
                value
            };
            out.insert(key.trim().to_string(), value.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn scans_sidecar_and_derives_from_frontmatter() {
        let root = std::env::temp_dir().join(format!("yebian-artifact-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(root.join(STUDIO_FOLDER)).unwrap();
        let note = root.join(STUDIO_FOLDER).join("demo.md");
        fs::write(
            &note,
            "---\nid: n1\n标题: 演示\n来源: B站\n作者: UP\n原始链接: https://example.com\n创建日期: 2026-11-05\n---\n\n# 演示\n\n正文\n",
        )
        .unwrap();
        let sidecar = root.join(STUDIO_FOLDER).join("demo.md.artifact.json");
        fs::write(
            &sidecar,
            r#"{"schema":"yebian.artifact.v1","artifactId":"a1","noteId":"n1","revision":2,"status":"published","note":"ScribeFlow/demo.md","title":"演示","run":{"projectName":"演示工程","runId":"run_1"}}"#,
        )
        .unwrap();
        let notes = vec![NoteMeta {
            id: "n1".into(),
            path: note.to_string_lossy().into(),
            rel_path: "ScribeFlow/demo.md".into(),
            title: "演示".into(),
            up: "UP".into(),
            date: "2026-11-05".into(),
            domain: "未分类".into(),
            tags: vec![],
            people: vec![],
            events: vec![],
            periods: vec![],
            run_id: String::new(),
            mtime: 0,
            chars: 2,
            annotation_count: 3,
        }];
        let artifacts = scan(&root, &notes);
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_id, "a1");
        assert_eq!(artifacts[0].revision, 2);
        assert_eq!(artifacts[0].annotation_count, 3);
        assert!(artifacts[0].note_exists);
        assert!(!artifacts[0].derived);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn derives_from_frontmatter_and_ignores_ordinary_notes() {
        let root = std::env::temp_dir().join(format!("yebian-artifact-derived-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(root.join(STUDIO_FOLDER)).unwrap();
        fs::create_dir_all(root.join("研究")).unwrap();
        fs::write(
            root.join(STUDIO_FOLDER).join("derived.md"),
            "---
id: n2
标题: 派生
来源: B站
作者: UP
原始链接: https://example.com/v
创建日期: 2026-11-05
---

# 派生

正文
",
        )
        .unwrap();
        fs::write(
            root.join("研究").join("plain.md"),
            "---
标题: 普通
来源: B站
---

# 普通

正文
",
        )
        .unwrap();
        let artifacts = scan(&root, &[]);
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].note_id, "n2");
        assert_eq!(artifacts[0].title, "派生");
        assert!(artifacts[0].derived);
        assert!(artifacts[0].note_exists);
        assert_eq!(artifacts[0].source.as_ref().unwrap().label, "B站");
        let _ = fs::remove_dir_all(root);
    }
}
