use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::production::clients::{MIMO_MAX_AUDIO_BYTES, MIMO_SPLIT_BYTES};

pub fn job_temp_dir(root: &Path, job_id: &str) -> PathBuf {
    root.join(".yebian").join("production-tmp").join(job_id)
}

/// Split a long MiMo input into deterministic 10-minute parts. The ASR
/// pipeline checkpoints each part, so a failure late in the file does not
/// re-bill the earlier parts.
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
    let size = std::fs::metadata(source).map_err(|error| format!("音频打不开：{error}"))?.len();
    if size <= MIMO_SPLIT_BYTES {
        return Ok(vec![source.to_path_buf()]);
    }
    let dir = job_temp_dir(root, job_id).join("asr-parts").join(format!("item-{index}"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.map_err(|error| error.to_string())?;
    let ffmpeg = ffmpeg_path.trim().to_string();
    let ffmpeg = if ffmpeg.is_empty() {
        std::env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".into())
    } else {
        ffmpeg
    };
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
        .arg("600")
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
    let mut child = command.spawn().map_err(|error| format!("找不到 ffmpeg（{ffmpeg}）：{error}"))?;
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
            "ffmpeg 音频分段失败（exit {}，ffmpeg {ffmpeg}）：{}",
            status.code().map(|code| code.to_string()).unwrap_or_else(|| "terminated".into()),
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
    let ffmpeg = ffmpeg_path
        .trim()
        .to_string();
    let ffmpeg = if ffmpeg.is_empty() {
        std::env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".into())
    } else {
        ffmpeg
    };
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
    let mut child = command.spawn().map_err(|error| {
        format!("找不到 ffmpeg（{ffmpeg}）：{error}。本地视频/音频需要 ffmpeg，可在「设置 → 制作」里填写路径。")
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
            "ffmpeg 提取音频失败（exit {}，ffmpeg {ffmpeg}）：{}",
            status.code().map(|code| code.to_string()).unwrap_or_else(|| "terminated".into()),
            stderr.trim().chars().take(500).collect::<String>()
        ));
    }
    if !output.is_file() {
        return Err("ffmpeg 没有生成音频文件".into());
    }
    Ok(output)
}
