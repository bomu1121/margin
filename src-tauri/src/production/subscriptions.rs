//! UP 主更新检查与自动制作。
//!
//! 设计约束（见 docs/SUBSCRIPTION_DESIGN.md）：
//! - 先建立基线，再把基线之后的新视频排成任务；
//! - 同一笔记库 + 同一 mid + 同一 bvid 只处理一次；
//! - 自动任务进入 `ScribeFlow/待整理/`，最终目录由用户自己移动；
//! - 串行检查、请求间隔至少 2 秒、遇到 412/-352 暂停，避免把账号打进风控。

use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use super::clients;
use super::store::{now_ms, ProductionStore};
use super::{
    BiliVideoRef, JobOptions, ProductionService, SourceSpec, StartJobRequest, Subscription,
    SubscriptionCheckResult, SubscriptionFilters, SubscriptionInput,
};

const MAX_BACKFILL: u32 = 10;
const RISK_COOLDOWN_SECS: i64 = 2 * 3600;
const HTTP_412_COOLDOWN_SECS: i64 = 6 * 3600;
const AUTH_COOLDOWN_SECS: i64 = 6 * 3600;

static CHECK_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static RISK_UNTIL_MS: AtomicI64 = AtomicI64::new(0);

fn check_lock() -> &'static tokio::sync::Mutex<()> {
    CHECK_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn jitter_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.subsec_nanos() as i64) % 180_000)
        .unwrap_or(0)
}

fn next_check_at(now: i64, minutes: u32) -> i64 {
    now + (minutes.max(10) as i64) * 60_000 + jitter_ms()
}

fn canonical_video_url(bvid: &str) -> String {
    format!("https://www.bilibili.com/video/{bvid}")
}

fn is_auth_error(error: &str) -> bool {
    error.contains("登录") && (error.contains("过期") || error.contains("扫码"))
}

fn is_risk_error(error: &str) -> bool {
    error.contains("风控") || error.contains("412") || error.contains("code -352")
}

fn is_transient_network_error(error: &str) -> bool {
    error.contains("连接失败")
        || error.contains("请求超时")
        || error.contains("网络请求失败")
        || error.contains("响应读取失败")
        || error.contains("error sending request")
}

fn failure_delay(error: &str, failures: u32, interval_minutes: u32) -> i64 {
    let now = now_ms();
    if error.contains("412") {
        return now + HTTP_412_COOLDOWN_SECS * 1000;
    }
    if is_risk_error(error) {
        return now + RISK_COOLDOWN_SECS * 1000;
    }
    if is_auth_error(error) {
        return now + AUTH_COOLDOWN_SECS * 1000;
    }
    if is_transient_network_error(error) {
        // A single connect reset should not park the UP for an hour.
        let base: i64 = 2 * 60_000;
        let exponent = failures.saturating_sub(1).min(4);
        return now + (base.saturating_mul(1i64 << exponent)).min(60 * 60 * 1000);
    }
    let base = (interval_minutes.max(10) as i64) * 60_000;
    let exponent = failures.min(4) as u32;
    now + (base.saturating_mul(1i64 << exponent)).min(6 * 3600 * 1000)
}

fn cookies(service: &ProductionService) -> Result<String, String> {
    let cookies = service
        .store()
        .get_setting("production.bili.cookies")?
        .unwrap_or_default();
    if cookies.trim().is_empty() {
        return Err("B站还没有登录；登录后自动检查才会开始".into());
    }
    Ok(cookies)
}

fn auto_options(sub: &Subscription, service: &ProductionService) -> JobOptions {
    let settings = service.settings();
    let mut options = JobOptions::default();
    options.prompt = sub.prompt.clone();
    options.fields = if settings.default_fields.is_empty() {
        vec!["人物".into(), "事件".into()]
    } else {
        settings.default_fields
    };
    // 自动身份看 job.auto / subscription_id；不再给笔记加「自动制作」标签。
    options.tags = Vec::new();
    options.auto = true;
    options.subscription_id = sub.id.clone();
    options
}

fn video_source(video: &BiliVideoRef) -> SourceSpec {
    SourceSpec {
        kind: "bili".into(),
        url: canonical_video_url(&video.bvid),
        videos: vec![video.clone()],
        ..SourceSpec::default()
    }
}

fn record_failure(service: &ProductionService, sub: &Subscription, error: &str) -> String {
    let failures = sub.consecutive_failures.saturating_add(1);
    if error.contains("412") {
        RISK_UNTIL_MS.store(now_ms() + HTTP_412_COOLDOWN_SECS * 1000, Ordering::Relaxed);
    } else if is_risk_error(error) {
        RISK_UNTIL_MS.store(now_ms() + RISK_COOLDOWN_SECS * 1000, Ordering::Relaxed);
    }
    let next = failure_delay(error, failures, sub.interval_minutes);
    let _ = service.store().record_subscription_check(
        &sub.id,
        now_ms(),
        next,
        error,
        "",
        failures,
    );
    error.to_string()
}

/// 把页面上未处理过的视频转成自动任务。返回 (新任务, 跳过已存在, 跳过过滤, 剩余)。
fn process_candidates(
    service: &ProductionService,
    root: &Path,
    sub: &Subscription,
    videos: &[BiliVideoRef],
    limit: usize,
    skip_before_ms: i64,
) -> (usize, usize, usize, usize) {
    let mut new_tasks = 0usize;
    let mut skipped_existing = 0usize;
    let mut skipped_filter = 0usize;
    let mut remaining = 0usize;
    let store = service.store();
    let vault = root.to_string_lossy().to_string();
    for video in videos {
        if store.seen_exists(&vault, sub.mid, &video.bvid).unwrap_or(false) {
            continue;
        }
        if skip_before_ms > 0 && video.pubdate > 0 && video.pubdate * 1000 < skip_before_ms {
            let _ = store.insert_seen(&vault, sub.mid, video, "baseline", "", "");
            continue;
        }
        if !sub.filters.allows(&video.title, video.duration) {
            let _ = store.insert_seen(&vault, sub.mid, video, "skipped_filter", "", "");
            skipped_filter += 1;
            continue;
        }
        let source = video_source(video);
        let note_id = ProductionStore::note_id_for(&sub.template_id, &source);
        let exists = crate::artifact::registry_for_note(root, &note_id)
            .map(|item| !item.note_rel_path.is_empty() && root.join(&item.note_rel_path).is_file())
            .unwrap_or(false);
        if exists {
            let _ = store.insert_seen(&vault, sub.mid, video, "skipped_existing", "", &note_id);
            skipped_existing += 1;
            continue;
        }
        if new_tasks >= limit {
            remaining += 1;
            continue;
        }
        // 先登记 seen，再启动任务：这样下一次检查不会在同一视频上重复排任务。
        let _ = store.insert_seen(&vault, sub.mid, video, "queued", "", &note_id);
        let request = StartJobRequest {
            template_id: sub.template_id.clone(),
            title: video.title.clone(),
            source,
            options: auto_options(sub, service),
        };
        match service.start(root, request) {
            Ok(job) => {
                let _ = store.mark_seen(&vault, sub.mid, &video.bvid, "queued", &job.id, &note_id);
                new_tasks += 1;
            }
            Err(error) => {
                let _ = store.mark_seen(&vault, sub.mid, &video.bvid, "failed", "", &note_id);
                log::warn!("auto production start failed for {}: {error}", video.bvid);
            }
        }
    }
    (new_tasks, skipped_existing, skipped_filter, remaining)
}

fn baseline_candidates(videos: &[BiliVideoRef], backfill: u32) -> Vec<BiliVideoRef> {
    let mut sorted = videos.to_vec();
    sorted.sort_by(|a, b| b.pubdate.cmp(&a.pubdate));
    sorted.truncate(backfill.min(MAX_BACKFILL) as usize);
    sorted
}

async fn baseline(
    service: &ProductionService,
    root: &Path,
    sub: &Subscription,
    videos: &[BiliVideoRef],
    backfill: u32,
) -> Result<SubscriptionCheckResult, String> {
    let vault = root.to_string_lossy().to_string();
    let store = service.store();
    let now = now_ms();
    let selected = baseline_candidates(videos, backfill);
    let selected_ids = selected.iter().map(|video| video.bvid.as_str()).collect::<std::collections::HashSet<_>>();
    for video in videos {
        if selected_ids.contains(video.bvid.as_str()) {
            continue;
        }
        let _ = store.insert_seen(&vault, sub.mid, video, "baseline", "", "");
    }
    store.set_subscription_baseline(&sub.id, now)?;
    let auto = service.auto_settings();
    let used = store.queued_auto_today(&vault).unwrap_or(0);
    let daily_remaining = auto.max_per_day.saturating_sub(used) as usize;
    let limit = selected.len().min(daily_remaining).min(MAX_BACKFILL as usize);
    let (new_tasks, skipped_existing, skipped_filter, remaining) =
        process_candidates(service, root, sub, &selected, limit, 0);
    // 基线补跑受每日上限约束；没轮到的视频记为基线，不能等到“下次”再偷偷补跑。
    for video in &selected {
        if !store.seen_exists(&vault, sub.mid, &video.bvid).unwrap_or(false) {
            let _ = store.insert_seen(&vault, sub.mid, video, "baseline", "", "");
        }
    }
    let message = if backfill == 0 {
        format!("已建立基线（{} 条现有视频不会补跑）", videos.len())
    } else {
        let extra = if remaining > 0 {
            format!("；还有 {remaining} 条因每日上限未补跑")
        } else {
            String::new()
        };
        format!(
            "已建立基线；补跑 {new_tasks} 条（跳过已存在 {skipped_existing}，过滤 {skipped_filter}）{extra}"
        )
    };
    let next = next_check_at(now, sub.interval_minutes);
    store.record_subscription_check(&sub.id, now, next, "", &message, 0)?;
    Ok(SubscriptionCheckResult {
        checked_at: now,
        baseline: true,
        new_tasks,
        skipped_existing,
        skipped_filter,
        remaining,
        message,
    })
}

async fn check_locked(
    service: &ProductionService,
    root: &Path,
    sub: &Subscription,
    backfill: u32,
) -> Result<SubscriptionCheckResult, String> {
    let now = now_ms();
    let risk_until = RISK_UNTIL_MS.load(Ordering::Relaxed);
    if risk_until > now {
        let minutes = ((risk_until - now) / 60_000).max(1);
        return Err(format!("B站风控冷却中，约 {minutes} 分钟后再试"));
    }
    let auto = service.auto_settings();
    if auto.pause_on_budget
        && auto.monthly_budget_cny > 0.0
        && auto.monthly_spend_cny >= auto.monthly_budget_cny
    {
        return Err("本月预算已用完，自动检查已暂停；调整预算或关闭「预算用完时暂停」后继续".into());
    }
    let cookies = match cookies(service) {
        Ok(cookies) => cookies,
        Err(error) => {
            let _ = service.store().record_subscription_check(
                &sub.id,
                now,
                now + AUTH_COOLDOWN_SECS * 1000,
                &error,
                "",
                sub.consecutive_failures,
            );
            return Err(error);
        }
    };
    let page = match tokio::time::timeout(
        Duration::from_secs(45),
        clients::fetch_bili_up_videos(&cookies, sub.mid, 1),
    )
    .await
    {
        Ok(Ok(page)) => page,
        Ok(Err(error)) => return Err(record_failure(service, sub, &error)),
        Err(_) => return Err(record_failure(service, sub, "B站空间请求超时（45 秒）")),
    };
    if sub.baseline_at == 0 {
        return baseline(service, root, sub, &page.items, backfill).await;
    }
    let store = service.store();
    let vault = root.to_string_lossy().to_string();
    let used = store.queued_auto_today(&vault).unwrap_or(0);
    let daily_remaining = auto.max_per_day.saturating_sub(used) as usize;
    let limit = (auto.max_per_check as usize).min(daily_remaining);
    let (new_tasks, skipped_existing, skipped_filter, remaining) = if limit == 0 {
        let unseen = page
            .items
            .iter()
            .filter(|video| !store.seen_exists(&vault, sub.mid, &video.bvid).unwrap_or(false))
            .count();
        (0, 0, 0, unseen)
    } else {
        process_candidates(service, root, sub, &page.items, limit, sub.baseline_at)
    };
    let message = if limit == 0 && remaining > 0 {
        format!("今日自动任务已达上限（{} 条）；还有 {remaining} 条留到明天", auto.max_per_day)
    } else if new_tasks == 0 && skipped_existing == 0 && skipped_filter == 0 {
        "没有新视频".to_string()
    } else {
        let extra = if remaining > 0 {
            format!("；还有 {remaining} 条留到下次")
        } else {
            String::new()
        };
        format!(
            "新任务 {new_tasks} 个（跳过已存在 {skipped_existing}，过滤 {skipped_filter}）{extra}"
        )
    };
    let next = next_check_at(now, sub.interval_minutes);
    store.record_subscription_check(&sub.id, now, next, "", &message, 0)?;
    Ok(SubscriptionCheckResult {
        checked_at: now,
        baseline: false,
        new_tasks,
        skipped_existing,
        skipped_filter,
        remaining,
        message,
    })
}

/// 添加订阅。资料读取失败会直接返回；基线失败仍保留订阅，并把错误记在订阅上，
/// 用户重新登录后点「立即检查」即可补上基线。
pub async fn create(
    service: &ProductionService,
    root: &Path,
    mut input: SubscriptionInput,
) -> Result<Subscription, String> {
    if input.filters.min_duration_sec > 0
        && input.filters.max_duration_sec > 0
        && input.filters.min_duration_sec > input.filters.max_duration_sec
    {
        return Err("最小时长不能大于最大时长".into());
    }
    let profile = clients::fetch_bili_up_profile(input.mid).await?;
    input.mid = profile.mid;
    input.name = profile.name;
    input.avatar = profile.face;
    let vault = root.to_string_lossy().to_string();
    let sub = service.store().save_subscription(&vault, &input)?;
    let _guard = check_lock().lock().await;
    if let Err(error) = check_locked(service, root, &sub, input.backfill.min(MAX_BACKFILL)).await {
        // check_locked already persisted the real backoff for B站/网络错误；
        // 不要在这里用固定的 6 小时覆盖它。
        log::warn!("initial subscription check failed: {error}");
    }
    service
        .store()
        .get_subscription(&sub.id)?
        .ok_or_else(|| "订阅不存在".into())
}

pub async fn check_now(
    app: &AppHandle,
    service: &ProductionService,
    root: &Path,
    id: &str,
) -> Result<SubscriptionCheckResult, String> {
    let sub = service
        .store()
        .get_subscription(id)?
        .ok_or_else(|| "订阅不存在".to_string())?;
    if !sub.vault_path.is_empty() && sub.vault_path != root.to_string_lossy() {
        return Err("这个订阅属于另一个笔记库".into());
    }
    let _guard = check_lock().lock().await;
    let result = check_locked(service, root, &sub, 0).await;
    let _ = app.emit("production-subscriptions-changed", ());
    result
}

/// 调度器每轮调用一次；返回本轮检查数量。
pub async fn run_due(
    app: &AppHandle,
    service: &ProductionService,
    root: &Path,
) -> Result<usize, String> {
    let auto = service.auto_settings();
    if !auto.enabled {
        return Ok(0);
    }
    if auto.pause_on_budget
        && auto.monthly_budget_cny > 0.0
        && auto.monthly_spend_cny >= auto.monthly_budget_cny
    {
        return Ok(0);
    }
    if !auto.bili_logged_in {
        return Ok(0);
    }
    let vault = root.to_string_lossy().to_string();
    let due = service.store().due_subscriptions(&vault, now_ms())?;
    let mut checked = 0usize;
    for sub in due {
        let _guard = check_lock().lock().await;
        if let Err(error) = check_locked(service, root, &sub, 0).await {
            log::warn!("subscription {} check failed: {error}", sub.name);
        }
        drop(_guard);
        checked += 1;
        let _ = app.emit("production-subscriptions-changed", ());
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Ok(checked)
}

pub fn parse_mid(query: &str) -> Result<i64, String> {
    clients::parse_up_mid(query).ok_or_else(|| "只支持 UID 或 space.bilibili.com 空间链接".to_string())
}

pub fn allowed_by_filters(filters: &SubscriptionFilters, title: &str, duration: i64) -> bool {
    filters.allows(title, duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_handle_duration_and_keywords() {
        let filters = SubscriptionFilters {
            min_duration_sec: 60,
            max_duration_sec: 3600,
            include_keywords: "AI, 模型".into(),
            exclude_keywords: "直播, 切片".into(),
        };
        assert!(filters.allows("AI 模型新进展", 600));
        assert!(!filters.allows("AI 模型新进展", 30));
        assert!(!filters.allows("AI 模型新进展", 7200));
        assert!(!filters.allows("随便聊聊", 600));
        assert!(!filters.allows("AI 直播回放", 600));
        // 时长未知时不拿 0 当“太短”，避免漏掉直播回放/首映。
        assert!(filters.allows("AI 模型", 0));
    }

    #[test]
    fn baseline_takes_newest_backfill_in_descending_order() {
        let videos = vec![
            BiliVideoRef { bvid: "old".into(), pubdate: 10, ..Default::default() },
            BiliVideoRef { bvid: "new".into(), pubdate: 30, ..Default::default() },
            BiliVideoRef { bvid: "mid".into(), pubdate: 20, ..Default::default() },
        ];
        let picked = baseline_candidates(&videos, 2);
        assert_eq!(picked.iter().map(|video| video.bvid.as_str()).collect::<Vec<_>>(), vec!["new", "mid"]);
    }

    #[test]
    fn parses_space_uid_and_rejects_video_urls() {
        assert_eq!(parse_mid("946974").unwrap(), 946974);
        assert_eq!(parse_mid("https://space.bilibili.com/946974/video").unwrap(), 946974);
        assert!(parse_mid("https://www.bilibili.com/video/BV1xx").is_err());
    }

    #[test]
    fn failure_delay_uses_long_cooldown_for_risk_control() {
        let now = now_ms();
        let auth = failure_delay("B站登录已过期，请重新扫码", 1, 30);
        assert!(auth >= now + AUTH_COOLDOWN_SECS * 1000 - 1000);
        let risk = failure_delay("B站风控拦截（HTTP 412）", 1, 30);
        assert!(risk >= now + HTTP_412_COOLDOWN_SECS * 1000 - 1000);
        let generic = failure_delay("网络错误", 1, 30);
        assert!(generic >= now + 2 * 30 * 60_000 - 1000);
        let transient = failure_delay("B站空间请求失败：连接失败（DNS/代理/TLS）", 1, 30);
        assert!(transient >= now + 2 * 60_000 - 1000);
        assert!(transient < now + 5 * 60_000);
    }
}
