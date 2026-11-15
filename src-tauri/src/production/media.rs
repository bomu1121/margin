use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::store::ProductionStore;
use crate::production::clients::{MIMO_MAX_AUDIO_BYTES, MIMO_PART_SECONDS};

/// Windows GUI processes otherwise flash a console window for every child
/// console program. `ffmpeg -version` is called by the health check, so without
/// this flag the subscription maintenance loop pops a black window every tick.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const FFMPEG_CACHE_TTL: Duration = Duration::from_secs(300);

struct CachedFfmpeg {
    configured: String,
    checked_at: Instant,
    result: Result<PathBuf, String>,
}

fn ffmpeg_cache() -> &'static Mutex<Option<CachedFfmpeg>> {
    static CACHE: OnceLock<Mutex<Option<CachedFfmpeg>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn job_temp_dir(root: &Path, job_id: &str) -> PathBuf {
    root.join(".yebian").join("production-tmp").join(job_id)
}

/// Resolve ffmpeg for this machine.
///
/// A configured path is authoritative only while it still works. It used to be
/// synced between devices, so a path from another Windows account could be
/// copied here and every subscription job failed with "找不到 ffmpeg". The
/// resolved path now falls back to FFMPEG_PATH, PATH and the common install
/// roots, and the caller persists the repaired path.
pub fn resolve_ffmpeg(configured: &str) -> Result<PathBuf, String> {
    let configured = configured.trim().trim_matches('"').trim().to_string();
    if let Ok(cache) = ffmpeg_cache().lock() {
        if let Some(cached) = cache.as_ref() {
            if cached.configured == configured && cached.checked_at.elapsed() < FFMPEG_CACHE_TTL {
                return cached.result.clone();
            }
        }
    }
    let result = resolve_ffmpeg_uncached(&configured);
    if let Ok(mut cache) = ffmpeg_cache().lock() {
        *cache = Some(CachedFfmpeg {
            configured,
            checked_at: Instant::now(),
            result: result.clone(),
        });
    }
    result
}

fn resolve_ffmpeg_uncached(configured: &str) -> Result<PathBuf, String> {
    let candidates = ffmpeg_candidates(configured);
    for candidate in &candidates {
        if usable_ffmpeg(candidate) {
            return Ok(candidate.clone());
        }
    }
    Err(format!(
        "找不到可用的 ffmpeg（已检查 {} 个位置：设置里的路径、FFMPEG_PATH、PATH 和常见安装目录）。请安装 ffmpeg，或在「设置 → 制作」填写 ffmpeg.exe 的完整路径。",
        candidates.len()
    ))
}

/// Resolve ffmpeg and repair a stale device-local setting in place.
pub fn resolve_and_store(store: &ProductionStore) -> Result<String, String> {
    let configured = store
        .get_setting("production.ffmpeg_path")?
        .unwrap_or_default();
    let path = resolve_ffmpeg(&configured)?;
    let resolved = path.to_string_lossy().to_string();
    let configured = configured.trim();
    if configured.is_empty() || !same_path(configured, &resolved) {
        let _ = store.set_setting("production.ffmpeg_path", &resolved);
        if !configured.is_empty() {
            log::warn!("ffmpeg 路径已自动修正：{configured} -> {resolved}");
        }
    }
    Ok(resolved)
}

fn same_path(left: &str, right: &str) -> bool {
    let left = left.replace('\\', "/");
    let right = right.replace('\\', "/");
    left.trim_end_matches('/').eq_ignore_ascii_case(right.trim_end_matches('/'))
}

fn usable_ffmpeg(candidate: &Path) -> bool {
    let looks_like_path = candidate.is_absolute() || candidate.components().count() > 1;
    if looks_like_path && !candidate.is_file() {
        return false;
    }
    let mut command = StdCommand::new(candidate);
    command
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output();
    let Ok(output) = output else { return false };
    if !output.status.success() {
        return false;
    }
    let mut text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    text.push_str(&String::from_utf8_lossy(&output.stderr).to_ascii_lowercase());
    text.contains("ffmpeg version")
}

fn ffmpeg_candidates(configured: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let configured = configured.trim().trim_matches('"').trim();
    if !configured.is_empty() {
        add_candidate(&mut out, &mut seen, PathBuf::from(configured));
    }
    if let Ok(value) = std::env::var("FFMPEG_PATH") {
        if !value.trim().is_empty() {
            add_candidate(&mut out, &mut seen, PathBuf::from(value.trim().trim_matches('"')));
        }
    }
    add_candidate(&mut out, &mut seen, PathBuf::from("ffmpeg"));
    #[cfg(windows)]
    {
        add_candidate(&mut out, &mut seen, PathBuf::from("ffmpeg.exe"));
        let mut roots = Vec::new();
        for key in ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMDATA", "USERPROFILE"] {
            if let Ok(value) = std::env::var(key) {
                if !value.trim().is_empty() {
                    roots.push(PathBuf::from(value));
                }
            }
        }
        for root in &roots {
            let winget = root.join("Microsoft/WinGet/Packages");
            collect_ffmpeg_under(&winget, 5, &mut out, &mut seen);
            collect_ffmpeg_under(&root.join("ffmpeg"), 4, &mut out, &mut seen);
            add_candidate(&mut out, &mut seen, root.join("ffmpeg/bin/ffmpeg.exe"));
            add_candidate(&mut out, &mut seen, root.join("scoop/shims/ffmpeg.exe"));
        }
        add_candidate(&mut out, &mut seen, PathBuf::from("C:/ffmpeg/bin/ffmpeg.exe"));
        collect_ffmpeg_under(Path::new("C:/ffmpeg"), 4, &mut out, &mut seen);
        add_candidate(
            &mut out,
            &mut seen,
            PathBuf::from("C:/ProgramData/chocolatey/bin/ffmpeg.exe"),
        );
    }
    #[cfg(not(windows))]
    {
        for path in ["/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/opt/homebrew/bin/ffmpeg"] {
            add_candidate(&mut out, &mut seen, PathBuf::from(path));
        }
    }
    out
}

fn add_candidate(out: &mut Vec<PathBuf>, seen: &mut HashSet<String>, path: PathBuf) {
    let key = path.to_string_lossy().to_ascii_lowercase();
    if seen.insert(key) {
        out.push(path);
    }
}

#[cfg(windows)]
fn collect_ffmpeg_under(
    root: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
    seen: &mut HashSet<String>,
) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
            if name.eq_ignore_ascii_case("ffmpeg.exe") {
                add_candidate(out, seen, path);
            }
        } else if path.is_dir() {
            collect_ffmpeg_under(&path, depth - 1, out, seen);
        }
    }
}

/// Split MiMo input into deterministic 60-second parts. MiMo has an 8K
/// context / 2K output window and drops middle content on long single passes;
/// the ASR pipeline checkpoints each part, so a failure late in the file does
/// not re-bill the earlier parts.
pub async fn prepare_asr_parts(
    source: &Path,
    job_id: &str,
    root: &Path,
    index: usize,
    token: &CancellationToken,
    engine: &str,
    ffmpeg_path: &str,
) -> Result<Vec<PathBuf>, String> {
    if engine != "mimo" {
        return Ok(vec![source.to_path_buf()]);
    }
    let dir = job_temp_dir(root, job_id).join("asr-parts").join(format!("item-{index}"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.map_err(|error| error.to_string())?;
    let ffmpeg = resolve_ffmpeg(ffmpeg_path)?;
    let pattern = dir.join("part-%03d.mp3");
    let mut command = Command::new(&ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-vn")
        .arg("-f")
        .arg("segment")
        .arg("-segment_time")
        .arg(MIMO_PART_SECONDS.to_string())
        .arg("-reset_timestamps")
        .arg("1")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("48k")
        .arg(&pattern)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 ffmpeg（{}）：{error}", ffmpeg.display()))?;
    let stderr_pipe = child.stderr.take();
    let status = tokio::select! {
        _ = token.cancelled() => {
            let _ = child.start_kill();
            return Err("已取消".into());
        }
        status = child.wait() => status.map_err(|error| error.to_string())?,
    };
    let mut stderr = Vec::new();
    if let Some(mut pipe) = stderr_pipe {
        use tokio::io::AsyncReadExt;
        let _ = pipe.read_to_end(&mut stderr).await;
    }
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        return Err(format!(
            "ffmpeg 音频分段失败（exit {}，ffmpeg {}）：{}",
            status.code().map(|code| code.to_string()).unwrap_or_else(|| "terminated".into()),
            ffmpeg.display(),
            stderr.trim().chars().take(300).collect::<String>()
        ));
    }
    let mut parts = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir).await.map_err(|error| error.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|error| error.to_string())? {
        let path = entry.path();
        if path.extension().map(|value| value.to_string_lossy() == "mp3").unwrap_or(false) {
            parts.push(path);
        }
    }
    parts.sort();
    if parts.is_empty() {
        return Err("ffmpeg 没有生成可转写的音频分段".into());
    }
    Ok(parts)
}

pub async fn prepare_audio(
    source: &Path,
    job_id: &str,
    root: &Path,
    index: usize,
    token: &CancellationToken,
    engine: &str,
    ffmpeg_path: &str,
) -> Result<PathBuf, String> {
    let extension = source
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mimo = engine == "mimo";
    let supported = if mimo {
        matches!(extension.as_str(), "mp3" | "wav")
    } else {
        matches!(extension.as_str(), "mp3" | "m4a" | "wav" | "aac" | "ogg" | "flac")
    };
    if supported {
        let size_ok = !mimo
            || std::fs::metadata(source)
                .map(|metadata| metadata.len() <= MIMO_MAX_AUDIO_BYTES as u64)
                .unwrap_or(false);
        if size_ok {
            return Ok(source.to_path_buf());
        }
    }
    let output = if mimo {
        job_temp_dir(root, job_id).join(format!("audio-{index}.mp3"))
    } else {
        job_temp_dir(root, job_id).join(format!("audio-{index}.wav"))
    };
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| error.to_string())?;
    }
    let ffmpeg = resolve_ffmpeg(ffmpeg_path)?;
    let mut command = Command::new(&ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000");
    if mimo {
        command.arg("-c:a").arg("libmp3lame").arg("-b:a").arg("48k");
    } else {
        command.arg("-c:a").arg("pcm_s16le");
    }
    command
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|error| {
        format!(
            "无法启动 ffmpeg（{}）：{error}。本地视频/音频需要 ffmpeg，可在「设置 → 制作」填写路径。",
            ffmpeg.display()
        )
    })?;
    let stderr_pipe = child.stderr.take();
    let status = tokio::select! {
        _ = token.cancelled() => {
            let _ = child.start_kill();
            return Err("已取消".into());
        }
        status = child.wait() => status.map_err(|error| error.to_string())?,
    };
    let mut stderr = Vec::new();
    if let Some(mut pipe) = stderr_pipe {
        use tokio::io::AsyncReadExt;
        let _ = pipe.read_to_end(&mut stderr).await;
    }
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        return Err(format!(
            "ffmpeg 提取音频失败（exit {}，ffmpeg {}）：{}",
            status.code().map(|code| code.to_string()).unwrap_or_else(|| "terminated".into()),
            ffmpeg.display(),
            stderr.trim().chars().take(500).collect::<String>()
        ));
    }
    if !output.is_file() {
        return Err("ffmpeg 没有生成音频文件".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_configured_path_falls_back_to_a_working_ffmpeg() {
        let configured = r"Z:\yebian-definitely-missing\ffmpeg.exe";
        match resolve_ffmpeg("") {
            Ok(_) => {
                let resolved = resolve_ffmpeg(configured).expect("fallback to PATH/install root");
                assert!(usable_ffmpeg(&resolved), "resolved path must execute: {resolved:?}");
            }
            Err(_) => {
                // CI machines without ffmpeg should fail with an actionable
                // error instead of silently accepting the stale path.
                assert!(resolve_ffmpeg(configured).is_err());
            }
        }
    }

    #[test]
    fn candidates_include_path_lookup_and_deduplicate() {
        let candidates = ffmpeg_candidates(r"C:\ffmpeg\bin\ffmpeg.exe");
        assert_eq!(candidates[0], PathBuf::from(r"C:\ffmpeg\bin\ffmpeg.exe"));
        assert!(
            candidates.iter().any(|path| path == Path::new("ffmpeg")),
            "PATH lookup must always be a candidate"
        );
        let keys = candidates
            .iter()
            .map(|path| path.to_string_lossy().to_ascii_lowercase())
            .collect::<HashSet<_>>();
        assert_eq!(keys.len(), candidates.len(), "candidate list must be unique");
    }
}
