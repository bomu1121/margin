//! Single-shot note engine.
//!
//! ZCode's note runs used a general-purpose subagent: a short system prompt, the
//! full file tool set, native tool calls, and a note written to a file. This
//! module reproduces that process in the desktop process instead of parking a
//! task for a webview worker.
//!
//! One job = one task. There is no chat layer and no follow-up message; the
//! model may use as many turns and tools as the task needs. If the user wants a
//! different note, the job is rerun from scratch.

use std::path::{Component, Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::clients::{self, AiConfig, ChatMessage, ToolCall, ToolChatResult, ToolSpec};
use super::Usage;

/// The built-in ZCode `general-purpose` subagent prompt, read from
/// `~/.zcode/cli/agents/<sess>/agent_*/metadata.json` on 2026-11-19.
/// See `docs/ZCODE_ENGINE_SPEC.md` §2.2.
const GENERAL_PURPOSE_PROMPT: &str = "You are an agent for ZCode CLI. Given the user's message, you should use the tools available to complete the task. Complete the task fully—don't gold-plate, but don't leave it half-done. When you complete the task, respond with a concise report covering what was done and any key findings — the caller will relay this to the user, so it only needs the essentials.

Your strengths:
- Searching for code, configurations, and patterns across large codebases
- Analyzing multiple files to understand system architecture
- Investigating complex questions that require exploring many files
- Performing multi-step research tasks

Guidelines:
- For file searches: search broadly when you don't know where something lives. Use Read when you know the specific file path.
- For analysis: Start broad and narrow down. Use multiple search strategies if the first doesn't yield results.
- Be thorough: Check multiple locations, consider different naming conventions, look for related files.
- NEVER create files unless they're absolutely necessary for achieving your goal. ALWAYS prefer editing an existing file to creating a new one.
- NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested.";

pub const DEFAULT_TASK: &str = "把素材写成一份可直接阅读的笔记，保存到 note.md。只使用素材里的信息，不编造数字、书名、链接。";

/// Deterministic acceptance rules. The model's own "done" is not evidence;
/// the brief's constraints are checked here.
#[derive(Debug, Clone)]
pub struct NoteGateSpec {
    pub min_cjk: usize,
    pub max_cjk: usize,
    pub min_h2: usize,
    pub max_h2: usize,
    pub allow_tables: bool,
    /// Empty = any figure kind (or no figures); non-empty = whitelist.
    pub figure_kinds: Vec<String>,
    /// Any H2 heading containing one of these strings is a violation.
    pub forbidden_h2: Vec<String>,
}

impl Default for NoteGateSpec {
    fn default() -> Self {
        Self {
            min_cjk: 200,
            max_cjk: usize::MAX,
            min_h2: 0,
            max_h2: usize::MAX,
            allow_tables: true,
            figure_kinds: Vec::new(),
            forbidden_h2: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoteEngineOptions {
    /// Safety cap, not a shaper. ZCode's own runs used 4–13 turns; 20 is still
    /// above that, and the progress guard below normally stops much earlier.
    pub max_rounds: usize,
    pub max_tokens: Option<u32>,
    /// `None` leaves the provider default (ZCode's request body omitted it).
    pub temperature: Option<f32>,
    pub gate: NoteGateSpec,
    /// If the model stops but the deterministic gate fails, feed the violations
    /// back and give it this many repair turns before failing.
    pub max_repairs: usize,
    /// Hard cost guard: stop a run that keeps re-reading/re-polishing. Cached
    /// input is cheap but 37 turns and 2M prompt tokens are still a bad run.
    pub max_prompt_tokens: u64,
}

impl Default for NoteEngineOptions {
    fn default() -> Self {
        Self {
            max_rounds: 20,
            max_tokens: Some(32_768),
            temperature: None,
            gate: NoteGateSpec::default(),
            max_repairs: 2,
            max_prompt_tokens: 600_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineTokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub reasoning_tokens: u64,
    pub cached_tokens: u64,
    pub cache_write_tokens: u64,
}

impl EngineTokenUsage {
    fn from_response(response: &ToolChatResult) -> Self {
        Self {
            prompt_tokens: response.prompt_tokens,
            completion_tokens: response.completion_tokens,
            reasoning_tokens: response.reasoning_tokens,
            cached_tokens: response.cached_tokens,
            cache_write_tokens: response.cache_write_tokens,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NoteTurn {
    pub index: usize,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default)]
    pub observation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<EngineTokenUsage>,
    #[serde(default)]
    pub model_ms: u64,
    #[serde(default)]
    pub tool_ms: u64,
    #[serde(default)]
    pub started_at: i64,
    #[serde(default)]
    pub ended_at: i64,
}

#[derive(Debug, Clone, Copy, Default)]
struct EngineUsage {
    prompt_tokens: u64,
    uncached_input_tokens: u64,
    completion_tokens: u64,
    cached_tokens: u64,
    cache_write_tokens: u64,
    reasoning_tokens: u64,
}

impl EngineUsage {
    fn add(&mut self, response: &ToolChatResult) {
        self.prompt_tokens += response.prompt_tokens;
        self.uncached_input_tokens += response.uncached_input_tokens;
        self.completion_tokens += response.completion_tokens;
        self.cached_tokens += response.cached_tokens;
        self.cache_write_tokens += response.cache_write_tokens;
        self.reasoning_tokens += response.reasoning_tokens;
    }

    fn to_usage(self) -> Usage {
        Usage {
            prompt_tokens: self.prompt_tokens,
            uncached_input_tokens: self.uncached_input_tokens,
            completion_tokens: self.completion_tokens,
            cached_tokens: self.cached_tokens,
            cache_write_tokens: self.cache_write_tokens,
            reasoning_tokens: self.reasoning_tokens,
            ..Usage::default()
        }
    }
}

pub struct NoteEngineResult {
    pub note: String,
    pub turns: Vec<NoteTurn>,
    pub usage: Usage,
    pub workspace: PathBuf,
    pub rounds: usize,
    pub run_json: String,
    pub note_source: String,
}

pub struct NoteEngineFailure {
    pub error: String,
    pub run_json: String,
    pub usage: Usage,
    pub draft: String,
}

pub struct NoteEngineHooks<'a> {
    pub log: &'a (dyn Fn(&str) + Send + Sync + 'a),
    pub turn: &'a (dyn Fn(&NoteTurn) + Send + Sync + 'a),
}

fn noop_log(_: &str) {}
fn noop_turn(_: &NoteTurn) {}

impl NoteEngineHooks<'static> {
    pub fn noop() -> Self {
        Self { log: &noop_log, turn: &noop_turn }
    }
}

pub fn workspace(root: &Path, job_id: &str) -> PathBuf {
    let safe: String = job_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' { ch } else { '_' })
        .collect();
    root.join(".yebian").join("runs").join(safe).join("note-engine")
}

fn prepare(workspace: &Path, material: &str) -> Result<(), String> {
    std::fs::create_dir_all(workspace).map_err(|error| format!("创建引擎工作区失败：{error}"))?;
    std::fs::write(workspace.join("material.md"), material.as_bytes())
        .map_err(|error| format!("写入素材失败：{error}"))?;
    let note = workspace.join("note.md");
    if note.exists() {
        std::fs::remove_file(&note).map_err(|error| format!("清理旧笔记失败：{error}"))?;
    }
    Ok(())
}

fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "read_file".into(),
            description: "读取工作区里的文本文件。offset/limit 按字符计数；文件上限 512 KB。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "工作区内的相对路径，例如 material.md"},
                    "offset": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1}
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "write_file".into(),
            description: "写入工作区里的文本文件（新建或覆盖）。写最终笔记用 note.md。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"]
            }),
        },
        ToolSpec {
            name: "edit_file".into(),
            description: "按唯一片段替换工作区文件的内容；old 必须在文件里恰好出现一次。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old": {"type": "string"},
                    "new": {"type": "string"}
                },
                "required": ["path", "old", "new"]
            }),
        },
        ToolSpec {
            name: "list_dir".into(),
            description: "列出工作区某个目录的条目。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": []
            }),
        },
    ]
}

fn text_arg(args: &Value, key: &str) -> String {
    args.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn int_arg(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(Value::as_u64).map(|value| value as usize)
}

fn resolve_path(root: &Path, input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    let relative = if trimmed.is_empty() { PathBuf::new() } else { Path::new(trimmed).to_path_buf() };
    for component in relative.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err("只允许工作区内的相对路径，不能使用绝对路径或 ..".into()),
        }
    }
    Ok(root.join(relative))
}

fn tool_read_file(root: &Path, args: &Value) -> String {
    let path = text_arg(args, "path");
    if path.is_empty() {
        return "缺少 path".into();
    }
    let resolved = match resolve_path(root, &path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let bytes = match std::fs::read(&resolved) {
        Ok(value) => value,
        Err(error) => return format!("读取失败：{error}"),
    };
    if bytes.len() > 512 * 1024 {
        return format!("文件 {} B，超过 512 KB", bytes.len());
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    let total = text.chars().count();
    let offset = int_arg(args, "offset").unwrap_or(0).min(total);
    let limit = int_arg(args, "limit").unwrap_or(12_000).clamp(500, 40_000);
    let slice: String = text.chars().skip(offset).take(limit).collect();
    let end = offset + slice.chars().count();
    let suffix = if end < total { format!("\n…（{end}/{total}，继续用 offset={end} 读）") } else { String::new() };
    format!("{} · {} B\n{}{}", path, bytes.len(), slice, suffix)
}

fn tool_write_file(root: &Path, args: &Value, spec: &NoteGateSpec) -> String {
    let path = text_arg(args, "path");
    if path.is_empty() {
        return "缺少 path".into();
    }
    let content = text_arg(args, "content");
    let resolved = match resolve_path(root, &path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Some(parent) = resolved.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return format!("创建目录失败：{error}");
        }
    }
    let base = match std::fs::write(&resolved, content.as_bytes()) {
        Ok(()) => format!("已写 {}（{} B）", path, content.len()),
        Err(error) => format!("写入失败：{error}"),
    };
    if is_note_path(&path) {
        format!("{base}{}", note_status(root, spec))
    } else {
        base
    }
}

fn tool_edit_file(root: &Path, args: &Value, spec: &NoteGateSpec) -> String {
    let path = text_arg(args, "path");
    let old = text_arg(args, "old");
    let new = text_arg(args, "new");
    if path.is_empty() || old.is_empty() {
        return "参数不完整：需要 path 和 old".into();
    }
    let resolved = match resolve_path(root, &path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let text = match std::fs::read_to_string(&resolved) {
        Ok(value) => value,
        Err(error) => return format!("读取失败：{error}"),
    };
    let count = text.matches(&old).count();
    if count != 1 {
        return format!("old 在文件里出现 {count} 次，必须恰好 1 次");
    }
    let next = text.replacen(&old, &new, 1);
    let base = match std::fs::write(&resolved, next.as_bytes()) {
        Ok(()) => format!("已改 {}（{} B）", path, next.len()),
        Err(error) => format!("写入失败：{error}"),
    };
    if is_note_path(&path) {
        format!("{base}{}", note_status(root, spec))
    } else {
        base
    }
}

fn tool_list_dir(root: &Path, args: &Value) -> String {
    let path = text_arg(args, "path");
    let resolved = match resolve_path(root, &path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let entries = match std::fs::read_dir(&resolved) {
        Ok(value) => value,
        Err(error) => return format!("列目录失败：{error}"),
    };
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata();
        let label = match meta {
            Ok(meta) if meta.is_dir() => "[d]".to_string(),
            Ok(meta) => format!("[{}B]", meta.len()),
            Err(_) => "[?]".to_string(),
        };
        items.push(format!("{label} {name}"));
    }
    items.sort();
    if items.len() > 200 {
        items.truncate(200);
        items.push("…（超过 200 条，已截断）".into());
    }
    let shown = if path.trim().is_empty() { ".".to_string() } else { path };
    format!("{}\n{}", shown, items.join("\n"))
}

fn execute_tool(root: &Path, call: &ToolCall, spec: &NoteGateSpec) -> (String, Option<Value>) {
    let args = serde_json::from_str::<Value>(&call.function.arguments).ok();
    let Some(args) = args else {
        return ("工具参数不是合法 JSON".into(), None);
    };
    let observation = match call.function.name.as_str() {
        "read_file" => tool_read_file(root, &args),
        "write_file" => tool_write_file(root, &args, spec),
        "edit_file" => tool_edit_file(root, &args, spec),
        "list_dir" => tool_list_dir(root, &args),
        other => format!("未知工具：{other}"),
    };
    (observation, Some(args))
}

/// Deterministic gate: the model's own "done" is not evidence.
pub fn gate_with_spec(note: &str, spec: &NoteGateSpec) -> Vec<String> {
    let text = note.trim();
    let mut issues = Vec::new();
    let h1 = text.lines().filter(|line| line.starts_with("# ")).count();
    if h1 != 1 {
        issues.push(format!("H1 {h1} 个，应为 1"));
    }
    let h2_lines: Vec<&str> = text.lines().filter(|line| line.starts_with("## ")).collect();
    let h2 = h2_lines.len();
    if h2 < spec.min_h2 || h2 > spec.max_h2 {
        issues.push(format!("H2 {h2} 个，应为 {}-{}", spec.min_h2, spec.max_h2));
    }
    let cjk = text.chars().filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch)).count();
    if cjk < spec.min_cjk || cjk > spec.max_cjk {
        if spec.max_cjk == usize::MAX {
            issues.push(format!("正文 {cjk} 字，低于 {}；请补一段有依据的内容", spec.min_cjk));
        } else {
            let over = cjk.saturating_sub(spec.max_cjk);
            let under = spec.min_cjk.saturating_sub(cjk);
            let hint = if over > 0 {
                format!("超出 {over} 字；请一次删减至少 {} 字（整段/整句），不要逐字润色", over.min(200).max(50))
            } else {
                format!("还差 {under} 字；请补一段有依据的内容")
            };
            issues.push(format!("正文 {cjk} 字，应为 {}-{}；{hint}", spec.min_cjk, spec.max_cjk));
        }
    }
    let tables = text.lines().filter(|line| line.trim_start().starts_with('|')).count();
    if !spec.allow_tables && tables > 0 {
        issues.push(format!("正文有 {tables} 行表格，应无表格"));
    }

    let mut in_fence = false;
    let mut figure = false;
    let mut kind = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if in_fence {
            if trimmed == "```" {
                in_fence = false;
                if figure {
                    if !spec.figure_kinds.is_empty() && !spec.figure_kinds.iter().any(|item| item == &kind) {
                        issues.push(format!("图 kind「{}」不在白名单", if kind.is_empty() { "缺失" } else { &kind }));
                    }
                    figure = false;
                    kind.clear();
                }
            } else if figure && trimmed.starts_with("kind:") {
                kind = trimmed[5..].trim().to_string();
            }
            continue;
        }
        if trimmed.starts_with("```") {
            in_fence = true;
            figure = trimmed.starts_with("```figure");
            kind.clear();
        }
    }
    if in_fence {
        issues.push("代码块没闭合".into());
    }
    for heading in &h2_lines {
        for forbidden in &spec.forbidden_h2 {
            if heading.contains(forbidden) {
                issues.push(format!("禁用小节「{forbidden}」"));
            }
        }
    }
    issues
}

/// Backwards-compatible helper for callers that only need a minimum length.
pub fn gate_note(note: &str, min_cjk: usize) -> Result<(), String> {
    let spec = NoteGateSpec { min_cjk, ..Default::default() };
    let issues = gate_with_spec(note, &spec);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues.join("；"))
    }
}

fn is_note_path(path: &str) -> bool {
    path.trim().trim_start_matches("./") == "note.md"
}

/// Appended to every note.md write. The author sees whether the note is
/// structurally done without spending another tool call on reading it.
fn note_status(root: &Path, spec: &NoteGateSpec) -> String {
    let text = match std::fs::read_to_string(root.join("note.md")) {
        Ok(value) => value,
        Err(_) => return String::new(),
    };
    let h1 = text.lines().filter(|line| line.starts_with("# ")).count();
    let h2 = text.lines().filter(|line| line.starts_with("## ")).count();
    let cjk = text.chars().filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch)).count();
    let tables = text.lines().filter(|line| line.trim_start().starts_with('|')).count();
    let figures = text.matches("```figure").count();
    let violations = gate_with_spec(&text, spec);
    let gate = if violations.is_empty() {
        "通过".to_string()
    } else {
        format!("未通过（{}）", violations.join("；"))
    };
    format!("；结构：H1 {h1} · H2 {h2} · {cjk} 字 · 表格 {tables} · 图 {figures}；门禁：{gate}")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn build_run_json(
    status: &str,
    error: Option<&str>,
    task: &str,
    turns: &[NoteTurn],
    usage: &EngineUsage,
    draft: &str,
    started_at: i64,
    rounds: usize,
) -> String {
    let phases = if status == "done" { vec!["idle", "model", "done"] } else { vec!["idle", "model", "failed"] };
    json!({
        "id": format!("engine_{started_at}"),
        "status": status,
        "goal": task,
        "phases": phases,
        "turns": turns,
        "compacted": 0,
        "todos": [],
        "startedAt": started_at,
        "endedAt": now_ms(),
        "usage": {
            "promptTokens": usage.prompt_tokens,
            "completionTokens": usage.completion_tokens,
            "reasoningTokens": usage.reasoning_tokens
        },
        "draft": draft,
        "rounds": rounds,
        "error": error,
    })
    .to_string()
}

fn failure(
    message: impl Into<String>,
    task: &str,
    turns: &[NoteTurn],
    usage: &EngineUsage,
    draft: &str,
    started_at: i64,
    rounds: usize,
) -> NoteEngineFailure {
    let error = message.into();
    NoteEngineFailure {
        error: error.clone(),
        run_json: build_run_json("failed", Some(&error), task, turns, usage, draft, started_at, rounds),
        usage: usage.to_usage(),
        draft: draft.to_string(),
    }
}

pub async fn run(
    ai: &AiConfig,
    root: &Path,
    job_id: &str,
    title: &str,
    material: &str,
    task: Option<&str>,
    options: &NoteEngineOptions,
    hooks: &NoteEngineHooks<'_>,
    token: &CancellationToken,
) -> Result<NoteEngineResult, NoteEngineFailure> {
    let started_at = now_ms();
    let task = task.unwrap_or(DEFAULT_TASK).to_string();
    let workspace = workspace(root, job_id);
    if let Err(error) = prepare(&workspace, material) {
        return Err(failure(error, &task, &[], &EngineUsage::default(), "", started_at, 0));
    }
    let system = format!(
        "{GENERAL_PURPOSE_PROMPT}

## 本次任务
你不是在写代码，是在给用户写一份可阅读的笔记。工作区：{}。
素材在 material.md；最终笔记写到 note.md。
用 read_file 读素材，用 write_file / edit_file 写笔记；只操作工作区内的文件。只使用素材里的信息，不编造数字、书名、链接。
写完笔记后用一句话说明你写了什么。如果 note.md 已经满足任务要求，不要再反复读/改；最多做一次自检，然后停止。门禁提示字数或结构不合格时，一次改到位（删减/增加足够字数，或整段重写），不要逐字微调。

## 图
需要解释流程、结构或对比时，用 figure 代码块画图；图紧挨着它解释的内容，要能独立读懂，不要为了配图而配图。
支持 flow / layers / bar / quote / compare / svg。示例：
```figure
kind: flow
title: 请求先到 Router，再分给 Worker
浏览器 -> Router
Router -> Worker A
Router -> Worker B
note: Worker 之间不直接通信。
```
```figure
kind: compare
title: 旧做法 vs 分块
left: 旧做法 | 全量遍历
right: 分块 | 只算视口
left-item: 首屏 | 慢
right-item: 首屏 | 快
note: 代价是块边界出现短暂空白。
```",
        workspace.display()
    );
    let user = format!(
        "{task}\n\n工作区：{}\n素材文件：material.md\n输出文件：note.md",
        workspace.display()
    );
    let mut messages = vec![ChatMessage::system(system), ChatMessage::user(user)];
    let tools = tool_specs();
    let mut turns: Vec<NoteTurn> = Vec::new();
    let mut total = EngineUsage::default();
    let mut last_content = String::new();
    let mut rounds = 0usize;
    let mut finished = false;
    let mut repairs = 0usize;
    let mut final_note: Option<String> = None;
    let mut last_note_len = 0usize;
    let mut stable_writes = 0usize;
    let mut stop_after_tools = false;
    let mut micro_notice_sent = false;

    for round in 0..options.max_rounds {
        if token.is_cancelled() {
            return Err(failure("已取消", &task, &turns, &total, &last_content, started_at, rounds));
        }
        rounds = round + 1;
        (hooks.log)(&format!("引擎第 {} 回合：请求模型", rounds));
        let model_start = Instant::now();
        let turn_started = now_ms();
        let response = match clients::chat_tools(
            ai,
            &messages,
            &tools,
            options.temperature,
            options.max_tokens,
            token,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                return Err(failure(
                    format!("第 {} 回合模型失败：{error}", rounds),
                    &task,
                    &turns,
                    &total,
                    &last_content,
                    started_at,
                    rounds,
                ))
            }
        };
        let model_ms = model_start.elapsed().as_millis() as u64;
        total.add(&response);
        if total.prompt_tokens > options.max_prompt_tokens {
            return Err(failure(
                format!("token 预算耗尽：{} / {}", total.prompt_tokens, options.max_prompt_tokens),
                &task,
                &turns,
                &total,
                &last_content,
                started_at,
                rounds,
            ));
        }
        let turn_usage = EngineTokenUsage::from_response(&response);
        if !response.content.is_empty() {
            last_content = response.content.clone();
        }
        let model_turn = NoteTurn {
            index: turns.len(),
            phase: "model".into(),
            tool: None,
            args: None,
            observation: String::new(),
            note: None,
            content: if response.content.is_empty() { None } else { Some(response.content.clone()) },
            reasoning: response.reasoning_content.clone(),
            usage: Some(turn_usage),
            model_ms,
            tool_ms: 0,
            started_at: turn_started,
            ended_at: now_ms(),
        };
        turns.push(model_turn.clone());
        (hooks.turn)(&model_turn);

        if response.tool_calls.is_empty() {
            let candidate = std::fs::read_to_string(workspace.join("note.md"))
                .ok()
                .filter(|text| !text.trim().is_empty())
                .or_else(|| if response.content.trim().starts_with("# ") { Some(response.content.clone()) } else { None });
            let Some(candidate) = candidate else {
                return Err(failure(
                    "模型没有写出 note.md",
                    &task,
                    &turns,
                    &total,
                    &last_content,
                    started_at,
                    rounds,
                ));
            };
            let issues = gate_with_spec(&candidate, &options.gate);
            if issues.is_empty() {
                final_note = Some(candidate);
                finished = true;
                break;
            }
            if repairs < options.max_repairs {
                repairs += 1;
                (hooks.log)(&format!(
                    "结构门禁未通过：{}；追加第 {} 次修复回合",
                    issues.join("；"),
                    repairs
                ));
                messages.push(ChatMessage::user(format!(
                    "结构门禁未通过：\n- {}\n\n请只修这些问题，保持其余内容不变；修完再结束。",
                    issues.join("\n- ")
                )));
                continue;
            }
            return Err(failure(
                format!("结构门禁失败：{}", issues.join("；")),
                &task,
                &turns,
                &total,
                &candidate,
                started_at,
                rounds,
            ));
        }

        let calls = response.tool_calls.clone();
        let mut micro_notice: Option<String> = None;
        messages.push(ChatMessage::assistant(
            if response.content.is_empty() { None } else { Some(response.content.clone()) },
            calls.clone(),
        ));
        for call in calls {
            if token.is_cancelled() {
                return Err(failure("已取消", &task, &turns, &total, &last_content, started_at, rounds));
            }
            let tool_start = Instant::now();
            let turn_started = now_ms();
            let (observation, args) = execute_tool(&workspace, &call, &options.gate);
            let tool_ms = tool_start.elapsed().as_millis() as u64;
            messages.push(ChatMessage::tool(call.id.clone(), observation.clone()));
            let tool_turn = NoteTurn {
                index: turns.len(),
                phase: "observe".into(),
                tool: Some(call.function.name.clone()),
                args,
                observation: observation.clone(),
                note: None,
                content: None,
                reasoning: None,
                usage: None,
                model_ms: 0,
                tool_ms,
                started_at: turn_started,
                ended_at: now_ms(),
            };
            turns.push(tool_turn.clone());
            (hooks.turn)(&tool_turn);
            (hooks.log)(&format!(
                "#{} {} · {}",
                turns.len(),
                call.function.name,
                observation.chars().take(160).collect::<String>()
            ));
            let tool_path = serde_json::from_str::<Value>(&call.function.arguments)
                .ok()
                .map(|value| text_arg(&value, "path"))
                .unwrap_or_default();
            if (call.function.name == "write_file" || call.function.name == "edit_file")
                && is_note_path(&tool_path)
            {
                if let Ok(text) = std::fs::read_to_string(workspace.join("note.md")) {
                    let len = text.chars().count();
                    if last_note_len > 0 && len.abs_diff(last_note_len) * 100 < last_note_len * 2 {
                        stable_writes += 1;
                    } else {
                        stable_writes = 0;
                    }
                    last_note_len = len;
                    let violations = gate_with_spec(&text, &options.gate);
                    if stable_writes >= 3 && violations.is_empty() {
                        stop_after_tools = true;
                    } else if stable_writes >= 3 && !violations.is_empty() && !micro_notice_sent {
                        micro_notice = Some(format!(
                            "你已连续 3 次只做小于 2% 的微调，但门禁仍未通过：{}。请一次改到位：按提示删减/增加足够字数，或整段重写；不要逐字润色。",
                            violations.join("；")
                        ));
                    }
                }
            }
            if stop_after_tools {
                break;
            }
        }
        if let Some(message) = micro_notice.take() {
            micro_notice_sent = true;
            (hooks.log)("门禁未通过且连续微调；已要求一次改到位");
            messages.push(ChatMessage::user(message));
        }
        if stop_after_tools {
            finished = true;
            (hooks.log)("检测到连续 3 次小于 2% 的润色，且门禁通过；停止，避免任务打转");
            break;
        }
    }

    if !finished {
        return Err(failure(
            format!("达到最大回合 {}，模型没有结束", options.max_rounds),
            &task,
            &turns,
            &total,
            &last_content,
            started_at,
            rounds,
        ));
    }

    let (note, note_source) = if let Some(note) = final_note {
        (note, "note.md".to_string())
    } else {
        match std::fs::read_to_string(workspace.join("note.md")) {
            Ok(text) if !text.trim().is_empty() => (text, "note.md".to_string()),
            _ if last_content.trim().starts_with("# ") => (last_content.clone(), "final-content".to_string()),
            _ => {
                return Err(failure(
                    "模型没有写出 note.md",
                    &task,
                    &turns,
                    &total,
                    &last_content,
                    started_at,
                    rounds,
                ))
            }
        }
    };
    let issues = gate_with_spec(&note, &options.gate);
    if !issues.is_empty() {
        return Err(failure(
            format!("结构门禁失败：{}", issues.join("；")),
            &task,
            &turns,
            &total,
            &note,
            started_at,
            rounds,
        ));
    }
    let usage = total.to_usage();
    let run_json = build_run_json("done", None, &task, &turns, &total, &note, started_at, rounds);
    Ok(NoteEngineResult { note, turns, usage, workspace, rounds, run_json, note_source })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "yebian-note-engine-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("workspace");
        dir
    }

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: format!("call_{name}"),
            kind: "function".into(),
            function: clients::ToolCallFunction {
                name: name.into(),
                arguments: args.to_string(),
            },
        }
    }

    #[test]
    fn file_tools_stay_inside_the_workspace() {
        let root = temp_workspace("files");
        let (observation, _) = execute_tool(
            &root,
            &call("write_file", json!({"path": "note.md", "content": "# 标题\n\n正文"})),
            &NoteGateSpec::default(),
        );
        assert!(observation.contains("已写 note.md"), "{observation}");
        let (observation, _) = execute_tool(&root, &call("read_file", json!({"path": "note.md"})), &NoteGateSpec::default());
        assert!(observation.contains("# 标题"), "{observation}");
        let (observation, _) = execute_tool(
            &root,
            &call("edit_file", json!({"path": "note.md", "old": "正文", "new": "新正文"})),
            &NoteGateSpec::default(),
        );
        assert!(observation.contains("已改 note.md"), "{observation}");
        let (observation, _) = execute_tool(&root, &call("list_dir", json!({"path": ""})), &NoteGateSpec::default());
        assert!(observation.contains("note.md"), "{observation}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_absolute_and_parent_paths() {
        let root = temp_workspace("escape");
        assert!(resolve_path(&root, "../outside.md").is_err());
        assert!(resolve_path(&root, "C:\\Windows\\win.ini").is_err());
        assert!(resolve_path(&root, "sub/note.md").is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn edit_requires_a_single_occurrence() {
        let root = temp_workspace("edit");
        std::fs::write(root.join("note.md"), "同名 同名").expect("write");
        let (observation, _) = execute_tool(
            &root,
            &call("edit_file", json!({"path": "note.md", "old": "同名", "new": "新名"})),
            &NoteGateSpec::default(),
        );
        assert!(observation.contains("出现 2 次"), "{observation}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gate_requires_title_and_substance() {
        assert!(gate_note("# 标题\n\n太短", 200).is_err());
        let body = format!("# 标题\n\n{}", "字".repeat(200));
        assert!(gate_note(&body, 200).is_ok());
        assert!(gate_note("没有标题\n\n{}", 1).is_err());
    }

    #[test]
    fn turn_json_uses_the_frontend_field_names() {
        let turn = NoteTurn {
            index: 0,
            phase: "model".into(),
            model_ms: 12,
            tool_ms: 3,
            started_at: 1,
            ended_at: 2,
            ..Default::default()
        };
        let value = serde_json::to_value(&turn).expect("json");
        assert_eq!(value["modelMs"], 12);
        assert_eq!(value["toolMs"], 3);
        assert_eq!(value["startedAt"], 1);
    }
}
