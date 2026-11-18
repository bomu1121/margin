use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use super::checkpoint::{self, CostModel};
use super::clients::{self, AiConfig, AsrConfig, ChatResult, SearchConfig};
use super::media;
use super::prompts;
use super::publish::{self, PublishInput};
use super::store::ProductionStore;
use super::{
    BiliVideoRef, DrillItem, Job, JobConfig, JobOptions, JobResult, JobVariant, TraceItem, Usage,
};

struct Item {
    text: String,
    audio: Option<PathBuf>,
    title: String,
    author: String,
    url: String,
    pubdate: i64,
    duration: i64,
}

/// A talking-head video should produce at least ~1.5 Chinese characters per
/// second of audio. This is intentionally conservative: it catches an ASR
/// request that returns only the tail (the 2026-11-12 MiMo case: 414s audio,
/// 193 chars) while tolerating intros, music and pauses.
const MIN_ASR_CHARS_PER_SECOND: f64 = 1.5;

fn asr_coverage_issue(
    duration_sec: i64,
    part_count: usize,
    non_empty_parts: usize,
    chars: usize,
) -> Option<String> {
    if duration_sec < 60 || part_count == 0 {
        return None;
    }
    let coverage = chars as f64 / duration_sec as f64;
    if coverage < MIN_ASR_CHARS_PER_SECOND {
        return Some(format!(
            "转写覆盖不足：{duration_sec} 秒音频只得到 {chars} 字（{coverage:.2} 字/秒，{part_count} 段中 {non_empty_parts} 段有内容）；疑似 ASR 截断，已停止制作"
        ));
    }
    if part_count >= 3 && non_empty_parts * 2 < part_count {
        return Some(format!(
            "转写覆盖不足：{part_count} 段中只有 {non_empty_parts} 段有内容；疑似 ASR 漏段，已停止制作"
        ));
    }
    None
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedItem {
    text: String,
    audio: Option<String>,
    title: String,
    author: String,
    url: String,
    pubdate: i64,
    duration: i64,
}

impl From<&Item> for CachedItem {
    fn from(item: &Item) -> Self {
        Self {
            text: item.text.clone(),
            audio: item.audio.as_ref().map(|path| path.to_string_lossy().to_string()),
            title: item.title.clone(),
            author: item.author.clone(),
            url: item.url.clone(),
            pubdate: item.pubdate,
            duration: item.duration,
        }
    }
}

impl From<CachedItem> for Item {
    fn from(item: CachedItem) -> Self {
        Self {
            text: item.text,
            audio: item.audio.map(PathBuf::from),
            title: item.title,
            author: item.author,
            url: item.url,
            pubdate: item.pubdate,
            duration: item.duration,
        }
    }
}

fn cached_source_item(
    store: &ProductionStore,
    job_id: &str,
    key: &str,
    input_hash: &str,
) -> Result<Option<Item>, String> {
    let Some(row) = store.checkpoint(job_id, key)? else { return Ok(None) };
    if row.input_hash != input_hash {
        return Ok(None);
    }
    let cached: CachedItem = match serde_json::from_str(&row.payload_json) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if let Some(audio) = &cached.audio {
        if !Path::new(audio).is_file() {
            return Ok(None);
        }
    }
    Ok(Some(cached.into()))
}

fn save_source_item(store: &ProductionStore, job_id: &str, key: &str, input_hash: &str, item: &Item) -> Result<(), String> {
    let payload = serde_json::to_string(&CachedItem::from(item)).map_err(|error| error.to_string())?;
    store.save_checkpoint(job_id, key, "source", "source", input_hash, &payload, "{}", payload.len() as i64)
}

pub async fn run(
    app: &AppHandle,
    store: &ProductionStore,
    facts: &super::facts::FactStore,
    history: &super::history::HistoryStore,
    cache_dir: &Path,
    root: &Path,
    id: &str,
    token: &CancellationToken,
) -> Result<String, String> {
    let Some(mut job) = store.get_job(id)? else {
        return Err("任务不存在".into());
    };
    if job.status == "cancelled" {
        return Ok(String::new());
    }
    if job.steps.is_empty() {
        job.steps = store.ensure_steps(id, &job.template_id, &job.source)?;
        let _ = store.log(id, None, "info", "补建缺失的任务步骤（任务来自同步/旧记录）");
    }
    store.start_job(id)?;
    emit(app, store, id);

    let settings = store.settings();
    let mut config = store.job_config(id)?.unwrap_or_else(|| config_from_job(&job, &settings));
    // A job snapshot can contain another machine's ffmpeg path (this setting
    // used to be synced). Resolve on every run so a stale path cannot poison
    // retries; text-only jobs simply keep the unresolved value.
    if let Ok(path) = media::resolve_and_store(store) {
        config.ffmpeg_path = path;
    }
    let price = match config.price.clone() {
        Some(price) => Some(price),
        None => super::resolve_price(store, cache_dir, &config.provider, &config.model).await,
    };
    if config.price.is_none() && price.is_some() {
        config.price = price.clone();
    }
    store.set_job_config(id, &config)?;
    let ai = AiConfig {
        provider: config.provider.clone(),
        protocol: config.protocol.clone(),
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        api_key: store.ai_key(&config.provider),
        retries: config.retries.min(3),
    };
    let asr = AsrConfig {
        engine: config.asr_engine.clone(),
        ffmpeg_path: config.ffmpeg_path.clone(),
        base_url: config.asr_base_url.clone(),
        model: config.asr_model.clone(),
        api_key: store.get_setting("production.asr.api_key").ok().flatten().unwrap_or_default(),
    };
    let search_key = store
        .get_setting("production.search.api_key")
        .ok()
        .flatten()
        .unwrap_or_default();
    // DeepSeek Search 与 DeepSeek 模型使用同一个平台 Key；检索 Key 留空时直接复用，
    // 避免用户在两个输入框里重复粘贴同一把 Key。
    let search_key = if search_key.trim().is_empty() && config.search_provider == "deepseek" {
        store.ai_key("deepseek")
    } else {
        search_key
    };
    let search = SearchConfig {
        provider: config.search_provider.clone(),
        api_key: search_key,
    };
    let custom_prompts = store.list_prompts().unwrap_or_default();
    let cost = CostModel { price: price.clone(), asr_price_per_minute: config.asr_price_per_minute };
    let mut usage = Usage {
        currency: "CNY".into(),
        provider_currency: "USD".into(),
        priced: price.is_some(),
        ..Usage::default()
    };
    store.set_usage(id, &usage)?;
    let temp = media::job_temp_dir(root, id);
    tokio::fs::create_dir_all(&temp).await.map_err(|error| error.to_string())?;

    let source_step = find_step(&job, "source");
    if let Some(index) = source_step {
        mark(store, app, id, index, "running", "正在读取来源", token)?;
    }
    let mut items = resolve_source(store, &mut job, root, token).await?;
    if items.is_empty() {
        return Err("没有读到可处理的素材".into());
    }
    let _ = store.set_source(id, &job.source);
    let generic_title = matches!(job.title.as_str(), "B站视频笔记" | "粘贴文稿" | "本地音视频");
    if generic_title {
        if let Some(first) = items.first() {
            if !first.title.trim().is_empty() {
                let _ = store.set_title(id, &first.title);
                job.title = first.title.clone();
            }
        }
    }
    if let Some(index) = source_step {
        mark(store, app, id, index, "done", &format!("{} 个素材", items.len()), token)?;
    }
    let source_meta = json!({
        "kind": job.source.kind,
        "items": items.iter().map(|item| json!({
            "title": item.title,
            "author": item.author,
            "url": item.url,
            "duration": item.duration,
            "pubdate": item.pubdate,
            "textChars": item.text.chars().count(),
            "hasAudio": item.audio.is_some(),
        })).collect::<Vec<_>>(),
    });
    write_artifact(
        root,
        store,
        id,
        "source",
        "source.json",
        "json",
        serde_json::to_string_pretty(&source_meta).unwrap_or_default().as_bytes(),
        "{}",
    );

    let mut body = if job.template_id == "note-trace" { join_items(&items) } else { String::new() };

    if let Some(index) = find_step(&job, "transcribe") {
        if items.iter().any(|item| item.audio.is_some()) {
            mark(store, app, id, index, "running", "正在转写音频", token)?;
            let total_items = items.len();
            for (item_index, item) in items.iter_mut().enumerate() {
                if let Some(audio) = item.audio.clone() {
                    let prepared = media::prepare_audio(&audio, id, root, item_index, token, &config.asr_engine, &config.ffmpeg_path).await?;
                    let parts = media::prepare_asr_parts(&prepared, id, root, item_index, token, &config.asr_engine, &config.ffmpeg_path).await?;
                    let audio_hash = checkpoint::file_sha256(&prepared)?;
                    let mut texts = Vec::new();
                    for (part_index, part) in parts.iter().enumerate() {
                        let key = format!("asr:item:{item_index}:part:{part_index}");
                        let hash = checkpoint::hash_json(&json!({
                            "pipelineVersion": checkpoint::PIPELINE_VERSION,
                            "audio": audio_hash,
                            "part": part_index,
                            "engine": asr.engine,
                            "baseUrl": asr.base_url,
                            "model": asr.model,
                        }))?;
                        let text = checkpoint::memo(
                            store,
                            id,
                            "transcribe",
                            &key,
                            "asr",
                            &hash,
                            &mut usage,
                            &cost,
                            || async {
                                let result = if parts.len() > 1 {
                                    clients::transcribe_part(&asr, part, token).await?
                                } else {
                                    clients::transcribe_single(&asr, part, token).await?
                                };
                                Ok((result.text, Usage::default()))
                            },
                        )
                        .await?;
                        texts.push(text);
                        usage.audio_seconds += item.duration.max(0) as f64 / parts.len().max(1) as f64;
                        cost.apply(&mut usage);
                        store.set_usage(id, &usage)?;
                    }
                    item.text = texts.join("
");
                    let chars = item.text.chars().count();
                    let non_empty = texts.iter().filter(|text| !text.trim().is_empty()).count();
                    let coverage = chars as f64 / item.duration.max(1) as f64;
                    store.log(
                        id,
                        Some(index),
                        "info",
                        &format!(
                            "转写分段 {} 段，{} 段有内容，共 {} 字（{:.2} 字/秒）",
                            parts.len(),
                            non_empty,
                            chars,
                            coverage
                        ),
                    )?;
                    store.log(id, Some(index), "info", &format!("转写稿 {}（{} 字）：
{}", item_index + 1, chars, item.text))?;
                    if let Some(issue) = asr_coverage_issue(item.duration, parts.len(), non_empty, chars) {
                        let _ = store.log(id, Some(index), "warn", &issue);
                        return Err(issue);
                    }
                    write_artifact(
                        root,
                        store,
                        id,
                        "transcribe",
                        &format!("transcript-{:02}.md", item_index + 1),
                        "md",
                        item.text.as_bytes(),
                        &json!({ "chars": chars, "parts": parts.len(), "nonEmptyParts": non_empty }).to_string(),
                    );
                    let detail = format!("{}/{} · {} 字", item_index + 1, total_items, chars);
                    store.progress(id, progress_for_step(&job, index, item_index, total_items), &detail)?;
                    emit(app, store, id);
                }
            }
            mark(store, app, id, index, "done", &format!("{} 个音频", total_items), token)?;
        } else {
            mark(store, app, id, index, "done", "文本来源，跳过转写", token)?;
        }
    }

    if let Some(index) = find_step(&job, "refine") {
        mark(store, app, id, index, "running", "正在校对文稿", token)?;
        let total_items = items.len();
        for (item_index, item) in items.iter_mut().enumerate() {
            item.text = refine_text(store, id, item_index, &item.text, &ai, &mut usage, &cost, token).await?;
            store.log(id, Some(index), "info", &format!("校对稿 {}（{} 字）：
{}", item_index + 1, item.text.chars().count(), item.text))?;
            write_artifact(
                root,
                store,
                id,
                "refine",
                &format!("refined-{:02}.md", item_index + 1),
                "md",
                item.text.as_bytes(),
                &json!({ "chars": item.text.chars().count() }).to_string(),
            );
            store.progress(id, progress_for_step(&job, index, item_index, total_items), "校对完成")?;
            emit(app, store, id);
        }
        mark(store, app, id, index, "done", "校对完成", token)?;
    }

    let template_id = job.template_id.clone();
    let options = job.options.clone();
    let title = job.title.clone();
    let mut variants: Vec<JobVariant> = Vec::new();
    let mut mindmap: Option<String> = None;
    let mut drill: Vec<DrillItem> = Vec::new();
    let mut trace: Vec<TraceItem> = Vec::new();

    if template_id == "booklet" {
        let material = join_items(&items);
        let material_cjk = super::booklet::cjk_len(&material);
        let (chapter_count, chapter_chars) = super::booklet::budget(material_cjk);
        store.log(
            id,
            None,
            "info",
            &format!("预算：素材 {} 字 → {} 章 × 约 {} 字", material_cjk, chapter_count, chapter_chars),
        )?;
        let mut plan: Option<super::booklet::BookPlan> = None;
        if let Some(index) = find_step(&job, "outline") {
            mark(store, app, id, index, "running", "正在规划章节", token)?;
            let user = format!("素材：\n{material}");
            let result = checkpoint::cached_chat(
                store,
                id,
                "outline",
                "plan",
                &super::booklet::plan_system(chapter_count, chapter_chars),
                &user,
                &ai,
                &mut usage,
                &cost,
                token,
            )
            .await?;
            let parsed = super::booklet::parse_plan(&result.content)?;
            let figures = parsed.chapters.iter().filter(|chapter| chapter.figure.is_some()).count();
            store.log(
                id,
                Some(index),
                "info",
                &format!("大纲：{} 章、{} 张图", parsed.chapters.len(), figures),
            )?;
            store.log(id, Some(index), "info", &format!("大纲 JSON：\n{}", result.content))?;
            plan = Some(parsed);
            mark(store, app, id, index, "done", "大纲完成", token)?;
        }
        let plan = plan.ok_or_else(|| "没有生成大纲".to_string())?;
        let mut chapters: Vec<String> = Vec::new();
        if let Some(index) = find_step(&job, "chapter") {
            mark(store, app, id, index, "running", "正在分章写作", token)?;
            let total = plan.chapters.len();
            for (chapter_index, chapter) in plan.chapters.iter().enumerate() {
                let user = super::booklet::chapter_user(chapter, &material);
                let result = checkpoint::cached_chat(
                    store,
                    id,
                    "chapter",
                    &format!("chapter:{chapter_index}"),
                    &super::booklet::chapter_system(chapter_chars),
                    &user,
                    &ai,
                    &mut usage,
                    &cost,
                    token,
                )
                .await?;
                store.log(
                    id,
                    Some(index),
                    "info",
                    &format!(
                        "章节 {}/{}「{}」{} 字",
                        chapter_index + 1,
                        total,
                        chapter.title,
                        super::booklet::cjk_len(&result.content)
                    ),
                )?;
                let mut content = result.content;
                let original = super::booklet::cjk_len(&content);
                if original > 1500 {
                    store.log(
                        id,
                        Some(index),
                        "info",
                        &format!("章节 {} 有 {} 字，超过预算，压缩一次", chapter_index + 1, original),
                    )?;
                    let compressed = checkpoint::cached_chat(
                        store,
                        id,
                        "chapter",
                        &format!("compress:{chapter_index}"),
                        super::booklet::compress_system(),
                        &content,
                        &ai,
                        &mut usage,
                        &cost,
                        token,
                    )
                    .await?;
                    let after = super::booklet::cjk_len(&compressed.content);
                    if after > 0 && after < original {
                        store.log(
                            id,
                            Some(index),
                            "info",
                            &format!("章节 {} 压缩：{} → {} 字", chapter_index + 1, original, after),
                        )?;
                        content = compressed.content;
                    }
                }
                write_artifact(
                    root,
                    store,
                    id,
                    "chapter",
                    &format!("chapter-{:02}.md", chapter_index + 1),
                    "md",
                    content.as_bytes(),
                    &json!({ "title": chapter.title, "chars": super::booklet::cjk_len(&content), "beforeCompress": original }).to_string(),
                );
                chapters.push(content);
                store.progress(id, progress_for_step(&job, index, chapter_index, total), &format!("{}/{}", chapter_index + 1, total))?;
                emit(app, store, id);
            }
            mark(store, app, id, index, "done", &format!("{total} 章"), token)?;
        }
        if let Some(index) = find_step(&job, "assemble") {
            mark(store, app, id, index, "running", "正在组装", token)?;
            let source_note = "内容来自本期素材，章节、图示与「一页看懂」是二次整理；数字仅按素材口径引用，未独立复现。";
            body = super::booklet::assemble(&plan, &chapters, source_note);
            write_artifact(
                root,
                store,
                id,
                "assemble",
                "booklet.md",
                "md",
                body.as_bytes(),
                &json!({ "chapters": plan.chapters.len() }).to_string(),
            );
            mark(store, app, id, index, "done", "组装完成", token)?;
        }
    } else if template_id == "html-booklet" {
        let material = join_items(&items);
        let material_cjk = super::booklet::cjk_len(&material);
        let (chapter_count, chapter_chars) = super::booklet::budget(material_cjk);
        store.log(
            id,
            None,
            "info",
            &format!("HTML 作者版预算：素材 {} 字 → {} 章 × 约 {} 字", material_cjk, chapter_count, chapter_chars),
        )?;

        if let Some(index) = find_step(&job, "design") {
            mark(store, app, id, index, "running", "套用纸/墨/朱砂版式", token)?;
            write_artifact(
                root,
                store,
                id,
                "design",
                "design.json",
                "json",
                super::htmlbook::DESIGN.as_bytes(),
                &json!({ "preset": "paper-vermilion" }).to_string(),
            );
            store.log(id, Some(index), "info", "版式：paper-vermilion（纸/墨/朱砂，宋体 + 黑体）")?;
            mark(store, app, id, index, "done", "paper-vermilion", token)?;
        }

        let mut plan: Option<super::booklet::BookPlan> = None;
        let mut plan_raw = String::new();
        if let Some(index) = find_step(&job, "plan") {
            mark(store, app, id, index, "running", "正在规划章节", token)?;
            let result = checkpoint::cached_chat(
                store,
                id,
                "plan",
                "plan",
                &super::booklet::plan_system(chapter_count, chapter_chars),
                &format!("素材：\n{material}"),
                &ai,
                &mut usage,
                &cost,
                token,
            )
            .await?;
            let parsed = super::booklet::parse_plan(&result.content)?;
            plan_raw = result.content;
            let figures = parsed.chapters.iter().filter(|chapter| chapter.figure.is_some()).count();
            store.log(
                id,
                Some(index),
                "info",
                &format!("计划：{} 章、{} 个计划图", parsed.chapters.len(), figures),
            )?;
            write_artifact(
                root,
                store,
                id,
                "plan",
                "plan.json",
                "json",
                plan_raw.as_bytes(),
                &json!({ "chapters": parsed.chapters.len(), "figures": figures }).to_string(),
            );
            plan = Some(parsed);
            mark(store, app, id, index, "done", "计划完成", token)?;
        }
        let plan = plan.ok_or_else(|| "没有生成章节计划".to_string())?;

        let paths = super::htmlbook::prepare(root, id, &material, &plan_raw, &plan)?;
        let mut html = std::fs::read_to_string(&paths.book).map_err(|error| format!("HTML 骨架打不开：{error}"))?;
        if let Some(index) = find_step(&job, "author") {
            mark(store, app, id, index, "running", "正在写 HTML 章节", token)?;
            let total = plan.chapters.len();
            for (chapter_index, chapter) in plan.chapters.iter().enumerate() {
                let user = super::htmlbook::chapter_user(&plan, chapter, &material, chapter_index);
                let system = super::htmlbook::author_system(chapter_chars);
                let result = checkpoint::cached_chat(
                    store,
                    id,
                    "author",
                    &format!("chapter:{chapter_index}"),
                    &system,
                    &user,
                    &ai,
                    &mut usage,
                    &cost,
                    token,
                )
                .await?;
                let fragment = super::htmlbook::clean_fragment(&result.content)?;
                html = super::htmlbook::insert_chapter(&html, chapter_index, &fragment)?;
                write_artifact(
                    root,
                    store,
                    id,
                    "author",
                    &format!("chapter-{:02}.html", chapter_index + 1),
                    "html",
                    fragment.as_bytes(),
                    &json!({ "chapter": chapter_index + 1, "title": chapter.title, "chars": super::booklet::cjk_len(&fragment) }).to_string(),
                );
                store.log(
                    id,
                    Some(index),
                    "info",
                    &format!(
                        "章节 {}/{}「{}」{} 字 HTML",
                        chapter_index + 1,
                        total,
                        chapter.title,
                        super::booklet::cjk_len(&fragment)
                    ),
                )?;
                store.progress(id, 60 + ((chapter_index + 1) * 20 / total.max(1)) as i64, &format!("{}/{}", chapter_index + 1, total))?;
                emit(app, store, id);
            }
            write_artifact(
                root,
                store,
                id,
                "author",
                "book.html",
                "html",
                html.as_bytes(),
                &json!({ "chapters": total, "chars": super::booklet::cjk_len(&html) }).to_string(),
            );
            if let Err(error) = super::htmlbook::validate(&html) {
                mark(store, app, id, index, "error", &error, token)?;
                return Err(error);
            }
            store.log(
                id,
                Some(index),
                "info",
                &format!("HTML 作者完成：{} 章、{} 字", total, super::booklet::cjk_len(&html)),
            )?;
            mark(store, app, id, index, "done", &format!("{} 章 · {} 字", total, super::booklet::cjk_len(&html)), token)?;
        } else {
            return Err("HTML 作者版缺少 author 步骤".into());
        }
        let mut html = html;

        let mut render_result: Option<super::render::RenderResult> = None;
        if let Some(index) = find_step(&job, "render") {
            mark(store, app, id, index, "running", "正在渲染并测量", token)?;
            let mut final_html = html.clone();
            let mut dir = root.join(".yebian").join("runs").join(id).join("render");
            let mut result = super::render::render_book(&final_html, &dir, "book").await?;
            let mut issues = super::htmlbook::layout_issues(&result.metrics);
            let mut compact = 0usize;
            while !issues.is_empty() && compact < 2 && issues.iter().all(|item| item.starts_with("页溢出")) {
                compact += 1;
                final_html = super::htmlbook::with_css(&html, super::htmlbook::compact_css(compact), compact);
                dir = root.join(".yebian").join("runs").join(id).join(format!("render-fix-{compact}"));
                result = super::render::render_book(&final_html, &dir, "book").await?;
                issues = super::htmlbook::layout_issues(&result.metrics);
                store.log(id, Some(index), "info", &format!("页溢出，应用紧凑 CSS L{compact}"))?;
            }
            html = final_html;
            write_artifact(
                root,
                store,
                id,
                "render",
                "book.html",
                "html",
                html.as_bytes(),
                &json!({ "pages": result.pages, "compact": compact }).to_string(),
            );
            let metrics_text = serde_json::to_string_pretty(&result.metrics).unwrap_or_else(|_| "{}".into());
            write_artifact(
                root,
                store,
                id,
                "render",
                "metrics.json",
                "json",
                metrics_text.as_bytes(),
                "{}",
            );
            if !issues.is_empty() {
                let detail = issues.join("；");
                mark(store, app, id, index, "error", &detail, token)?;
                return Err(format!("HTML 版面未清零：{detail}"));
            }
            store.log(
                id,
                Some(index),
                "info",
                &format!(
                    "渲染：{} 页、{} 图、{} 节{}",
                    result.pages,
                    result.metrics.get("figures").and_then(Value::as_i64).unwrap_or(0),
                    result.metrics.get("sections").and_then(Value::as_i64).unwrap_or(0),
                    if compact > 0 { format!("，紧凑 L{compact}") } else { String::new() }
                ),
            )?;
            mark(store, app, id, index, "done", &format!("{} 页{}", result.pages, if compact > 0 { format!(" · 紧凑 L{compact}") } else { String::new() }), token)?;
            render_result = Some(result);
        }
        let render = render_result.ok_or_else(|| "没有渲染结果".to_string())?;

        if let Some(index) = find_step(&job, "print") {
            mark(store, app, id, index, "running", "正在登记 PDF", token)?;
            let bytes = std::fs::read(&render.pdf_path).map_err(|error| format!("PDF 打不开：{error}"))?;
            write_artifact(
                root,
                store,
                id,
                "print",
                "book.pdf",
                "pdf",
                &bytes,
                &json!({ "pages": render.pages, "bytes": bytes.len() }).to_string(),
            );
            store.log(id, Some(index), "info", &format!("PDF：{} 页、{:.1} MB", render.pages, bytes.len() as f64 / 1024.0 / 1024.0))?;
            mark(store, app, id, index, "done", &format!("{} 页 · {:.1} MB", render.pages, bytes.len() as f64 / 1024.0 / 1024.0), token)?;
        }

        let mut page_paths: Vec<String> = Vec::new();
        if let Some(index) = find_step(&job, "pagecheck") {
            mark(store, app, id, index, "running", "正在生成页级切片", token)?;
            let offsets = super::htmlbook::page_offsets(&render.metrics);
            if offsets.is_empty() {
                mark(store, app, id, index, "error", "HTML 里没有 .page 分页", token)?;
                return Err("HTML 里没有 .page 分页，无法做页级检查".into());
            }
            let dir = root.join(".yebian").join("runs").join(id).join("pagecheck");
            page_paths = super::render::render_slices(&html, &dir, "page", &offsets).await?;
            for (index, path) in page_paths.iter().enumerate() {
                if let Ok(bytes) = std::fs::read(path) {
                    write_artifact(
                        root,
                        store,
                        id,
                        "pagecheck",
                        &format!("page-{:02}.png", index + 1),
                        "png",
                        &bytes,
                        &json!({ "page": index + 1 }).to_string(),
                    );
                }
            }
            let manifest = json!({ "pages": page_paths.len(), "offsets": offsets }).to_string();
            write_artifact(root, store, id, "pagecheck", "pages.json", "json", manifest.as_bytes(), "{}");
            store.log(id, Some(index), "info", &format!("页级切片：{} 页", page_paths.len()))?;
            mark(store, app, id, index, "done", &format!("{} 页", page_paths.len()), token)?;
        }

        if let Some(index) = find_step(&job, "judge") {
            mark(store, app, id, index, "running", "正在逐页视觉审校", token)?;
            match super::htmlbook::judge_pages(&ai, &page_paths, token).await {
                Ok((verdicts, prompt, completion, reasoning)) => {
                    usage.prompt_tokens += prompt;
                    usage.completion_tokens += completion;
                    usage.reasoning_tokens += reasoning;
                    usage.uncached_input_tokens += prompt;
                    cost.apply(&mut usage);
                    store.set_usage(id, &usage)?;
                    let report = serde_json::to_string_pretty(&json!({
                        "pages": verdicts.len(),
                        "verdicts": verdicts,
                    }))
                    .unwrap_or_else(|_| "{}".into());
                    write_artifact(
                        root,
                        store,
                        id,
                        "judge",
                        "judge.json",
                        "json",
                        report.as_bytes(),
                        &json!({ "pages": verdicts.len() }).to_string(),
                    );
                    store.log(id, Some(index), "info", &format!("视觉审校：{} 页全通过", verdicts.len()))?;
                    mark(store, app, id, index, "done", &format!("{} 页通过", verdicts.len()), token)?;
                }
                Err(error) => {
                    mark(store, app, id, index, "error", &error, token)?;
                    return Err(error);
                }
            }
        }

        body = super::htmlbook::projection(&plan, render.pages.max(page_paths.len() as i64));
    } else if template_id == "gameguide" {
        if let Some(index) = find_step(&job, "gameguide") {
            mark(store, app, id, index, "running", "正在核对攻略", token)?;
            let text = join_items(&items);
            let result = cached_generate(store, id, "gameguide", "gameguide", &ai, prompts::gameguide_system(), &title, &text, &mut usage, &cost, token).await?;
            body = result.content;
            mark(store, app, id, index, "done", "攻略完成", token)?;
        }
    } else if template_id == "mindmap" {
        if let Some(index) = find_step(&job, "mindmap") {
            mark(store, app, id, index, "running", "正在整理导图", token)?;
            let text = join_items(&items);
            let result = cached_generate(store, id, "mindmap", "mindmap", &ai, &prompts::mindmap_system(options.mind_depth.max(2)), &title, &text, &mut usage, &cost, token).await?;
            let outline = strip_leading_h1(&result.content);
            mindmap = Some(outline.clone());
            body = format!("# {title}

{outline}");
            mark(store, app, id, index, "done", "导图完成", token)?;
        }
    } else if template_id == "research-note" {
        // One-shot author with a per-note plan: the structure stays fixed, but
        // the sections, their length budget and the figures come from the plan.
        let text = join_items(&items);
        let material = clip_material(&text);
        let mut plan: Option<super::research_note::NotePlan> = None;
        if let Some(index) = find_step(&job, "plan") {
            mark(store, app, id, index, "running", "正在规划章节", token)?;
            let result = checkpoint::cached_chat(
                store,
                id,
                "plan",
                "research-note",
                prompts::research_note_plan_system(),
                &format!("标题：{title}\n\n素材：\n{material}"),
                &ai,
                &mut usage,
                &cost,
                token,
            )
            .await?;
            match super::research_note::parse_plan(&result.content) {
                Ok(parsed) => {
                    write_artifact(
                        root,
                        store,
                        id,
                        "plan",
                        "plan.json",
                        "json",
                        result.content.as_bytes(),
                        &json!({ "sections": parsed.sections.len(), "totalChars": parsed.total_chars }).to_string(),
                    );
                    store.log(
                        id,
                        Some(index),
                        "info",
                        &format!("章节计划：{} 节 / 约 {} 字", parsed.sections.len(), parsed.total_chars),
                    )?;
                    mark(store, app, id, index, "done", &format!("{} 节", parsed.sections.len()), token)?;
                    plan = Some(parsed);
                }
                Err(error) => {
                    store.log(id, Some(index), "info", &format!("章节计划解析失败，按固定结构成稿：{error}"))?;
                    mark(store, app, id, index, "done", "未采用计划", token)?;
                }
            }
        }
        if let Some(index) = find_step(&job, "prompt") {
            mark(store, app, id, index, "running", "正在按计划成稿", token)?;
            let plan_text = plan
                .as_ref()
                .map(|item| serde_json::to_string_pretty(item).unwrap_or_default())
                .unwrap_or_default();
            let user = if plan_text.trim().is_empty() {
                format!("标题：{title}\n\n素材：\n{material}")
            } else {
                format!("标题：{title}\n\n## 本篇计划\n{plan_text}\n\n素材：\n{material}")
            };
            let system = prompts::research_note_system();
            let first = checkpoint::cached_chat(
                store,
                id,
                "prompt",
                "draft:1",
                system,
                &user,
                &ai,
                &mut usage,
                &cost,
                token,
            )
            .await?;
            let mut written = first.content;
            let mut issues = super::research_note::validate(&written, plan.as_ref());
            if !issues.is_empty() {
                store.log(
                    id,
                    Some(index),
                    "info",
                    &format!("结构校验未通过（{} 项）：{}", issues.len(), issues.join("；")),
                )?;
                let retry_user = format!("{user}{}", prompts::research_note_repair_hint(&issues));
                let second = checkpoint::cached_chat(
                    store,
                    id,
                    "prompt",
                    "repair:1",
                    system,
                    &retry_user,
                    &ai,
                    &mut usage,
                    &cost,
                    token,
                )
                .await?;
                written = second.content;
                issues = super::research_note::validate(&written, plan.as_ref());
            }
            if !issues.is_empty() {
                let message = format!("结构校验失败：{}", issues.join("；"));
                mark(store, app, id, index, "error", &message, token)?;
                return Err(message);
            }
            let chars = super::booklet::cjk_len(&written);
            write_artifact(
                root,
                store,
                id,
                "prompt",
                "note.md",
                "md",
                written.as_bytes(),
                &json!({ "chars": chars }).to_string(),
            );
            body = written;
            mark(store, app, id, index, "done", &format!("{chars} 字"), token)?;
        }
    } else {
        if let Some(index) = find_step(&job, "prompt") {
            mark(store, app, id, index, "running", "正在生成笔记", token)?;
            let styles = styles_for(&template_id, &options, &config.default_prompt);
            let mut item_notes: Vec<String> = Vec::new();
            for (item_index, item) in items.iter().enumerate() {
                let note = generate_note(store, id, item_index, &ai, &custom_prompts, &styles, item, &title, &mut usage, &cost, token).await?;
                for variant in &note.0 {
                    store.log(id, Some(index), "info", &format!("{} 原文（{} 字）：
{}", variant.name, variant.body.chars().count(), variant.body))?;
                }
                variants.extend(note.0);
                item_notes.push(note.1);
                store.progress(id, progress_for_step(&job, index, item_index, items.len()), "笔记生成中")?;
                emit(app, store, id);
            }
            body = if item_notes.len() > 1 {
                merge_notes(store, id, "prompt:items:merge", &ai, &title, &item_notes, &mut usage, &cost, token).await?
            } else {
                item_notes.into_iter().next().unwrap_or_default()
            };
            if template_id == "multi-style" || template_id == "multi-branch" {
                if let Some(second) = variants.first().map(|item| item.body.clone()) {
                    if !body.contains(&second) {
                        body.push_str("\n\n## 另一版\n\n");
                        body.push_str(&second);
                    }
                }
            }
            mark(store, app, id, index, "done", "笔记完成", token)?;
        }

        if let Some(index) = find_step(&job, "chapter") {
            mark(store, app, id, index, "running", "正在切分章节", token)?;
            let text = join_items(&items);
            let result = cached_generate(store, id, "chapter", "chapter", &ai, &prompts::chapter_system(&options.chapter, options.max_chapters.max(2)), &title, &text, &mut usage, &cost, token).await?;
            body = result.content;
            mark(store, app, id, index, "done", "章节完成", token)?;
        }
        if let Some(index) = find_step(&job, "merge") {
            if items.len() > 1 {
                mark(store, app, id, index, "running", "正在合并多份产物", token)?;
                body = merge_notes(store, id, "merge", &ai, &title, &[body], &mut usage, &cost, token).await?;
                mark(store, app, id, index, "done", "合并完成", token)?;
            } else {
                mark(store, app, id, index, "done", "单份素材，无需合并", token)?;
            }
        }
        if let Some(index) = find_step(&job, "drill") {
            mark(store, app, id, index, "running", "正在出题", token)?;
            let result = cached_generate(
                store,
                id,
                "drill",
                "drill",
                &ai,
                &prompts::drill_system(options.drill_count.max(3)),
                &title,
                &body,
                &mut usage,
                &cost,
                token,
            )
            .await?;
            drill = parse_drill(&result.content);
            if drill.is_empty() {
                store.log(id, Some(index), "info", "练习 JSON 解析为空，保留笔记正文")?;
            } else {
                body.push_str("\n\n## 练一练\n\n");
                for item in &drill {
                    body.push_str(&format!("### {}\n\n", item.question));
                    for option in &item.options {
                        body.push_str(&format!("- {option}\n"));
                    }
                    body.push_str(&format!("\n答案：{}\n\n", item.answer));
                }
            }
            mark(store, app, id, index, "done", "练习完成", token)?;
        }
        if let Some(index) = find_step(&job, "trace") {
            if search.api_key.trim().is_empty() {
                return Err("已经选择溯源模板，但还没有配置联网检索 Key".into());
            }
            mark(store, app, id, index, "running", "正在联网核查", token)?;
            let trace_items = super::trace::run(
                store,
                facts,
                history,
                &job.note_id,
                id,
                index,
                &ai,
                &search,
                &title,
                &body,
                options.trace_claims,
                options.trace_max,
                &mut usage,
                &cost,
                token,
            )
            .await?;
            trace = trace_items;
            // 事实与来源已写入事实库；新笔记不再把整份溯源报告追加到 Markdown 末尾。
            // 旧笔记里已经存在的“## 溯源”文本保留，避免重写用户文件。
            mark(store, app, id, index, "done", "溯源完成", token)?;
        }
    }

    if job.template_id == "note-trace" {
        cost.apply(&mut usage);
        if let Some(index) = find_step(&job, "publish") {
            mark(store, app, id, index, "done", "事实已更新，笔记正文未改写", token)?;
        }
        let result_body = if trace.is_empty() {
            "# 溯源完成

没有提取到可外部核查的主张。".to_string()
        } else {
            format!("# 溯源完成

本次处理 {} 条主张，事实与来源已写入事实库；笔记正文未改写。", trace.len())
        };
        let result = JobResult {
            body: result_body,
            summary: format!("{} 条主张 · 只更新事实库", trace.len()),
            variants: Vec::new(),
            mindmap: None,
            drill: Vec::new(),
            trace,
            related: Vec::new(),
            note_path: job.note_rel_path.clone(),
        };
        store.finish(id, &result, &usage, &job.note_rel_path)?;
        let _ = store.clear_checkpoints(id);
        publish::cleanup_temp(root, id);
        emit(app, store, id);
        return Ok(job.note_rel_path);
    }

    if token.is_cancelled() {
        return Err("已取消".into());
    }
    if body.trim().is_empty() && find_step(&job, "agent_note").is_none() {
        return Err("模型没有返回笔记正文".into());
    }

    // Optional webview step. The Rust pipeline cannot build the booklet HTML,
    // so it queues the assembled draft and waits for the in-app worker. The
    // note-author template has no other generator, so it queues the material
    // and the writing spec; the worker's agent run is the whole processing.
    let agent_step = if find_step(&job, "agent").is_some() {
        Some(("agent", "layout", "{}".to_string()))
    } else {
        find_step(&job, "agent_note").map(|_| {
            (
                "agent_note",
                "note-author",
                json!({
                    "material": clip_material(&join_items(&items)),
                    "title": title,
                    "spec": prompts::agent_note_brief(),
                    "specFree": prompts::agent_note_free_brief(),
                    "variant": options.prompt,
                })
                .to_string(),
            )
        })
    };
    if let Some((step_key, mode, payload)) = agent_step {
        let index = find_step(&job, step_key).ok_or_else(|| "缺少智能体步骤".to_string())?;
        let action = if mode == "layout" { "智能体修订" } else { "智能体成稿" };
        let draft = if mode == "layout" { body.clone() } else { String::new() };
        let task_id = store.open_agent_task(id, index, mode, &payload, &draft)?;
        mark(store, app, id, index, "running", &format!("等待{action}（窗口需打开）"), token)?;
        store.progress(id, 90, &format!("等待{action}"))?;
        store.log(id, Some(index), "info", "已交给窗口内的智能体；运行期间请保持应用打开")?;
        emit(app, store, id);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1800);
        loop {
            if token.is_cancelled() {
                let _ = store.fail_agent_task(task_id, "任务已取消", None);
                return Err("已取消".into());
            }
            if let Some((status, result, run_json, error)) = store.agent_task_state(task_id)? {
                match status.as_str() {
                    "done" => {
                        if result.trim().is_empty() {
                            return Err("智能体没有返回草稿".into());
                        }
                        body = result;
                        if mode == "note-author" {
                            // The webview gate already checked the draft; this is
                            // the Rust-side sanity check against a false success.
                            // Structure and length are the author's call, so only
                            // a title and some substance are required here.
                            let chars = super::booklet::cjk_len(&body);
                            if !body.trim_start().starts_with("# ") || chars < 200 {
                                return Err(format!("智能体成稿没有通过基本检查：{chars} 字"));
                            }
                        }
                        let run: Option<Value> = serde_json::from_str(&run_json).ok();
                        let prompt = run.as_ref().and_then(|value| value.pointer("/usage/promptTokens")).and_then(Value::as_u64).unwrap_or(0);
                        let completion = run.as_ref().and_then(|value| value.pointer("/usage/completionTokens")).and_then(Value::as_u64).unwrap_or(0);
                        let reasoning = run.as_ref().and_then(|value| value.pointer("/usage/reasoningTokens")).and_then(Value::as_u64).unwrap_or(0);
                        let rounds = run.as_ref().and_then(|value| value.pointer("/turns")).and_then(Value::as_array).map(Vec::len).unwrap_or(0);
                        usage.prompt_tokens += prompt;
                        usage.completion_tokens += completion;
                        usage.reasoning_tokens += reasoning;
                        // The webview reports no cache split, so price the whole
                        // prompt as uncached (a conservative estimate).
                        usage.uncached_input_tokens += prompt;
                        cost.apply(&mut usage);
                        store.set_usage(id, &usage)?;
                        write_artifact(
                            root,
                            store,
                            id,
                            step_key,
                            if mode == "layout" { "agent.md" } else { "note.md" },
                            "md",
                            body.as_bytes(),
                            &json!({
                                "rounds": rounds,
                                "promptTokens": prompt,
                                "completionTokens": completion,
                            })
                            .to_string(),
                        );
                        write_artifact(
                            root,
                            store,
                            id,
                            step_key,
                            "agent-run.json",
                            "json",
                            run_json.as_bytes(),
                            "{}",
                        );
                        store.log(
                            id,
                            Some(index),
                            "info",
                            &format!("{action}完成：{rounds} 回合，{}/{} tokens", prompt, completion),
                        )?;
                        mark(store, app, id, index, "done", &format!("{rounds} 回合 · {}/{} tokens", prompt, completion), token)?;
                        break;
                    }
                    "failed" => {
                        // A failed run still spent tokens and produced a partial
                        // trajectory; keep both so the ledger and the run page
                        // do not show a free, invisible failure.
                        let run: Option<Value> = serde_json::from_str(&run_json).ok();
                        let prompt = run.as_ref().and_then(|value| value.pointer("/usage/promptTokens")).and_then(Value::as_u64).unwrap_or(0);
                        let completion = run.as_ref().and_then(|value| value.pointer("/usage/completionTokens")).and_then(Value::as_u64).unwrap_or(0);
                        let reasoning = run.as_ref().and_then(|value| value.pointer("/usage/reasoningTokens")).and_then(Value::as_u64).unwrap_or(0);
                        if prompt + completion + reasoning > 0 {
                            usage.prompt_tokens += prompt;
                            usage.completion_tokens += completion;
                            usage.reasoning_tokens += reasoning;
                            usage.uncached_input_tokens += prompt;
                            cost.apply(&mut usage);
                            store.set_usage(id, &usage)?;
                        }
                        if !run_json.trim().is_empty() {
                            write_artifact(
                                root,
                                store,
                                id,
                                step_key,
                                "agent-run.json",
                                "json",
                                run_json.as_bytes(),
                                &json!({ "failed": true }).to_string(),
                            );
                            if let Some(draft) = run.as_ref().and_then(|value| value.get("draft")).and_then(Value::as_str) {
                                if !draft.trim().is_empty() {
                                    write_artifact(
                                        root,
                                        store,
                                        id,
                                        step_key,
                                        if mode == "layout" { "agent.md" } else { "note.md" },
                                        "md",
                                        draft.as_bytes(),
                                        &json!({ "failed": true }).to_string(),
                                    );
                                }
                            }
                        }
                        return Err(format!(
                            "{action}失败：{}",
                            if error.trim().is_empty() { "见任务日志" } else { error.trim() }
                        ))
                    }
                    _ => {}
                }
            }
            if std::time::Instant::now() >= deadline {
                if let Some((status, _, _, _)) = store.agent_task_state(task_id)? {
                    if status == "pending" || status == "running" {
                        let _ = store.fail_agent_task(task_id, "等待超时", None);
                    }
                }
                return Err(format!("等待{action}超时（30 分钟）；请保持窗口打开后重试"));
            }
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
    }

    if body.trim().is_empty() {
        return Err("智能体没有返回笔记正文".into());
    }
    if generic_title {
        if let Some(title) = body.lines().find_map(|line| line.strip_prefix("# ")).map(str::trim).filter(|value| !value.is_empty()) {
            let _ = store.set_title(id, title);
            job.title = title.to_string();
        }
    }

    let mut entities = HashMap::new();
    if !options.fields.is_empty() {
        entities = extract_entities(store, id, &ai, &body, &options.fields, &mut usage, &cost, token).await.unwrap_or_else(|error| {
            let _ = store.log(id, None, "info", &format!("属性抽取失败：{error}"));
            HashMap::new()
        });
    }
    let related = publish::related_notes(root, &job, &options.fields, &entities);
    if !related.is_empty() {
        body.push_str("\n\n## 相关笔记\n\n");
        for title in &related {
            body.push_str(&format!("- [[{}]]\n", title));
        }
    }

    // Resolve the destination before cost accounting: auto classification is
    // one extra model call and must count toward the task's tokens and cost.
    let mut destination_folder = String::new();
    if options.destination == "auto" {
        let classified = super::classify::classify(&ai, root, &job.title, &body, token).await;
        usage.prompt_tokens += classified.prompt_tokens;
        usage.completion_tokens += classified.completion_tokens;
        destination_folder = classified.folder.clone();
        let _ = store.log(
            id,
            None,
            "info",
            &format!("自动归类：{}（{}）", classified.folder, classified.reason),
        );
    } else if options.destination == "folder" {
        destination_folder = options.folder.clone();
        if !destination_folder.trim().is_empty() {
            let _ = store.log(id, None, "info", &format!("入库位置：{destination_folder}"));
        }
    }

    if token.is_cancelled() {
        return Err("已取消".into());
    }
    let publish_index = find_step(&job, "publish");
    if let Some(index) = publish_index {
        mark(store, app, id, index, "running", "正在入库", token)?;
    }
    cost.apply(&mut usage);
    let summary = format!("{} 字 · {} tokens", body.chars().count(), usage.prompt_tokens + usage.completion_tokens);
    let mut result = JobResult {
        body: body.clone(),
        summary,
        variants,
        mindmap,
        drill,
        trace,
        related,
        note_path: String::new(),
    };
    let prompt_name = if matches!(job.template_id.as_str(), "research-note" | "agent-note") {
        "技术调研".to_string()
    } else {
        options.prompt.clone()
    };
    let note_rel = publish::publish(
        store,
        PublishInput {
            root,
            job: &job,
            body: body.clone(),
            tags: options.tags.clone(),
            entities,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            estimated_cost: usage.estimated_cost,
            currency: usage.currency.clone(),
            model: config.model.clone(),
            prompt: prompt_name,
            folder: destination_folder,
        },
    )?;
    result.note_path = note_rel.clone();
    if let Some(index) = publish_index {
        mark(store, app, id, index, "done", &note_rel, token)?;
    }
    if let Ok(bytes) = std::fs::read(root.join(&note_rel)) {
        write_artifact(
            root,
            store,
            id,
            "publish",
            "note.md",
            "md",
            &bytes,
            &json!({ "notePath": note_rel }).to_string(),
        );
    }
    store.finish(id, &result, &usage, &note_rel)?;
    if let Err(error) = store.clear_checkpoints(id) {
        let _ = store.log(id, None, "info", &format!("清理检查点失败：{error}"));
    }
    publish::cleanup_temp(root, id);
    emit(app, store, id);
    Ok(note_rel)
}

fn subscription_author(store: &ProductionStore, job: &Job) -> Option<String> {
    if job.options.subscription_id.trim().is_empty() {
        return None;
    }
    store
        .get_subscription(&job.options.subscription_id)
        .ok()
        .flatten()
        .map(|sub| sub.name.trim().to_string())
        .filter(|name| !name.is_empty())
}

async fn resolve_source(
    store: &ProductionStore,
    job: &mut Job,
    root: &Path,
    token: &CancellationToken,
) -> Result<Vec<Item>, String> {
    let temp = media::job_temp_dir(root, &job.id);
    tokio::fs::create_dir_all(&temp).await.map_err(|error| error.to_string())?;
    if job.source.kind == "bili" {
        // Rebuild from the source list on resume; the persisted source already
        // contains the videos from the previous attempt.
        job.source.videos.clear();
    }
    match job.source.kind.as_str() {
        "text" => Ok(vec![Item {
            text: job.source.text.clone(),
            audio: None,
            title: job.title.clone(),
            author: String::new(),
            url: String::new(),
            pubdate: 0,
            duration: 0,
        }]),
        "file" => {
            let path = PathBuf::from(&job.source.path);
            if !path.is_file() {
                return Err(format!("文件不存在：{}", path.display()));
            }
            let title = path
                .file_stem()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| job.title.clone());
            Ok(vec![Item {
                text: String::new(),
                audio: Some(path),
                title,
                author: String::new(),
                url: String::new(),
                pubdate: 0,
                duration: 0,
            }])
        }
        "bili" => {
            let preview = clients::fetch_bili_preview(&job.source.url).await?;
            // A joint submission may list another collaborator as the video
            // owner. Automatic jobs must keep the UP the user subscribed to;
            // otherwise the note's 作者/UP tag points at the wrong person.
            let subscription_author = subscription_author(store, job);
            let pages = if job.source.pages.is_empty() {
                preview.pages.first().cloned().into_iter().collect::<Vec<_>>()
            } else {
                preview
                    .pages
                    .iter()
                    .filter(|page| job.source.pages.contains(&(page.page as u32)))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if pages.is_empty() {
                return Err("这个视频没有可用的分P".into());
            }
            let mut items = Vec::new();
            for (index, page) in pages.iter().enumerate() {
                if token.is_cancelled() {
                    return Err("已取消".into());
                }
                let key = format!("source:item:{index}");
                let input_hash = checkpoint::hash_json(&json!({
                    "pipelineVersion": checkpoint::PIPELINE_VERSION,
                    "kind": "bili",
                    "url": job.source.url,
                    "bvid": preview.bvid,
                    "cid": page.cid,
                    "index": index,
                }))?;
                let mut item = if let Some(item) = cached_source_item(store, &job.id, &key, &input_hash)? {
                    item
                } else {
                    let audio_url = clients::bili_audio_url(&preview.bvid, page.cid).await?;
                    let audio = temp.join(format!("bili-{index}.m4s"));
                    clients::download_bili_audio(&audio_url, &audio, token).await?;
                    let page_title = if preview.pages.len() > 1 {
                        format!("{} · {}", preview.title, page.part)
                    } else {
                        preview.title.clone()
                    };
                    let item = Item {
                        text: String::new(),
                        audio: Some(audio),
                        title: page_title,
                        author: subscription_author.clone().unwrap_or_else(|| preview.author.clone()),
                        url: format!("https://www.bilibili.com/video/{}?p={}", preview.bvid, page.page),
                        pubdate: preview.pubdate,
                        duration: page.duration,
                    };
                    save_source_item(store, &job.id, &key, &input_hash, &item)?;
                    item
                };
                if let Some(author) = &subscription_author {
                    item.author = author.clone();
                }
                job.source.videos.push(BiliVideoRef {
                    bvid: preview.bvid.clone(),
                    title: item.title.clone(),
                    author: item.author.clone(),
                    duration: item.duration,
                    pubdate: item.pubdate,
                    cover: preview.cover.clone(),
                    cid: page.cid,
                });
                items.push(item);
            }
            Ok(items)
        }
        "favorites" => {
            let mut items = Vec::new();
            for (index, video) in job.source.videos.clone().iter().enumerate() {
                if token.is_cancelled() {
                    return Err("已取消".into());
                }
                let preview = clients::fetch_bili_preview(&format!("https://www.bilibili.com/video/{}", video.bvid)).await?;
                let page = preview.pages.first().ok_or_else(|| format!("{} 没有分P", video.bvid))?;
                let key = format!("source:item:{index}");
                let input_hash = checkpoint::hash_json(&json!({
                    "pipelineVersion": checkpoint::PIPELINE_VERSION,
                    "kind": "favorites",
                    "bvid": preview.bvid,
                    "cid": page.cid,
                    "index": index,
                }))?;
                let item = if let Some(item) = cached_source_item(store, &job.id, &key, &input_hash)? {
                    item
                } else {
                    let audio_url = clients::bili_audio_url(&preview.bvid, page.cid).await?;
                    let audio = temp.join(format!("favorite-{index}.m4s"));
                    clients::download_bili_audio(&audio_url, &audio, token).await?;
                    let item = Item {
                        text: String::new(),
                        audio: Some(audio),
                        title: preview.title.clone(),
                        author: preview.author.clone(),
                        url: format!("https://www.bilibili.com/video/{}", preview.bvid),
                        pubdate: preview.pubdate,
                        duration: page.duration,
                    };
                    save_source_item(store, &job.id, &key, &input_hash, &item)?;
                    item
                };
                if let Some(target) = job.source.videos.get_mut(index) {
                    target.title = item.title.clone();
                    target.author = item.author.clone();
                    target.duration = item.duration;
                    target.pubdate = item.pubdate;
                    target.cover = preview.cover.clone();
                    target.cid = page.cid;
                }
                items.push(item);
            }
            Ok(items)
        }
        other => Err(format!("不支持的来源：{other}")),
    }
}

async fn refine_text(
    store: &ProductionStore,
    job_id: &str,
    item_index: usize,
    text: &str,
    ai: &AiConfig,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<String, String> {
    let chunks = split_text(text, 6000);
    let mut output = Vec::new();
    for (part_index, chunk) in chunks.iter().enumerate() {
        let key = format!("refine:item:{item_index}:part:{part_index}");
        let result = checkpoint::cached_chat(
            store,
            job_id,
            "refine",
            &key,
            prompts::refine_system(),
            chunk,
            ai,
            usage,
            cost,
            token,
        )
        .await?;
        output.push(result.content.trim().to_string());
    }
    Ok(output.join("\n\n"))
}

async fn generate_note(
    store: &ProductionStore,
    job_id: &str,
    item_index: usize,
    ai: &AiConfig,
    custom: &[prompts::PromptEntry],
    styles: &[String],
    item: &Item,
    title: &str,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<(Vec<JobVariant>, String), String> {
    let chunks = split_text(&item.text, 12000);
    let mut variants = Vec::new();
    let mut primary = String::new();
    for (style_index, style) in styles.iter().enumerate() {
        let system = prompts::prompt_system(style, custom);
        let mut parts = Vec::new();
        for (part_index, chunk) in chunks.iter().enumerate() {
            let key = format!("prompt:item:{item_index}:style:{style_index}:part:{part_index}");
            let user = format!("标题：{title}\n\n素材：\n{chunk}");
            let result = checkpoint::cached_chat(
                store,
                job_id,
                "prompt",
                &key,
                &system,
                &user,
                ai,
                usage,
                cost,
                token,
            )
            .await?;
            parts.push(result.content);
        }
        let mut body = if parts.len() > 1 {
            let key = format!("prompt:item:{item_index}:style:{style_index}:merge");
            merge_notes(store, job_id, &key, ai, title, &parts, usage, cost, token).await?
        } else {
            parts.into_iter().next().unwrap_or_default()
        };
        if style == "速览版" {
            let section = add_xianyin(store, job_id, item_index, style_index, ai, item, &body, usage, cost, token).await?;
            body = insert_before_section(&body, "## 速查", &section);
        }
        if style_index == 0 {
            primary = body.clone();
        }
        variants.push(JobVariant {
            name: style.clone(),
            summary: format!("{} 字", body.chars().count()),
            body,
        });
    }
    Ok((variants, primary))
}

async fn add_xianyin(
    store: &ProductionStore,
    job_id: &str,
    item_index: usize,
    style_index: usize,
    ai: &AiConfig,
    item: &Item,
    body: &str,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<String, String> {
    let system = format!("{}

原文全文：
{}", prompts::xianyin_system(), item.text);
    let key = format!("prompt:item:{item_index}:style:{style_index}:xianyin");
    let result = checkpoint::cached_chat(store, job_id, "prompt", &key, &system, body, ai, usage, cost, token).await?;
    let content = result.content.trim();
    Ok(if content.starts_with("## 弦外之音") {
        content.to_string()
    } else {
        format!("## 弦外之音
{}", content)
    })
}

fn insert_before_section(body: &str, marker: &str, section: &str) -> String {
    if let Some(index) = body.find(marker) {
        format!(
            "{}

{}

{}",
            body[..index].trim_end(),
            section.trim(),
            &body[index..]
        )
    } else {
        format!("{}

{}", body.trim_end(), section.trim())
    }
}

async fn cached_generate(
    store: &ProductionStore,
    job_id: &str,
    step_key: &str,
    unit_key: &str,
    ai: &AiConfig,
    system: &str,
    title: &str,
    text: &str,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<ChatResult, String> {
    let user = format!("标题：{title}

素材：
{text}");
    checkpoint::cached_chat(store, job_id, step_key, unit_key, system, &user, ai, usage, cost, token).await
}

async fn merge_notes(
    store: &ProductionStore,
    job_id: &str,
    key: &str,
    ai: &AiConfig,
    title: &str,
    notes: &[String],
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<String, String> {
    if notes.len() <= 1 {
        return Ok(notes.first().cloned().unwrap_or_default());
    }
    let user = format!("标题：{title}

以下是同一主题的多份笔记：

{}", notes.join("

---

"));
    let result = checkpoint::cached_chat(store, job_id, "prompt", key, prompts::merge_system(), &user, ai, usage, cost, token).await?;
    Ok(result.content)
}

fn styles_for(template_id: &str, options: &JobOptions, default_prompt: &str) -> Vec<String> {
    if template_id == "multi-style" || template_id == "multi-branch" {
        if options.styles.len() >= 2 {
            return options.styles.clone();
        }
        return vec!["成文版".into(), "速览版".into()];
    }
    let primary = if options.prompt.trim().is_empty() {
        if default_prompt.trim().is_empty() { "成文版".to_string() } else { default_prompt.to_string() }
    } else {
        options.prompt.clone()
    };
    vec![primary]
}

async fn extract_entities(
    store: &ProductionStore,
    job_id: &str,
    ai: &AiConfig,
    body: &str,
    fields: &[String],
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<HashMap<String, Vec<String>>, String> {
    let text = body.chars().take(16000).collect::<String>();
    let result = checkpoint::cached_chat(
        store,
        job_id,
        "extract",
        "extract",
        &prompts::extract_system(fields),
        &text,
        ai,
        usage,
        cost,
        token,
    )
    .await?;
    let raw = clients::strip_fence(&result.content);
    let value: Value = serde_json::from_str(&raw).map_err(|error| format!("抽取 JSON 无法解析：{error}"))?;
    let mut entities = HashMap::new();
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            let values = value
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            if !values.is_empty() {
                entities.insert(key.clone(), values);
            }
        }
    }
    Ok(entities)
}

fn parse_drill(content: &str) -> Vec<DrillItem> {
    let raw = clients::strip_fence(content);
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let items = value
        .as_array()
        .cloned()
        .or_else(|| value.get("items").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let question = item.get("question").and_then(Value::as_str)?.to_string();
            let options = item
                .get("options")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            let answer = item
                .get("answer")
                .map(|value| {
                    if let Some(text) = value.as_str() {
                        text.to_string()
                    } else if let Some(items) = value.as_array() {
                        items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("、")
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default();
            Some(DrillItem {
                id: item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("q{}", index + 1)),
                question,
                kind: item.get("kind").and_then(Value::as_str).unwrap_or("single").to_string(),
                options,
                answer,
                note: item.get("note").and_then(Value::as_str).unwrap_or_default().to_string(),
            })
        })
        .collect()
}


fn strip_leading_h1(value: &str) -> String {
    let trimmed = value.trim_start();
    if let Some(rest) = trimmed.strip_prefix("# ") {
        if let Some(line_end) = rest.find('\n') {
            return rest[line_end + 1..].trim_start().to_string();
        }
    }
    trimmed.to_string()
}

fn find_step(job: &Job, key: &str) -> Option<usize> {
    job.steps.iter().position(|step| step.key == key)
}

fn mark(
    store: &ProductionStore,
    app: &AppHandle,
    id: &str,
    index: usize,
    status: &str,
    detail: &str,
    token: &CancellationToken,
) -> Result<(), String> {
    if token.is_cancelled() {
        return Err("已取消".into());
    }
    store.set_step(id, index, status, detail)?;
    // Mirror the legacy step onto the run graph so the UI can show node
    // timings, token deltas and per-node artifacts.
    if let Ok(Some(job)) = store.get_job(id) {
        if let Some(step) = job.steps.get(index) {
            match status {
                "running" => {
                    let _ = store.node_start(id, &step.key, &step.label);
                }
                "done" | "skipped" | "error" => {
                    let error = if status == "error" { detail } else { "" };
                    let summary = if status == "error" { "" } else { detail };
                    let _ = store.node_finish(id, &step.key, status, summary, error);
                }
                _ => {}
            }
        }
    }
    emit(app, store, id);
    Ok(())
}

/// Persist one step output under `.yebian/runs/<job>/<node>/` and register it
/// in the run store. Artifacts survive successful runs on purpose: the point is
/// to inspect them afterwards.
fn write_artifact(
    root: &Path,
    store: &ProductionStore,
    job_id: &str,
    node: &str,
    name: &str,
    kind: &str,
    content: &[u8],
    meta: &str,
) {
    let dir = root.join(".yebian").join("runs").join(job_id).join(node);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(name);
    if std::fs::write(&path, content).is_err() {
        return;
    }
    let mut hasher = Sha256::new();
    hasher.update(content);
    let sha = format!("{:x}", hasher.finalize());
    let _ = store.add_artifact(&super::RunArtifact {
        id: 0,
        job_id: job_id.to_string(),
        node_key: node.to_string(),
        kind: kind.to_string(),
        name: name.to_string(),
        path: path.to_string_lossy().into_owned(),
        bytes: content.len() as i64,
        sha256: sha,
        meta: meta.to_string(),
        created_at: super::store::now_ms(),
    });
}

fn emit(app: &AppHandle, store: &ProductionStore, id: &str) {
    if let Ok(Some(job)) = store.get_job(id) {
        let _ = app.emit("production-update", &job);
    }
}

fn progress_for_step(job: &Job, index: usize, item: usize, total: usize) -> i64 {
    let base = if job.steps.is_empty() { 0 } else { index as i64 * 100 / job.steps.len() as i64 };
    let span = if job.steps.is_empty() { 100 } else { 100 / job.steps.len() as i64 };
    base + if total == 0 { 0 } else { span * item as i64 / total as i64 }
}

fn join_items(items: &[Item]) -> String {
    items
        .iter()
        .map(|item| item.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// The research-note template writes the whole note in one pass, so the
/// material has to fit one prompt. 48k chars covers a 20–40 minute video while
/// keeping a runaway source from silently blowing the context.
fn clip_material(text: &str) -> String {
    const LIMIT: usize = 48_000;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let clipped: String = text.chars().take(LIMIT).collect();
    format!("{clipped}\n\n（素材超过 {LIMIT} 字，已截断；以上为前 {LIMIT} 字。）")
}

fn split_text(text: &str, max_chars: usize) -> Vec<String> {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if !current.is_empty() && current.chars().count() + paragraph.chars().count() + 2 > max_chars {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    if chunks.is_empty() {
        vec![text.to_string()]
    } else {
        chunks
    }
}

fn config_from_job(job: &Job, settings: &super::ProductionSettings) -> JobConfig {
    JobConfig {
        pipeline_version: checkpoint::PIPELINE_VERSION,
        provider: job.provider.clone(),
        protocol: settings.ai_protocol.clone(),
        base_url: settings.ai_base_url.clone(),
        model: job.model.clone(),
        asr_engine: settings.asr_engine.clone(),
        asr_base_url: settings.asr_base_url.clone(),
        asr_model: settings.asr_model.clone(),
        asr_price_per_minute: settings.asr_price_per_minute,
        ffmpeg_path: settings.ffmpeg_path.clone(),
        search_provider: settings.search_provider.clone(),
        default_prompt: settings.default_prompt.clone(),
        exchange_rate: settings.ai_exchange_rate,
        retries: settings.retries,
        price: None,
    }
}

fn estimate_cost(usage: &Usage, price: Option<&super::providers::PriceQuote>, settings: &super::ProductionSettings) -> f64 {
    let mut cost = 0.0;
    if let Some(price) = price {
        let cache_read = if price.cache_read_priced { price.cache_read_cny } else { price.input_cny };
        let cache_write = if price.cache_write_priced { price.cache_write_cny } else { price.input_cny };
        cost += usage.uncached_input_tokens as f64 / 1_000_000.0 * price.input_cny;
        cost += usage.cached_tokens as f64 / 1_000_000.0 * cache_read;
        cost += usage.cache_write_tokens as f64 / 1_000_000.0 * cache_write;
        cost += usage.completion_tokens as f64 / 1_000_000.0 * price.output_cny;
    }
    cost += usage.audio_seconds / 60.0 * settings.asr_price_per_minute;
    cost
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price() -> super::super::providers::PriceQuote {
        super::super::providers::PriceQuote {
            input_cny: 1.0,
            output_cny: 4.0,
            cache_read_cny: 0.5,
            cache_write_cny: 2.0,
            cache_read_priced: true,
            cache_write_priced: true,
            ..super::super::providers::PriceQuote::default()
        }
    }

    #[test]
    fn subscription_author_overrides_joint_video_owner() {
        let dir = std::env::temp_dir().join(format!("yebian-author-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ProductionStore::open(&dir.join("yebian.sqlite")).unwrap();
        let sub = store
            .save_subscription(
                &dir.to_string_lossy(),
                &crate::production::SubscriptionInput {
                    mid: 397_490_386,
                    name: "肝帝董佳宁".into(),
                    template_id: "video-note".into(),
                    prompt: "速览版".into(),
                    enabled: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let template = crate::production::templates::template("text-note").unwrap();
        let job = store
            .create_job(
                &dir,
                crate::production::StartJobRequest {
                    template_id: "text-note".into(),
                    title: "联合投稿".into(),
                    source: crate::production::SourceSpec {
                        kind: "text".into(),
                        text: "素材".into(),
                        ..Default::default()
                    },
                    options: crate::production::JobOptions {
                        auto: true,
                        subscription_id: sub.id.clone(),
                        ..Default::default()
                    },
                },
                &template,
            )
            .unwrap();
        assert_eq!(subscription_author(&store, &job).as_deref(), Some("肝帝董佳宁"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn costs_each_token_class_and_audio() {
        let usage = Usage {
            uncached_input_tokens: 1_000_000,
            cached_tokens: 1_000_000,
            cache_write_tokens: 1_000_000,
            completion_tokens: 1_000_000,
            audio_seconds: 60.0,
            ..Usage::default()
        };
        let settings = super::super::ProductionSettings {
            asr_price_per_minute: 0.5,
            ..Default::default()
        };
        let cost = estimate_cost(&usage, Some(&price()), &settings);
        assert!((cost - 8.0).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn manual_price_costs_input_and_output_exactly() {
        let manual = super::super::ModelPrice {
            provider: "custom".into(),
            model: "manual-model".into(),
            input_cny: 10.0,
            output_cny: 30.0,
            updated_at: 0,
        };
        let quote = super::super::providers::manual_quote(&manual, 7.2);
        let usage = Usage {
            uncached_input_tokens: 1_000_000,
            completion_tokens: 1_000_000,
            ..Usage::default()
        };
        let cost = estimate_cost(&usage, Some(&quote), &super::super::ProductionSettings::default());
        assert!((cost - 40.0).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn missing_cache_price_falls_back_to_input_price() {
        let usage = Usage {
            cached_tokens: 1_000_000,
            cache_write_tokens: 1_000_000,
            ..Usage::default()
        };
        let mut quote = price();
        quote.cache_read_priced = false;
        quote.cache_write_priced = false;
        let cost = estimate_cost(&usage, Some(&quote), &super::super::ProductionSettings::default());
        assert!((cost - 2.0).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn inserts_xianyin_before_glossary() {
        let body = "# 标题

## 速览
- 一

## 速查
- **条目**（数字）：说明";
        let result = insert_before_section(body, "## 速查", "## 弦外之音
作者在反讽。");
        assert!(result.find("## 弦外之音").unwrap() < result.find("## 速查").unwrap());
        assert!(result.contains("作者在反讽。"));
    }

    #[test]
    fn no_price_records_no_model_cost() {
        let usage = Usage { completion_tokens: 1_000_000, ..Usage::default() };
        let cost = estimate_cost(&usage, None, &super::super::ProductionSettings::default());
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn tail_only_asr_is_rejected() {
        let issue = asr_coverage_issue(414, 7, 1, 193).expect("tail-only transcript must fail");
        assert!(issue.contains("0.47"));
        assert!(issue.contains("1 段有内容"));
    }

    #[test]
    fn full_asr_passes_coverage_gate() {
        assert!(asr_coverage_issue(414, 7, 7, 2_354).is_none());
        assert!(asr_coverage_issue(414, 7, 7, 2_636).is_none());
    }

    #[test]
    fn moderate_undercoverage_is_rejected() {
        assert!(asr_coverage_issue(414, 7, 7, 600).is_some());
    }

    #[test]
    fn short_clip_skips_coverage_gate() {
        assert!(asr_coverage_issue(30, 1, 0, 0).is_none());
    }

    #[test]
    fn mostly_empty_parts_are_rejected_even_with_some_chars() {
        assert!(asr_coverage_issue(414, 7, 2, 300).is_some());
    }
}
