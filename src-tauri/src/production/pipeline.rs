use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
    store.start_job(id)?;
    emit(app, store, id);

    let settings = store.settings();
    let mut config = store.job_config(id)?.unwrap_or_else(|| config_from_job(&job, &settings));
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
    let search = SearchConfig {
        provider: config.search_provider.clone(),
        api_key: store.get_setting("production.search.api_key").ok().flatten().unwrap_or_default(),
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
                                let result = clients::transcribe_single(&asr, part, token).await?;
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
                    store.log(id, Some(index), "info", &format!("转写稿 {}（{} 字）：
{}", item_index + 1, item.text.chars().count(), item.text))?;
                    let detail = format!("{}/{} · {} 字", item_index + 1, total_items, item.text.chars().count());
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
            store.progress(id, progress_for_step(&job, index, item_index, total_items), "校对完成")?;
            emit(app, store, id);
        }
        mark(store, app, id, index, "done", "校对完成", token)?;
    }

    let template_id = job.template_id.clone();
    let options = job.options.clone();
    let title = job.title.clone();
    let mut variants: Vec<JobVariant> = Vec::new();
    let mut body = String::new();
    let mut mindmap: Option<String> = None;
    let mut drill: Vec<DrillItem> = Vec::new();
    let mut trace: Vec<TraceItem> = Vec::new();

    if template_id == "gameguide" {
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
            let hits = checkpoint::memo(
                store,
                id,
                "trace",
                "trace:search",
                "search",
                &checkpoint::hash_json(&serde_json::json!({
                    "pipelineVersion": checkpoint::PIPELINE_VERSION,
                    "query": format!("{title} 事实核查"),
                    "provider": search.provider,
                    "max": options.trace_max.clamp(1, 10),
                }))?,
                &mut usage,
                &cost,
                || async {
                    let hits = clients::search(&search, &format!("{title} 事实核查"), options.trace_max.clamp(1, 10), token).await?;
                    Ok((hits, Usage::default()))
                },
            )
            .await?;
            let evidence = hits
                .iter()
                .map(|hit| format!("标题：{}\n链接：{}\n摘要：{}", hit.title, hit.url, hit.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            let user = format!("标题：{title}

素材：
原文：
{}

检索结果：
{}", body.chars().take(12000).collect::<String>(), evidence);
            let result = checkpoint::cached_chat(store, id, "trace", "trace:chat", prompts::trace_system(), &user, &ai, &mut usage, &cost, token).await?;
            trace = parse_trace(&result.content);
            body.push_str("\n\n## 溯源\n\n");
            for item in &trace {
                body.push_str(&format!("- **{}** · {}\n", item.claim, item.verdict));
                if !item.quote.is_empty() {
                    body.push_str(&format!("  > {}\n", item.quote));
                }
            }
            mark(store, app, id, index, "done", "溯源完成", token)?;
        }
    }

    if token.is_cancelled() {
        return Err("已取消".into());
    }
    if body.trim().is_empty() {
        return Err("模型没有返回笔记正文".into());
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
    let prompt_name = options.prompt.clone();
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
        },
    )?;
    result.note_path = note_rel.clone();
    if let Some(index) = publish_index {
        mark(store, app, id, index, "done", &note_rel, token)?;
    }
    store.finish(id, &result, &usage, &note_rel)?;
    if let Err(error) = store.clear_checkpoints(id) {
        let _ = store.log(id, None, "info", &format!("清理检查点失败：{error}"));
    }
    publish::cleanup_temp(root, id);
    emit(app, store, id);
    Ok(note_rel)
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
                let item = if let Some(item) = cached_source_item(store, &job.id, &key, &input_hash)? {
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
                        author: preview.author.clone(),
                        url: format!("https://www.bilibili.com/video/{}?p={}", preview.bvid, page.page),
                        pubdate: preview.pubdate,
                        duration: page.duration,
                    };
                    save_source_item(store, &job.id, &key, &input_hash, &item)?;
                    item
                };
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

fn parse_trace(content: &str) -> Vec<TraceItem> {
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
        .filter_map(|item| {
            Some(TraceItem {
                claim: item.get("claim").and_then(Value::as_str)?.to_string(),
                verdict: item.get("verdict").and_then(Value::as_str).unwrap_or("未核查").to_string(),
                source: item.get("source").and_then(Value::as_str).unwrap_or_default().to_string(),
                quote: item.get("quote").and_then(Value::as_str).unwrap_or_default().to_string(),
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
    emit(app, store, id);
    Ok(())
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
}
