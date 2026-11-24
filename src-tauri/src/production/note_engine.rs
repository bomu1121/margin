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
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::clients::{self, AiConfig, ChatMessage, SearchConfig, ToolCall, ToolChatResult, ToolSpec};
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

/// Research mode used by the whiteboard. The task message carries the user's
/// question and the selected cards; this prompt sets the evidence posture.
pub const WHITEBOARD_RESEARCH_PROMPT: &str = "You are a research assistant working from a user's whiteboard. The material is a set of cards the user selected: notes, links and image descriptions. Read material.md first. Fetch the links with fetch_url; use search_web when it is available to verify or expand. Write a Chinese research note to note.md: lead with the conclusion, then separate facts, inference and open questions. Every key fact needs its source URL. Never invent numbers, names, quotes or links. If the material does not support a claim, say it is unknown. The note should be useful for a later reader, not a transcript of your process.";

/// Machine-readable source list for the provenance agent. Kept out of the
/// visible note by using an HTML comment; the note itself stays human-first.
const CLAIMS_PROMPT: &str = r#"## 史源清单
若正文有可核史实，在 note.md 末尾追加一个 HTML 注释块：
<!-- yebian-claims
[{"quote":"正文原句","kind":"史载","work":"书名","section":"篇名/卷次","original":"史书原文","url":""}]
-->
规则：quote 必须逐字来自正文；只列史载/转述，判断不要列；最多 8 条。不要检索、不要补写素材里没有的书名或原文。史载/转述的 work 不能为空；original 只能是史书原文，不能填素材的现代转述；素材没有书名或原文时一律写 kind:"待查"，work/original 留空。没有史实就写 []。视觉块不要写新的史实。"#;

#[derive(Debug, Clone)]
pub struct NoteEngineOptions {
    /// Safety cap, not a shaper. The model decides when it is done.
    pub max_rounds: usize,
    /// `None` leaves the provider default (ZCode's request body omitted it).
    pub max_tokens: Option<u32>,
    /// `None` leaves the provider default (ZCode's request body omitted it).
    pub temperature: Option<f32>,
    /// Hard cost guard. This is a guardrail, not a writing rule.
    pub max_prompt_tokens: u64,
    /// Optional task-specific system prompt. `None` keeps the general-purpose
    /// agent prompt used by the note pipeline.
    pub system_prompt: Option<String>,
    /// Search provider used by the optional `search_web` tool. `None` hides the
    /// tool instead of showing one that always fails.
    pub search: Option<SearchConfig>,
}

impl Default for NoteEngineOptions {
    fn default() -> Self {
        Self {
            max_rounds: 100,
            max_tokens: None,
            temperature: None,
            max_prompt_tokens: 2_000_000,
            system_prompt: None,
            search: None,
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

fn tool_specs(include_search: bool) -> Vec<ToolSpec> {
    let mut tools = vec![
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
        ToolSpec {
            name: "bash".into(),
            description: "在工作区执行一条命令，返回 exit code、stdout 和 stderr。用它可以查找文件、跑脚本或做文本处理。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"command": {"type": "string", "description": "要执行的命令"}},
                "required": ["command"]
            }),
        },
        ToolSpec {
            name: "fetch_url".into(),
            description: "抓取一个公开网页的文本内容，返回 HTTP 状态和正文（截断）。只用于只读检索。".into(),
            parameters: json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"]
            }),
        },
    ];
    if include_search {
        tools.push(ToolSpec {
            name: "search_web".into(),
            description: "联网检索公开网页，返回标题、URL 和摘要。用于核实事实或补充资料；引用时保留 URL。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "检索词"},
                    "max": {"type": "integer", "minimum": 1, "maximum": 10}
                },
                "required": ["query"]
            }),
        });
    }
    tools
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

fn tool_write_file(root: &Path, args: &Value) -> String {
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
    base
}

fn tool_edit_file(root: &Path, args: &Value) -> String {
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
    base
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

fn tool_bash(root: &Path, args: &Value) -> String {
    let command = text_arg(args, "command");
    if command.trim().is_empty() {
        return "缺少 command".into();
    }
    #[cfg(windows)]
    let mut process = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", &command]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &command]);
        cmd
    };
    let output = match process.current_dir(root).output() {
        Ok(value) => value,
        Err(error) => return format!("命令启动失败：{error}"),
    };
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    let code = output.status.code().map(|value| value.to_string()).unwrap_or_else(|| "?".into());
    let clipped: String = text.chars().take(20_000).collect();
    format!("exit {code}
{clipped}")
}

fn tool_fetch_url(args: &Value) -> String {
    let url = text_arg(args, "url");
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return "url 必须以 http:// 或 https:// 开头".into();
    }
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(value) => value,
        Err(error) => return format!("HTTP 客户端失败：{error}"),
    };
    let response = match client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (compatible; yebian-notes/0.1)")
        .send()
    {
        Ok(value) => value,
        Err(error) => return format!("抓取失败：{error}"),
    };
    let status = response.status();
    let text = match response.text() {
        Ok(value) => value,
        Err(error) => return format!("读取网页失败：{error}"),
    };
    let clipped: String = text.chars().take(20_000).collect();
    format!("{status}
{clipped}")
}

fn execute_tool(root: &Path, call: &ToolCall) -> (String, Option<Value>) {
    let args = serde_json::from_str::<Value>(&call.function.arguments).ok();
    let Some(args) = args else {
        return ("工具参数不是合法 JSON".into(), None);
    };
    let observation = match call.function.name.as_str() {
        "read_file" => tool_read_file(root, &args),
        "write_file" => tool_write_file(root, &args),
        "edit_file" => tool_edit_file(root, &args),
        "list_dir" => tool_list_dir(root, &args),
        "bash" => tool_bash(root, &args),
        "fetch_url" => tool_fetch_url(&args),
        other => format!("未知工具：{other}"),
    };
    (observation, Some(args))
}

async fn execute_tool_async(
    root: &Path,
    call: &ToolCall,
    search: Option<&SearchConfig>,
    token: &CancellationToken,
) -> (String, Option<Value>) {
    if call.function.name != "search_web" {
        return execute_tool(root, call);
    }
    let args = serde_json::from_str::<Value>(&call.function.arguments).ok();
    let Some(args) = args else {
        return ("工具参数不是合法 JSON".into(), None);
    };
    let Some(config) = search else {
        return ("没有配置联网检索 Key；只抓取素材里的链接。".into(), Some(args));
    };
    let query = text_arg(&args, "query");
    if query.trim().is_empty() {
        return ("缺少 query".into(), Some(args));
    }
    let max = int_arg(&args, "max").unwrap_or(5).clamp(1, 10) as u32;
    match clients::search(config, query.trim(), max, token).await {
        Ok(hits) if hits.is_empty() => ("没有检索到结果".into(), Some(args)),
        Ok(hits) => {
            let mut out = String::new();
            for (index, hit) in hits.iter().enumerate() {
                out.push_str(&format!(
                    "{}. {}\n{}\n{}\n",
                    index + 1,
                    hit.title.trim(),
                    hit.url.trim(),
                    hit.content.trim().chars().take(500).collect::<String>()
                ));
            }
            (out, Some(args))
        }
        Err(error) => (format!("检索失败：{error}"), Some(args)),
    }
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
    let base_prompt = options.system_prompt.as_deref().unwrap_or(GENERAL_PURPOSE_PROMPT);
    let system = format!(
        "{base_prompt}

{CLAIMS_PROMPT}

## 输出格式
笔记是 Markdown。需要图表、卡片、时间线或对比时，写一个自包含的 ```html 代码块；App 会在正文列宽里原样渲染：笔记约 575px，任务弹窗约 680px，窄屏约 350px；按 575px 设计，用相对单位和自动换行，不要按 720px 画死。
可以用任意布局和强调色；中性色优先用 CSS 变量（--surface/--paper/--line/--ink/--ink-3），保证明暗主题都清楚。
不要 <script>、<html>/<body>、外链、外部图片、表单；图片用 inline SVG；文字不要溢出；高度自适应；不要为了配图而配图。
示例只示意写法，布局按内容决定：
```html
<div style='display:grid;gap:10px'>
  <div style='padding:12px;border:1px solid var(--line);border-radius:8px'>
    <b style='color:var(--ink)'>读取</b>
    <p style='margin:4px 0 0;color:var(--ink-3);font-size:13px'>先把素材读进来。</p>
  </div>
</div>
```"
    );
    let user = format!(
        "{task}\n\n工作区：{}\n素材文件：material.md\n输出文件：note.md",
        workspace.display()
    );
    let mut messages = vec![ChatMessage::system(system), ChatMessage::user(user)];
    let tools = tool_specs(options.search.is_some());
    let mut turns: Vec<NoteTurn> = Vec::new();
    let mut total = EngineUsage::default();
    let mut last_content = String::new();
    let mut rounds = 0usize;
    let mut finished = false;
    let mut final_note: Option<String> = None;

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
            final_note = Some(candidate);
            finished = true;
            break;
        }

        let calls = response.tool_calls.clone();
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
            let (observation, args) =
                execute_tool_async(&workspace, &call, options.search.as_ref(), token).await;
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
        );
        assert!(observation.contains("已写 note.md"), "{observation}");
        let (observation, _) = execute_tool(&root, &call("read_file", json!({"path": "note.md"})));
        assert!(observation.contains("# 标题"), "{observation}");
        let (observation, _) = execute_tool(
            &root,
            &call("edit_file", json!({"path": "note.md", "old": "正文", "new": "新正文"})),
        );
        assert!(observation.contains("已改 note.md"), "{observation}");
        let (observation, _) = execute_tool(&root, &call("list_dir", json!({"path": ""})));
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
        );
        assert!(observation.contains("出现 2 次"), "{observation}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bash_runs_inside_the_workspace() {
        let root = temp_workspace("bash");
        let (observation, _) = execute_tool(&root, &call("bash", json!({"command": "echo note-engine"})));
        assert!(observation.contains("note-engine"), "{observation}");
        let _ = std::fs::remove_dir_all(&root);
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
