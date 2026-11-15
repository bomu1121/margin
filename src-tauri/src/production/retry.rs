//! Failure classification and durable retry timing for automatic jobs.
//!
//! Automatic production is at-least-once: the note publisher is idempotent
//! (stable note id), so a retry may safely re-run. The retry decision is stored
//! in SQLite (`retry_count`, `next_retry_at`) rather than in a timer, which
//! means it survives an app restart.

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobErrorKind {
    /// Network reset, timeout, 429/5xx, truncated ASR, empty model reply.
    Transient,
    /// ffmpeg missing, API key missing, budget exceeded.
    Dependency,
    /// Bilibili login expired or provider rejected the credential.
    Auth,
    /// Bilibili risk control (412/-352); back off longer than a normal error.
    Risk,
    /// Bad source / unsupported video; retry slowly instead of hot-looping.
    Permanent,
    Cancelled,
}

impl JobErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::Dependency => "dependency",
            Self::Auth => "auth",
            Self::Risk => "risk",
            Self::Permanent => "permanent",
            Self::Cancelled => "cancelled",
        }
    }

    /// Status written to the durable seen-video ledger.
    pub fn seen_status(self) -> &'static str {
        match self {
            Self::Dependency | Self::Auth | Self::Risk => "blocked",
            Self::Cancelled => "cancelled",
            Self::Transient | Self::Permanent => "failed",
        }
    }

    pub fn retryable(self) -> bool {
        !matches!(self, Self::Cancelled)
    }
}

pub fn classify(error: &str) -> JobErrorKind {
    let lower = error.to_ascii_lowercase();
    if error.contains("已取消") || lower.contains("cancelled") {
        return JobErrorKind::Cancelled;
    }
    if error.contains("还没有配置 AI API Key")
        || error.contains("还没有配置语音识别 API Key")
        || error.contains("还没有配置联网检索 Key")
        || error.contains("B站还没有登录")
        || error.contains("登录已过期")
        || error.contains("扫码")
        || error.contains("Key 无效")
        || lower.contains("401")
        || lower.contains("403")
    {
        return JobErrorKind::Auth;
    }
    if error.contains("找不到可用的 ffmpeg")
        || error.contains("找不到 ffmpeg")
        || error.contains("无法启动 ffmpeg")
        || error.contains("需要 ffmpeg")
        || error.contains("预算已用完")
        || error.contains("本月预算")
        || error.contains("预算用完")
    {
        return JobErrorKind::Dependency;
    }
    if error.contains("ffmpeg 音频分段失败")
        || error.contains("ffmpeg 提取音频失败")
        || error.contains("ffmpeg 没有生成")
    {
        return JobErrorKind::Permanent;
    }
    if error.contains("412") || error.contains("风控") || error.contains("-352") {
        return JobErrorKind::Risk;
    }
    if error.contains("没有可用分P")
        || error.contains("没有可用的分P")
        || error.contains("文件不存在")
        || error.contains("不支持的来源")
        || error.contains("未知模板")
        || error.contains("模板不存在")
        || error.contains("模板「")
        || error.contains("视频不存在")
        || error.contains("已删除")
    {
        return JobErrorKind::Permanent;
    }
    JobErrorKind::Transient
}

/// Capped exponential backoff with +/-20% jitter. Dependency/auth failures are
/// re-probed on a short cadence because the fix is usually a setting change;
/// transient failures grow to a 24h cap so a poison video cannot hot-loop.
pub fn delay_ms(kind: JobErrorKind, attempts: u32) -> i64 {
    let base = match kind {
        JobErrorKind::Cancelled => return 0,
        JobErrorKind::Dependency => 10 * 60_000,
        JobErrorKind::Auth => 15 * 60_000,
        JobErrorKind::Risk => 2 * 3600_000,
        JobErrorKind::Permanent => 6 * 3600_000,
        JobErrorKind::Transient => {
            const SCHEDULE: &[i64] = &[
                60_000,
                5 * 60_000,
                15 * 60_000,
                3600_000,
                3 * 3600_000,
                6 * 3600_000,
                12 * 3600_000,
                24 * 3600_000,
            ];
            let index = attempts.saturating_sub(1) as usize;
            SCHEDULE[index.min(SCHEDULE.len() - 1)]
        }
    };
    jitter(base)
}

fn jitter(base: i64) -> i64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as i64)
        .unwrap_or(0);
    let factor = 0.8 + (nanos % 4_001) as f64 / 10_000.0;
    ((base as f64) * factor).max(1_000.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_actionable_failures() {
        assert_eq!(classify("找不到可用的 ffmpeg（已检查 12 个位置）"), JobErrorKind::Dependency);
        assert_eq!(classify("ffmpeg 提取音频失败（exit 1）"), JobErrorKind::Permanent);
        assert_eq!(classify("还没有配置 AI API Key"), JobErrorKind::Auth);
        assert_eq!(classify("B站登录已过期，请重新扫码"), JobErrorKind::Auth);
        assert_eq!(classify("B站风控拦截（HTTP 412）"), JobErrorKind::Risk);
        assert_eq!(classify("B站空间请求失败：请求超时"), JobErrorKind::Transient);
        assert_eq!(classify("这个视频没有可用的分P"), JobErrorKind::Permanent);
        assert_eq!(classify("任务模板不存在：old-template"), JobErrorKind::Permanent);
        assert_eq!(classify("已取消"), JobErrorKind::Cancelled);
    }

    #[test]
    fn transient_backoff_grows_and_is_capped() {
        let mut previous = 0;
        for attempt in 1..=8 {
            let delay = delay_ms(JobErrorKind::Transient, attempt);
            assert!(delay >= 48_000, "attempt {attempt} delay too small: {delay}");
            assert!(delay <= 29 * 3600_000, "attempt {attempt} delay too large: {delay}");
            if attempt >= 2 {
                assert!(delay > previous / 2, "backoff should not collapse at {attempt}");
            }
            previous = delay;
        }
        assert!(delay_ms(JobErrorKind::Dependency, 1) >= 8 * 60_000);
        assert!(delay_ms(JobErrorKind::Dependency, 99) <= 12 * 60_000);
        assert_eq!(delay_ms(JobErrorKind::Cancelled, 3), 0);
    }

    #[test]
    fn dependency_and_risk_are_blocked_not_failed() {
        assert_eq!(JobErrorKind::Dependency.seen_status(), "blocked");
        assert_eq!(JobErrorKind::Auth.seen_status(), "blocked");
        assert_eq!(JobErrorKind::Risk.seen_status(), "blocked");
        assert_eq!(JobErrorKind::Transient.seen_status(), "failed");
        assert!(!JobErrorKind::Cancelled.retryable());
    }
}
