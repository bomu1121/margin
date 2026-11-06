use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use super::clients::{self, AiConfig, AsrConfig, ChatResult, SearchConfig};
use super::media;
use super::prompts;
use super::publish::{self, PublishInput};
use super::store::ProductionStore;
use super::{
    BiliVideoRef, DrillItem, Job, JobOptions, JobResult, JobVariant, TraceItem, Usage,
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
    let preset = super::providers::preset_by_id(&job.provider);
    let protocol = preset
        .as_ref()
        .map(|item| item.protocol.clone())
        .unwrap_or_else(|| settings.ai_protocol.clone());
    let base_url = if job.provider == settings.ai_provider {
        settings.ai_base_url.clone()
    } else {
        preset.as_ref().map(|item| item.base_url.clone()).unwrap_or_default()
    };
    let catalog_id = preset.as_ref().map(|item| item.catalog_id.clone()).unwrap_or_default();
    let price = super::providers::PriceCatalog::load(cache_dir, false)
        .await
        .ok()
        .and_then(|catalog| catalog.price(&catalog_id, &job.model, settings.ai_exchange_rate));
    let ai = AiConfig {
        provider: job.provider.clone(),
        protocol,
        base_url,
        model: job.model.clone(),
        api_key: store.ai_key(&job.provider),
        retries: settings.retries.min(3),
    };
    let asr = AsrConfig {
        engine: settings.asr_engine.clone(),
        base_url: settings.asr_base_url.clone(),
        model: settings.asr_model.clone(),
        api_key: store.get_setting("production.asr.api_key").ok().flatten().unwrap_or_default(),
    };
    let search = SearchConfig {
        provider: settings.search_provider.clone(),
        api_key: store.get_setting("production.search.api_key").ok().flatten().unwrap_or_default(),
    };
    let custom_prompts = store.list_prompts().unwrap_or_default();
    let mut usage = Usage {
        currency: "CNY".into(),
        provider_currency: "USD".into(),
        priced: price.is_some(),
        ..Usage::default()
    };
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
                    let prepared = media::prepare_audio(store, &audio, id, root, item_index, token).await?;
                    let asr_result = clients::transcribe(&asr, &prepared, token).await?;
                    item.text = asr_result.text;
                    usage.audio_seconds += item.duration.max(0) as f64;
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
            item.text = refine_text(&ai, &item.text, token).await?;
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
            let result = generate_with_system(&ai, prompts::gameguide_system(), &title, &text, token).await?;
            add_usage(&mut usage, &result);
            body = result.content;
            mark(store, app, id, index, "done", "攻略完成", token)?;
        }
    } else if template_id == "mindmap" {
        if let Some(index) = find_step(&job, "mindmap") {
            mark(store, app, id, index, "running", "正在整理导图", token)?;
            let text = join_items(&items);
            let result = generate_with_system(&ai, &prompts::mindmap_system(options.mind_depth.max(2)), &title, &text, token).await?;
            add_usage(&mut usage, &result);
            let outline = strip_leading_h1(&result.content);
            mindmap = Some(outline.clone());
            body = format!("# {title}

{outline}");
            mark(store, app, id, index, "done", "导图完成", token)?;
        }
    } else {
        if let Some(index) = find_step(&job, "prompt") {
            mark(store, app, id, index, "running", "正在生成笔记", token)?;
            let styles = styles_for(&template_id, &options, &settings.default_prompt);
            let mut item_notes: Vec<String> = Vec::new();
            for (item_index, item) in items.iter().enumerate() {
                let note = generate_note(&ai, &custom_prompts, &styles, item, &title, token).await?;
                add_usage(&mut usage, &note.0);
                variants.extend(note.1);
                item_notes.push(note.2);
                store.progress(id, progress_for_step(&job, index, item_index, items.len()), "笔记生成中")?;
                emit(app, store, id);
            }
            body = if item_notes.len() > 1 {
                merge_notes(&ai, &title, &item_notes, token).await?
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
            let result = generate_with_system(&ai, &prompts::chapter_system(&options.chapter, options.max_chapters.max(2)), &title, &text, token).await?;
            add_usage(&mut usage, &result);
            body = result.content;
            mark(store, app, id, index, "done", "章节完成", token)?;
        }
        if let Some(index) = find_step(&job, "merge") {
            if items.len() > 1 {
                mark(store, app, id, index, "running", "正在合并多份产物", token)?;
                body = merge_notes(&ai, &title, &[body], token).await?;
                mark(store, app, id, index, "done", "合并完成", token)?;
            } else {
                mark(store, app, id, index, "done", "单份素材，无需合并", token)?;
            }
        }
        if let Some(index) = find_step(&job, "drill") {
            mark(store, app, id, index, "running", "正在出题", token)?;
            let result = generate_with_system(
                &ai,
                &prompts::drill_system(options.drill_count.max(3)),
                &title,
                &body,
                token,
            )
            .await?;
            add_usage(&mut usage, &result);
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
            let hits = clients::search(&search, &format!("{title} 事实核查"), options.trace_max.clamp(1, 10), token).await?;
            let evidence = hits
                .iter()
                .map(|hit| format!("标题：{}\n链接：{}\n摘要：{}", hit.title, hit.url, hit.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            let result = generate_with_system(
                &ai,
                prompts::trace_system(),
                &title,
                &format!("原文：\n{}\n\n检索结果：\n{}", body.chars().take(12000).collect::<String>(), evidence),
                token,
            )
            .await?;
            add_usage(&mut usage, &result);
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
        entities = extract_entities(&ai, &body, &options.fields, token).await.unwrap_or_else(|error| {
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
            estimated_cost: estimate_cost(&usage, price.as_ref(), &settings),
            currency: usage.currency.clone(),
            model: settings.ai_model.clone(),
            prompt: prompt_name,
        },
    )?;
    result.note_path = note_rel.clone();
    if let Some(index) = publish_index {
        mark(store, app, id, index, "done", &note_rel, token)?;
    }
    store.finish(id, &result, &usage, &note_rel)?;
    publish::cleanup_temp(root, id);
    emit(app, store, id);
    Ok(note_rel)
}

async fn resolve_source(
    _store: &ProductionStore,
    job: &mut Job,
    root: &Path,
    token: &CancellationToken,
) -> Result<Vec<Item>, String> {
    let temp = media::job_temp_dir(root, &job.id);
    tokio::fs::create_dir_all(&temp).await.map_err(|error| error.to_string())?;
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
                let audio_url = clients::bili_audio_url(&preview.bvid, page.cid).await?;
                let audio = temp.join(format!("bili-{index}.m4s"));
                clients::download_bili_audio(&audio_url, &audio, token).await?;
                let page_title = if preview.pages.len() > 1 {
                    format!("{} · {}", preview.title, page.part)
                } else {
                    preview.title.clone()
                };
                items.push(Item {
                    text: String::new(),
                    audio: Some(audio),
                    title: page_title.clone(),
                    author: preview.author.clone(),
                    url: format!("https://www.bilibili.com/video/{}?p={}", preview.bvid, page.page),
                    pubdate: preview.pubdate,
                    duration: page.duration,
                });
                job.source.videos.push(BiliVideoRef {
                    bvid: preview.bvid.clone(),
                    title: page_title,
                    author: preview.author.clone(),
                    duration: page.duration,
                    pubdate: preview.pubdate,
                    cover: preview.cover.clone(),
                    cid: page.cid,
                });
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
                let audio_url = clients::bili_audio_url(&preview.bvid, page.cid).await?;
                let audio = temp.join(format!("favorite-{index}.m4s"));
                clients::download_bili_audio(&audio_url, &audio, token).await?;
                items.push(Item {
                    text: String::new(),
                    audio: Some(audio),
                    title: preview.title.clone(),
                    author: preview.author.clone(),
                    url: format!("https://www.bilibili.com/video/{}", preview.bvid),
                    pubdate: preview.pubdate,
                    duration: page.duration,
                });
                if let Some(target) = job.source.videos.get_mut(index) {
                    target.title = preview.title.clone();
                    target.author = preview.author.clone();
                    target.duration = page.duration;
                    target.pubdate = preview.pubdate;
                    target.cover = preview.cover.clone();
                    target.cid = page.cid;
                }
            }
            Ok(items)
        }
        other => Err(format!("不支持的来源：{other}")),
    }
}

async fn refine_text(ai: &AiConfig, text: &str, token: &CancellationToken) -> Result<String, String> {
    let chunks = split_text(text, 6000);
    let mut output = Vec::new();
    for chunk in chunks {
        let result = clients::chat(ai, prompts::refine_system(), &chunk, token).await?;
        output.push(result.content.trim().to_string());
    }
    Ok(output.join("\n\n"))
}

async fn generate_note(
    ai: &AiConfig,
    custom: &[prompts::PromptEntry],
    styles: &[String],
    item: &Item,
    title: &str,
    token: &CancellationToken,
) -> Result<(ChatResult, Vec<JobVariant>, String), String> {
    let chunks = split_text(&item.text, 12000);
    let mut total = ChatResult::default();
    let mut variants = Vec::new();
    let mut primary = String::new();
    for (index, style) in styles.iter().enumerate() {
        let system = prompts::prompt_system(style, custom);
        let mut parts = Vec::new();
        for chunk in &chunks {
            let result = generate_with_system(ai, &system, title, chunk, token).await?;
            merge_usage(&mut total, &result);
            parts.push(result.content);
        }
        let body = if parts.len() > 1 {
            merge_notes(ai, title, &parts, token).await?
        } else {
            parts.into_iter().next().unwrap_or_default()
        };
        if index == 0 {
            primary = body.clone();
        }
        variants.push(JobVariant {
            name: style.clone(),
            summary: format!("{} 字", body.chars().count()),
            body,
        });
    }
    Ok((total, variants, primary))
}

async fn generate_with_system(
    ai: &AiConfig,
    system: &str,
    title: &str,
    text: &str,
    token: &CancellationToken,
) -> Result<ChatResult, String> {
    let user = format!("标题：{title}\n\n素材：\n{text}");
    clients::chat(ai, system, &user, token).await
}

async fn merge_notes(ai: &AiConfig, title: &str, notes: &[String], token: &CancellationToken) -> Result<String, String> {
    if notes.len() <= 1 {
        return Ok(notes.first().cloned().unwrap_or_default());
    }
    let user = format!("标题：{title}\n\n以下是同一主题的多份笔记：\n\n{}", notes.join("\n\n---\n\n"));
    let result = clients::chat(ai, prompts::merge_system(), &user, token).await?;
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
    ai: &AiConfig,
    body: &str,
    fields: &[String],
    token: &CancellationToken,
) -> Result<HashMap<String, Vec<String>>, String> {
    let result = clients::chat(
        ai,
        &prompts::extract_system(fields),
        &body.chars().take(16000).collect::<String>(),
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

fn add_usage(usage: &mut Usage, result: &ChatResult) {
    usage.prompt_tokens += result.prompt_tokens;
    usage.uncached_input_tokens += result.uncached_input_tokens;
    usage.cached_tokens += result.cached_tokens;
    usage.cache_write_tokens += result.cache_write_tokens;
    usage.completion_tokens += result.completion_tokens;
    usage.reasoning_tokens += result.reasoning_tokens;
    if let Some(cost) = result.provider_cost {
        usage.provider_cost += cost;
    }
}

fn merge_usage(total: &mut ChatResult, result: &ChatResult) {
    total.prompt_tokens += result.prompt_tokens;
    total.uncached_input_tokens += result.uncached_input_tokens;
    total.cached_tokens += result.cached_tokens;
    total.cache_write_tokens += result.cache_write_tokens;
    total.completion_tokens += result.completion_tokens;
    total.reasoning_tokens += result.reasoning_tokens;
    if let Some(cost) = result.provider_cost {
        total.provider_cost = Some(total.provider_cost.unwrap_or(0.0) + cost);
    }
    total.content.push_str(&result.content);
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
    fn no_price_records_no_model_cost() {
        let usage = Usage { completion_tokens: 1_000_000, ..Usage::default() };
        let cost = estimate_cost(&usage, None, &super::super::ProductionSettings::default());
        assert_eq!(cost, 0.0);
    }
}
