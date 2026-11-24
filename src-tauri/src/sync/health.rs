//! Reachability probe for the sync server.
//!
//! The sync address is per-device configuration, but it is easy to leave a
//! retired transport behind (the old SSH tunnel used `127.0.0.1:18787`). A dead
//! loopback address looks "configured" in the UI while nothing syncs, so the
//! app probes the server and reports what is actually reachable instead.
//!
//! See `docs/SYNC_STATE.md` for the deployed topology.

use std::time::{Duration, Instant};

use serde::Serialize;

use super::SyncEngine;

/// Addresses worth trying when the configured one is a dead loopback.
/// WireGuard is the current transport; keep this list short and explicit.
pub const CANDIDATE_SERVERS: &[&str] = &["http://10.88.0.1:8787"];

const PROBE_TIMEOUT: Duration = Duration::from_millis(2_000);
const CANDIDATE_TIMEOUT: Duration = Duration::from_millis(1_500);
const HEAL_TIMEOUT: Duration = Duration::from_millis(800);

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProbe {
    pub url: String,
    pub ok: bool,
    pub status: u16,
    pub latency_ms: i64,
    pub error: String,
    /// `loopback` | `tailscale` | `private` | `public` | `invalid`
    pub kind: String,
    pub loopback: bool,
    /// The retired SSH-tunnel address (`127.0.0.1:18787`).
    pub legacy: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub configured: bool,
    pub vault_id: String,
    pub current: ServerProbe,
    pub candidates: Vec<ServerProbe>,
    pub recommended_url: String,
    pub last_sync_at: i64,
    pub last_error: String,
    pub hint: String,
}

pub fn probe(url: &str, timeout: Duration) -> ServerProbe {
    let base = url.trim().trim_end_matches('/');
    let mut result = describe(base);
    if base.is_empty() {
        result.error = "地址为空".to_string();
        return result;
    }
    let started = Instant::now();
    let client = match reqwest::blocking::Client::builder().timeout(timeout).build() {
        Ok(client) => client,
        Err(error) => {
            result.error = error.to_string();
            return result;
        }
    };
    match client.get(format!("{base}/v1/health")).send() {
        Ok(response) => {
            result.status = response.status().as_u16();
            result.ok = response.status().is_success();
            if !result.ok {
                result.error = format!("HTTP {}", response.status());
            }
        }
        Err(error) => result.error = short_error(error),
    }
    result.latency_ms = started.elapsed().as_millis() as i64;
    result
}

fn describe(url: &str) -> ServerProbe {
    let parsed = reqwest::Url::parse(url).ok();
    let host = parsed.as_ref().and_then(|value| value.host_str()).unwrap_or("").to_ascii_lowercase();
    let port = parsed.as_ref().and_then(|value| value.port_or_known_default()).unwrap_or(0);
    let loopback = is_loopback_host(&host);
    ServerProbe {
        url: url.to_string(),
        kind: classify(&host, loopback).to_string(),
        loopback,
        legacy: loopback && port == 18787,
        ..ServerProbe::default()
    }
}

pub fn is_loopback(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(|host| is_loopback_host(&host.to_ascii_lowercase())))
        .unwrap_or(false)
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host == "::1" || host == "[::1]" || host.starts_with("127.")
}

fn classify(host: &str, loopback: bool) -> &'static str {
    if host.is_empty() {
        return "invalid";
    }
    if loopback {
        return "loopback";
    }
    if host.ends_with(".ts.net") {
        return "tailscale";
    }
    let octets: Vec<u8> = host.split('.').filter_map(|part| part.parse().ok()).collect();
    if octets.len() == 4 {
        if octets[0] == 100 && (64..=127).contains(&octets[1]) {
            return "tailscale";
        }
        if octets[0] == 10
            || (octets[0] == 192 && octets[1] == 168)
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        {
            return "private";
        }
    }
    "public"
}

fn short_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "连接超时".to_string()
    } else if error.is_connect() {
        "连接失败（端口未开放，或虚拟网络未连接）".to_string()
    } else {
        error.to_string()
    }
}

/// Probe the active server, plus the known fallback when it is a loopback.
pub fn diagnose(engine: &SyncEngine) -> Result<HealthReport, String> {
    let state = engine.state()?;
    let current = probe(&state.server_url, PROBE_TIMEOUT);
    let mut candidates = Vec::new();
    if !current.ok && current.loopback {
        for candidate in CANDIDATE_SERVERS {
            candidates.push(probe(candidate, CANDIDATE_TIMEOUT));
        }
    }
    let recommended = candidates.iter().find(|item| item.ok).map(|item| item.url.clone()).unwrap_or_default();
    let hint = hint_for(state.configured, &current, &recommended);
    Ok(HealthReport {
        configured: state.configured,
        vault_id: state.vault_id,
        current,
        candidates,
        recommended_url: recommended,
        last_sync_at: state.last_sync_at,
        last_error: state.last_error,
        hint,
    })
}

fn hint_for(configured: bool, current: &ServerProbe, recommended: &str) -> String {
    if !configured {
        return "还没配置同步。填写服务器地址、笔记库 ID 和 Token 后保存。".to_string();
    }
    if current.ok && current.loopback {
        return "本机端口可达（可能走了隧道）。这个地址在另一台电脑上指向它自己，不能照抄。".to_string();
    }
    if current.ok {
        return "服务器可达。".to_string();
    }
    if !recommended.is_empty() {
        return format!("当前地址不可达，但 {recommended} 可达。");
    }
    if current.loopback {
        return "当前是本机 loopback 地址，服务器不可达。先在系统里连上 WireGuard / Tailscale，或把地址改成服务器在虚拟网络里的地址。".to_string();
    }
    "服务器不可达。检查虚拟网络是否连接、地址和端口是否正确。".to_string()
}

/// If the configured address is a dead loopback and a known candidate answers,
/// move to it. Returns the adopted address.
///
/// Only loopback addresses are healed: a public or tailnet address that is
/// temporarily offline must not be silently replaced.
pub fn heal_loopback(engine: &SyncEngine) -> Option<String> {
    heal(engine, CANDIDATE_SERVERS)
}

fn heal(engine: &SyncEngine, candidates: &[&str]) -> Option<String> {
    let state = engine.state().ok()?;
    if !state.configured || !is_loopback(&state.server_url) {
        return None;
    }
    if probe(&state.server_url, HEAL_TIMEOUT).ok {
        return None;
    }
    for candidate in candidates {
        if !probe(candidate, CANDIDATE_TIMEOUT).ok {
            continue;
        }
        if engine.configure(candidate, "", &state.vault_id).is_err() {
            return None;
        }
        log::warn!("sync address moved from {} to {}: old address unreachable", state.server_url, candidate);
        return Some((*candidate).to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    #[test]
    fn probe_reads_the_health_endpoint() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = "{\"ok\":true}";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        });
        let result = probe(&format!("http://{address}"), Duration::from_secs(2));
        server.join().expect("server");
        assert!(result.ok, "probe failed: {}", result.error);
        assert_eq!(result.status, 200);
        assert!(result.loopback);
    }

    #[test]
    fn diagnose_reads_the_configured_state() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = "{\"ok\":true}";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        });
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("yb-sync-health-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let engine = SyncEngine::open(&dir.join("yebian.sqlite")).expect("engine");
        engine
            .configure(&format!("http://{address}"), "token", "vault-test")
            .expect("configure");
        let report = diagnose(&engine).expect("report");
        server.join().expect("server");
        assert!(report.configured);
        assert_eq!(report.vault_id, "vault-test");
        assert!(report.current.ok, "probe failed: {}", report.current.error);
        assert!(report.current.loopback);
        assert!(report.candidates.is_empty());
        assert!(report.hint.contains("本机端口可达"));
        drop(engine);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn heal_moves_a_dead_loopback_and_keeps_identity() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = "{\"ok\":true}";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        });
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("yb-sync-heal-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let engine = SyncEngine::open(&dir.join("yebian.sqlite")).expect("engine");
        engine
            .configure("http://127.0.0.1:1", "keep-me", "vault-test")
            .expect("configure");
        let candidate = format!("http://{address}");
        let adopted = heal(&engine, &[candidate.as_str()]).expect("adopt");
        server.join().expect("server");
        assert_eq!(adopted, candidate);
        let state = engine.state().expect("state");
        assert_eq!(state.server_url, candidate);
        assert_eq!(state.vault_id, "vault-test");
        assert!(state.has_token);
        assert!(state.configured);
        assert!(state.auto_enabled);
        drop(engine);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loopback_and_legacy_are_distinguished() {
        let legacy = describe("http://127.0.0.1:18787");
        assert!(legacy.loopback);
        assert!(legacy.legacy);
        assert_eq!(legacy.kind, "loopback");

        let direct = describe("http://10.88.0.1:8787");
        assert!(!direct.loopback);
        assert!(!direct.legacy);
        assert_eq!(direct.kind, "private");
    }

    #[test]
    fn tailscale_and_public_hosts_are_classified() {
        assert_eq!(describe("http://100.101.102.103:8787").kind, "tailscale");
        assert_eq!(describe("http://server.tailnet.ts.net:8787").kind, "tailscale");
        assert_eq!(describe("https://sync.example.com").kind, "public");
        assert_eq!(describe("not a url").kind, "invalid");
    }

    #[test]
    fn loopback_detection_accepts_localhost_and_127_range() {
        assert!(is_loopback("http://localhost:8787"));
        assert!(is_loopback("http://127.0.0.1:18787"));
        assert!(is_loopback("http://127.1.2.3:8787"));
        assert!(is_loopback("http://[::1]:8787"));
        assert!(!is_loopback("http://10.88.0.1:8787"));
        assert!(!is_loopback(""));
    }

    #[test]
    fn a_dead_loopback_has_a_hint() {
        let dead = describe("http://127.0.0.1:18787");
        let hint = hint_for(true, &dead, "");
        assert!(hint.contains("loopback"));
    }
}
