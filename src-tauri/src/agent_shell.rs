// Scoped file/shell tools for the in-app agent. Every path must stay inside
// the current vault; writes keep a backup; shell commands are allowlisted and
// metacharacters are rejected (no pipes, redirection or command chaining).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

const MAX_READ_BYTES: u64 = 512 * 1024;
const MAX_OUTPUT: usize = 32 * 1024;
const SHELL_TIMEOUT: Duration = Duration::from_secs(60);

fn display_path(path: &Path) -> String {
    path.to_string_lossy().trim_start_matches("\\\\?\\").to_string()
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(root).map_err(|error| format!("笔记库不可用：{error}"))
}

fn resolve_read(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = canonical_root(root)?;
    let candidate = if Path::new(rel).is_absolute() { PathBuf::from(rel) } else { root.join(rel) };
    let path = std::fs::canonicalize(&candidate).map_err(|error| format!("文件打不开：{error}"))?;
    if !path.starts_with(&root) {
        return Err("只允许访问当前笔记库".into());
    }
    Ok(path)
}

fn resolve_write(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = canonical_root(root)?;
    let candidate = if Path::new(rel).is_absolute() { PathBuf::from(rel) } else { root.join(rel) };
    let parent = candidate.parent().ok_or_else(|| "路径无效".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let parent = std::fs::canonicalize(parent).map_err(|error| error.to_string())?;
    if !parent.starts_with(&root) {
        return Err("只允许写入当前笔记库".into());
    }
    let name = candidate.file_name().ok_or_else(|| "缺少文件名".to_string())?;
    Ok(parent.join(name))
}

fn allowed_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),
        "md" | "json" | "mjs" | "js" | "txt" | "csv" | "html" | "svg"
    )
}

fn backup(root: &Path, path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let dir = root.join(".yebian").join("backups");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0);
    let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("file");
    std::fs::copy(path, dir.join(format!("{stamp}-{name}"))).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn read_file(root: &Path, rel: &str) -> Result<Value, String> {
    let path = resolve_read(root, rel)?;
    let meta = std::fs::metadata(&path).map_err(|error| error.to_string())?;
    if meta.len() > MAX_READ_BYTES {
        return Err(format!("文件 {:.0} KB，超过 512 KB 读取上限", meta.len() as f64 / 1024.0));
    }
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    Ok(json!({
        "path": display_path(&path),
        "bytes": bytes.len(),
        "text": String::from_utf8_lossy(&bytes).into_owned(),
    }))
}

pub fn write_file(root: &Path, rel: &str, content: &str) -> Result<Value, String> {
    let path = resolve_write(root, rel)?;
    if !allowed_extension(&path) {
        return Err("只允许写 md/json/mjs/js/txt/csv/html/svg".into());
    }
    backup(root, &path)?;
    std::fs::write(&path, content.as_bytes()).map_err(|error| error.to_string())?;
    Ok(json!({ "path": display_path(&path), "bytes": content.len() }))
}

pub fn edit_file(root: &Path, rel: &str, old: &str, new: &str) -> Result<Value, String> {
    if old.trim().is_empty() {
        return Err("old 不能为空".into());
    }
    let path = resolve_read(root, rel)?;
    if !allowed_extension(&path) {
        return Err("只允许编辑文本类文件".into());
    }
    let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let count = text.matches(old).count();
    if count == 0 {
        return Err("没有找到 old 片段".into());
    }
    if count > 1 {
        return Err(format!("old 片段出现 {count} 次，不唯一；请提供更长的上下文"));
    }
    backup(root, &path)?;
    let next = text.replacen(old, new, 1);
    std::fs::write(&path, next.as_bytes()).map_err(|error| error.to_string())?;
    Ok(json!({ "path": display_path(&path), "bytes": next.len() }))
}

pub fn list_dir(root: &Path, rel: &str) -> Result<Value, String> {
    let path = if rel.trim().is_empty() { canonical_root(root)? } else { resolve_read(root, rel)? };
    if !path.is_dir() {
        return Err("不是目录".into());
    }
    let mut items = Vec::new();
    for entry in std::fs::read_dir(&path).map_err(|error| error.to_string())?.flatten() {
        let meta = entry.metadata().ok();
        let name = entry.file_name().to_string_lossy().into_owned();
        items.push(json!({
            "name": name,
            "dir": meta.as_ref().map(|value| value.is_dir()).unwrap_or(false),
            "bytes": meta.as_ref().map(|value| value.len()).unwrap_or(0),
        }));
    }
    items.sort_by(|a, b| {
        let ad = a.get("dir").and_then(Value::as_bool).unwrap_or(false);
        let bd = b.get("dir").and_then(Value::as_bool).unwrap_or(false);
        bd.cmp(&ad).then_with(|| {
            a.get("name").and_then(Value::as_str).unwrap_or("").cmp(b.get("name").and_then(Value::as_str).unwrap_or(""))
        })
    });
    items.truncate(200);
    Ok(json!({ "path": display_path(&path), "items": items }))
}

fn reject_metacharacters(command: &str) -> Result<(), String> {
    for token in [";", "&", "|", ">", "<", "$", "`", "\n", "\r"] {
        if command.contains(token) {
            return Err(format!("命令里不允许出现 {token}"));
        }
    }
    Ok(())
}

fn check_program(root: &Path, program: &str, args: &[String]) -> Result<(), String> {
    match program {
        "node" => {
            let script = args.first().ok_or_else(|| "node 需要脚本路径".to_string())?;
            let path = resolve_read(root, script)?;
            if !matches!(path.extension().and_then(|value| value.to_str()), Some("mjs") | Some("js")) {
                return Err("node 只能运行 .mjs/.js 脚本".into());
            }
            Ok(())
        }
        "git" => match args.first().map(String::as_str) {
            Some("status") | Some("diff") | Some("log") | Some("show") => Ok(()),
            _ => Err("git 只允许 status/diff/log/show".into()),
        },
        "ls" | "cat" | "rg" | "grep" | "wc" | "head" | "tail" | "dir" => Ok(()),
        other => Err(format!("命令不在白名单：{other}")),
    }
}

/// Minimal tokenizer: splits on spaces, honours single/double quotes, and does
/// not expand anything. `rg -g "*.md"` keeps `*.md` as one argument.
fn tokenize(command: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        match ch {
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
            ' ' | '\t' if quote.is_none() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err("引号没有闭合".into());
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

/// Run a allowlisted command with the vault as cwd. Returns stdout/stderr and
/// the exit code; never shell-expanded.
pub async fn run_bash(root: &Path, cwd_rel: &str, command: &str) -> Result<Value, String> {
    reject_metacharacters(command)?;
    let cwd = if cwd_rel.trim().is_empty() { canonical_root(root)? } else { resolve_read(root, cwd_rel)? };
    if !cwd.is_dir() {
        return Err("cwd 不是目录".into());
    }
    let parts = tokenize(command)?;
    if parts.is_empty() {
        return Err("命令为空".into());
    }
    check_program(root, &parts[0], &parts[1..])?;
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();

    let mut cmd = tokio::process::Command::new(parts[0]);
    cmd.args(&parts[1..]).current_dir(&cwd).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let child = cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
        .map_err(|error| format!("无法启动命令：{error}"))?;
    let output = tokio::time::timeout(SHELL_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| format!("命令超时（{}s）", SHELL_TIMEOUT.as_secs()))?
        .map_err(|error| error.to_string())?;
    let mut stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    stdout.truncate(MAX_OUTPUT);
    stderr.truncate(MAX_OUTPUT);
    Ok(json!({
        "code": output.status.code().unwrap_or(-1),
        "stdout": stdout,
        "stderr": stderr,
    }))
}
