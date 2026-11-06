use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::production::clients::MIMO_MAX_AUDIO_BYTES;
use crate::production::store::ProductionStore;

pub fn job_temp_dir(root: &Path, job_id: &str) -> PathBuf {
    root.join(".yebian").join("production-tmp").join(job_id)
}

pub async fn prepare_audio(
    store: &ProductionStore,
    source: &Path,
    job_id: &str,
    root: &Path,
    index: usize,
    token: &CancellationToken,
) -> Result<PathBuf, String> {
    let extension = source
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mimo = store.settings().asr_engine == "mimo";
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
    let ffmpeg = store
        .get_setting("production.ffmpeg_path")
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("FFMPEG_PATH").ok().filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| "ffmpeg".into());
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
    let mut stderr = String::new();
    if let Some(mut pipe) = stderr_pipe {
        use tokio::io::AsyncReadExt;
        let _ = pipe.read_to_string(&mut stderr).await;
    }
    if !status.success() {
        return Err(format!("ffmpeg 提取音频失败：{}", stderr.trim().chars().take(500).collect::<String>()));
    }
    if !output.is_file() {
        return Err("ffmpeg 没有生成音频文件".into());
    }
    Ok(output)
}
