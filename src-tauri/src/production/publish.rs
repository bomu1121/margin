use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::Utc;

use crate::artifact::{
    ArtifactFile, ArtifactRun, ArtifactSource, ArtifactUsage, OUTBOX_DIR, REGISTRY_DIR,
};
use crate::production::store::ProductionStore;
use crate::production::{Job, SourceSpec};
use crate::vault;

pub struct PublishInput<'a> {
    pub root: &'a Path,
    pub job: &'a Job,
    pub body: String,
    pub tags: Vec<String>,
    pub entities: HashMap<String, Vec<String>>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub estimated_cost: f64,
    pub currency: String,
    pub model: String,
    pub prompt: String,
}

pub fn publish(_store: &ProductionStore, input: PublishInput<'_>) -> Result<String, String> {
    let body = ensure_body(&input.body, &input.job.title);
    let title = extract_h1(&body).unwrap_or_else(|| input.job.title.clone());
    let note_type = note_type(input.job);
    let source = source_meta(&input.job.source);
    let revision = next_revision(input.root, &input.job.note_id);
    let package = input
        .root
        .join(OUTBOX_DIR)
        .join(&input.job.note_id)
        .join(revision.to_string());
    let temp = package.with_extension("tmp");
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).map_err(|error| error.to_string())?;
    fs::write(temp.join("note.md"), body.as_bytes()).map_err(|error| error.to_string())?;
    let artifact = ArtifactFile {
        schema: "yebian.artifact.v2".into(),
        artifact_id: input.job.note_id.clone(),
        note_id: input.job.note_id.clone(),
        revision,
        status: "ready".into(),
        note: String::new(),
        note_rel_path: String::new(),
        title: title.clone(),
        note_type,
        domain: String::new(),
        tags: input.tags,
        entities: input.entities,
        source: Some(source),
        run: Some(ArtifactRun {
            project_id: "yebian-production".into(),
            project_name: input.job.template_id.clone(),
            run_id: input.job.id.clone(),
            node_id: "publish".into(),
            prompt: input.prompt,
            model: input.model,
        }),
        generated_at: Utc::now().to_rfc3339(),
        usage: Some(ArtifactUsage {
            prompt_tokens: input.prompt_tokens as i64,
            completion_tokens: input.completion_tokens as i64,
            estimated_cost: input.estimated_cost,
            currency: input.currency,
        }),
        assets: Vec::new(),
        error: None,
    };
    fs::write(
        temp.join("artifact.json"),
        serde_json::to_string_pretty(&artifact).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(temp.join("READY"), "").map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(&package);
    fs::rename(&temp, &package).map_err(|error| error.to_string())?;

    let report = crate::outbox::drain(input.root);
    if let Some(error) = report.errors.first() {
        return Err(format!("入库失败：{error}"));
    }
    let published = crate::artifact::registry_for_note(input.root, &input.job.note_id)
        .map(|item| item.note_rel_path)
        .filter(|path| !path.is_empty());
    published.ok_or_else(|| "入库完成但没有找到注册表记录".to_string())
}

pub fn related_notes(root: &Path, job: &Job, fields: &[String], values: &HashMap<String, Vec<String>>) -> Vec<String> {
    if fields.is_empty() || values.is_empty() {
        return Vec::new();
    }
    let mut matches: Vec<(usize, String)> = Vec::new();
    for path in vault::walk_notes(root) {
        let Some(note) = vault::parse_file_at(&path, root, vault::mtime_ms(&path)) else {
            continue;
        };
        if note.id == job.note_id || note.rel_path.starts_with("ScribeFlow/") {
            continue;
        }
        let mut score = 0usize;
        for field in fields {
            let wanted = values.get(field).cloned().unwrap_or_default();
            if wanted.is_empty() {
                continue;
            }
            let existing = match field.as_str() {
                "人物" => &note.people,
                "事件" => &note.events,
                "时期" => &note.periods,
                _ => continue,
            };
            score += existing.iter().filter(|value| wanted.iter().any(|item| item == *value)).count();
        }
        if score > 0 {
            matches.push((score, note.title));
        }
    }
    matches.sort_by(|a, b| b.0.cmp(&a.0));
    matches.into_iter().take(5).map(|(_, title)| title).collect()
}

fn next_revision(root: &Path, note_id: &str) -> i64 {
    let mut max = 0i64;
    if let Ok(entries) = fs::read_dir(root.join(OUTBOX_DIR).join(note_id)) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(value) = name.parse::<i64>() {
                    max = max.max(value);
                }
            }
        }
    }
    let registry = root.join(REGISTRY_DIR).join(format!("{note_id}.json"));
    if let Ok(raw) = fs::read_to_string(registry) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(revision) = value.get("revision").and_then(serde_json::Value::as_i64) {
                max = max.max(revision);
            }
        }
    }
    max + 1
}

fn ensure_body(body: &str, title: &str) -> String {
    let body = body.trim();
    if body.starts_with("# ") {
        body.to_string()
    } else {
        format!("# {title}\n\n{body}")
    }
}

fn extract_h1(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(|value| value.trim().to_string()))
        .filter(|value| !value.is_empty())
}

fn note_type(job: &Job) -> String {
    match job.template_id.as_str() {
        "text-note" => "学习笔记".into(),
        "gameguide" => "攻略笔记".into(),
        _ if job.source_kind == "text" => "学习笔记".into(),
        _ => "视频笔记".into(),
    }
}

fn source_meta(source: &SourceSpec) -> ArtifactSource {
    match source.kind.as_str() {
        "bili" => {
            let video = source.videos.first();
            ArtifactSource {
                kind: "bili".into(),
                label: "B站".into(),
                url: source.url.clone(),
                author: video.map(|item| item.author.clone()).unwrap_or_default(),
                published_at: video
                    .and_then(|item| {
                        if item.pubdate > 0 {
                            chrono::DateTime::from_timestamp(item.pubdate, 0)
                                .map(|date| date.format("%Y-%m-%d").to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
            }
        }
        "favorites" => ArtifactSource {
            kind: "bili".into(),
            label: "B站".into(),
            url: source.videos.first().map(|item| format!("https://www.bilibili.com/video/{}", item.bvid)).unwrap_or_default(),
            author: source.videos.first().map(|item| item.author.clone()).unwrap_or_default(),
            published_at: source
                .videos
                .first()
                .and_then(|item| {
                    if item.pubdate > 0 {
                        chrono::DateTime::from_timestamp(item.pubdate, 0).map(|date| date.format("%Y-%m-%d").to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        },
        "file" => ArtifactSource {
            kind: "file".into(),
            label: "本地文件".into(),
            url: String::new(),
            author: String::new(),
            published_at: String::new(),
        },
        _ => ArtifactSource {
            kind: "text".into(),
            label: "文本".into(),
            url: String::new(),
            author: String::new(),
            published_at: String::new(),
        },
    }
}


pub fn cleanup_temp(root: &Path, job_id: &str) {
    let _ = fs::remove_dir_all(root.join(".yebian").join("production-tmp").join(job_id));
}
