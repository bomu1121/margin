use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, REFERER, SET_COOKIE, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::BiliVideoRef;

const BILI_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const BILI_REFERER: &str = "https://www.bilibili.com/";

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BiliAccount {
    pub mid: i64,
    pub name: String,
    #[serde(default)]
    pub face: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiliFolder {
    pub id: i64,
    pub title: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiliPage {
    pub cid: i64,
    pub page: i64,
    pub part: String,
    pub duration: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiliVideoPreview {
    pub bvid: String,
    pub title: String,
    pub author: String,
    pub duration: i64,
    pub pubdate: i64,
    pub cover: String,
    pub pages: Vec<BiliPage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiliLoginStart {
    pub qrcode_key: String,
    pub url: String,
    pub modules: Vec<Vec<bool>>,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BiliLoginPoll {
    pub status: String,
    pub message: String,
    pub account: Option<BiliAccount>,
    #[serde(skip_serializing)]
    pub cookies: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChatResult {
    pub content: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub uncached_input_tokens: u64,
    pub cached_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub provider_cost: Option<f64>,
    pub provider_currency: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AsrResult {
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub content: String,
}

fn bili_headers(request: reqwest::RequestBuilder, cookies: &str) -> reqwest::RequestBuilder {
    let request = request
        .header(USER_AGENT, BILI_UA)
        .header(REFERER, BILI_REFERER);
    if cookies.trim().is_empty() {
        request
    } else {
        request.header("Cookie", cookies)
    }
}

async fn bili_json(cookies: &str, url: &str) -> Result<Value, String> {
    let response = bili_headers(client()?.get(url), cookies)
        .send()
        .await
        .map_err(|error| format!("B站请求失败：{error}"))?;
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("B站接口 {status}：{}", body.chars().take(300).collect::<String>()));
    }
    serde_json::from_str(&body).map_err(|error| format!("B站返回无法解析：{error}"))
}

fn bili_code(value: &Value) -> Result<&Value, String> {
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| value.get("msg").and_then(Value::as_str))
            .unwrap_or("B站接口返回错误");
        return Err(format!("{message}（code {code}）"));
    }
    value.get("data").ok_or_else(|| "B站返回缺少 data".to_string())
}

fn find_bvid(input: &str) -> Option<String> {
    let index = input.find("BV")?;
    let rest = &input[index..];
    if rest.len() < 12 {
        return None;
    }
    let candidate = &rest[..12];
    if candidate[2..].chars().all(|ch| ch.is_ascii_alphanumeric()) {
        Some(candidate.to_string())
    } else {
        None
    }
}

async fn resolve_bili_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if !trimmed.starts_with("http") {
        return Ok(trimmed.to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("b23.tv") {
        let response = bili_headers(client()?.get(trimmed), "")
            .send()
            .await
            .map_err(|error| format!("短链解析失败：{error}"))?;
        return Ok(response.url().to_string());
    }
    Ok(trimmed.to_string())
}

async fn view_json(input: &str) -> Result<Value, String> {
    let resolved = resolve_bili_url(input).await?;
    let bvid = find_bvid(&resolved);
    let aid = resolved
        .split(['?', '&'])
        .find_map(|part| part.strip_prefix("aid="))
        .and_then(|value| value.split('&').next())
        .map(|value| value.to_string());
    let url = if let Some(bvid) = bvid {
        format!("https://api.bilibili.com/x/web-interface/view?bvid={bvid}")
    } else if let Some(aid) = aid {
        format!("https://api.bilibili.com/x/web-interface/view?aid={aid}")
    } else {
        return Err("没有识别到 BV / av 号".into());
    };
    let value = bili_json("", &url).await?;
    Ok(bili_code(&value)?.clone())
}

pub async fn fetch_bili_preview(input: &str) -> Result<BiliVideoPreview, String> {
    let data = view_json(input).await?;
    let pages = data
        .get("pages")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| BiliPage {
                    cid: item.get("cid").and_then(Value::as_i64).unwrap_or(0),
                    page: item.get("page").and_then(Value::as_i64).unwrap_or(1),
                    part: item.get("part").and_then(Value::as_str).unwrap_or("正片").to_string(),
                    duration: item.get("duration").and_then(Value::as_i64).unwrap_or(0),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(BiliVideoPreview {
        bvid: data.get("bvid").and_then(Value::as_str).unwrap_or_default().to_string(),
        title: data.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
        author: data
            .get("owner")
            .and_then(|owner| owner.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        duration: data.get("duration").and_then(Value::as_i64).unwrap_or(0),
        pubdate: data.get("pubdate").and_then(Value::as_i64).unwrap_or(0),
        cover: data.get("pic").and_then(Value::as_str).unwrap_or_default().to_string(),
        pages,
    })
}

pub async fn bili_audio_url(bvid: &str, cid: i64) -> Result<String, String> {
    let url = format!(
        "https://api.bilibili.com/x/player/playurl?bvid={}&cid={}&fnval=16&fourk=1&qn=64",
        urlencoding(bvid),
        cid
    );
    let value = bili_json("", &url).await?;
    let data = bili_code(&value)?;
    let audio = data
        .get("dash")
        .and_then(|dash| dash.get("audio"))
        .and_then(Value::as_array)
        .and_then(|items| items.first());
    if let Some(audio) = audio {
        for key in ["baseUrl", "base_url"] {
            if let Some(url) = audio.get(key).and_then(Value::as_str) {
                if !url.is_empty() {
                    return Ok(url.to_string());
                }
            }
        }
    }
    data.get("durl")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("url"))
        .and_then(Value::as_str)
        .map(|url| url.to_string())
        .ok_or_else(|| "这条视频没有可下载的音轨".to_string())
}

pub async fn download_bili_audio(url: &str, target: &Path, token: &CancellationToken) -> Result<u64, String> {
    let response = tokio::select! {
        _ = token.cancelled() => return Err("已取消".into()),
        response = bili_headers(client()?.get(url), "") .send() => response.map_err(|error| format!("下载 B站音轨失败：{error}"))?,
    };
    let status = response.status();
    if !status.is_success() {
        return Err(format!("下载 B站音轨失败：HTTP {status}"));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| error.to_string())?;
    }
    let mut file = tokio::fs::File::create(target).await.map_err(|error| error.to_string())?;
    let mut stream = response.bytes_stream();
    let mut written = 0u64;
    while let Some(chunk) = tokio::select! {
        _ = token.cancelled() => return Err("已取消".into()),
        chunk = stream.next() => chunk,
    } {
        let chunk = chunk.map_err(|error| error.to_string())?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|error| error.to_string())?;
        written += chunk.len() as u64;
    }
    Ok(written)
}

pub async fn fetch_bili_account(cookies: &str) -> Result<BiliAccount, String> {
    let value = bili_json(cookies, "https://api.bilibili.com/x/web-interface/nav").await?;
    let data = bili_code(&value)?;
    let is_login = data.get("isLogin").and_then(Value::as_bool).unwrap_or(false);
    if !is_login {
        return Err("B站登录已过期，请重新扫码".into());
    }
    Ok(BiliAccount {
        mid: data.get("mid").and_then(Value::as_i64).unwrap_or(0),
        name: data.get("uname").and_then(Value::as_str).unwrap_or_default().to_string(),
        face: data.get("face").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

pub async fn fetch_bili_folders(cookies: &str) -> Result<Vec<BiliFolder>, String> {
    let account = fetch_bili_account(cookies).await?;
    let url = format!(
        "https://api.bilibili.com/x/v3/fav/folder/created/list-all?up_mid={}",
        account.mid
    );
    let value = bili_json(cookies, &url).await?;
    let data = bili_code(&value)?;
    let list = data
        .get("list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(list
        .iter()
        .map(|item| BiliFolder {
            id: item.get("id").and_then(Value::as_i64).unwrap_or(0),
            title: item.get("title").and_then(Value::as_str).unwrap_or("收藏夹").to_string(),
            count: item.get("media_count").and_then(Value::as_i64).unwrap_or(0),
        })
        .collect())
}

pub async fn fetch_bili_folder_videos(cookies: &str, media_id: i64) -> Result<Vec<BiliVideoRef>, String> {
    let url = format!(
        "https://api.bilibili.com/x/v3/fav/resource/list?media_id={media_id}&pn=1&ps=40&platform=web"
    );
    let value = bili_json(cookies, &url).await?;
    let data = bili_code(&value)?;
    let medias = data
        .get("medias")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(medias
        .iter()
        .filter_map(|item| {
            let bvid = item.get("bvid").and_then(Value::as_str)?.to_string();
            Some(BiliVideoRef {
                bvid,
                title: item.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
                author: item
                    .get("upper")
                    .and_then(|upper| upper.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                duration: item.get("duration").and_then(Value::as_i64).unwrap_or(0),
                pubdate: item.get("pubtime").and_then(Value::as_i64).unwrap_or(0),
                cover: item.get("cover").and_then(Value::as_str).unwrap_or_default().to_string(),
                cid: 0,
            })
        })
        .collect())
}

pub async fn fetch_bili_watch_later(cookies: &str) -> Result<Vec<BiliVideoRef>, String> {
    let value = bili_json(cookies, "https://api.bilibili.com/x/v2/history/toview").await?;
    let data = bili_code(&value)?;
    let list = data.get("list").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(list
        .iter()
        .filter_map(|item| {
            let bvid = item.get("bvid").and_then(Value::as_str)?.to_string();
            Some(BiliVideoRef {
                bvid,
                title: item.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
                author: item
                    .get("owner")
                    .and_then(|owner| owner.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                duration: item.get("duration").and_then(Value::as_i64).unwrap_or(0),
                pubdate: item.get("pubtime").and_then(Value::as_i64).unwrap_or(0),
                cover: item.get("pic").and_then(Value::as_str).unwrap_or_default().to_string(),
                cid: item.get("cid").and_then(Value::as_i64).unwrap_or(0),
            })
        })
        .collect())
}


pub async fn fetch_bili_history(cookies: &str) -> Result<Vec<BiliVideoRef>, String> {
    let value = bili_json(
        cookies,
        "https://api.bilibili.com/x/web-interface/history/cursor?max=0&view_at=0&business=archive&ps=20",
    )
    .await?;
    let data = bili_code(&value)?;
    let list = data.get("list").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(list
        .iter()
        .filter_map(|item| {
            let history = item.get("history")?;
            if history.get("business").and_then(Value::as_str) != Some("archive") {
                return None;
            }
            let bvid = history.get("bvid").and_then(Value::as_str)?.to_string();
            Some(BiliVideoRef {
                bvid,
                title: item.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
                author: item.get("author_name").and_then(Value::as_str).unwrap_or_default().to_string(),
                duration: item.get("duration").and_then(Value::as_i64).unwrap_or(0),
                pubdate: item.get("pubdate").and_then(Value::as_i64).unwrap_or(0),
                cover: item.get("cover").and_then(Value::as_str).unwrap_or_default().to_string(),
                cid: history.get("cid").and_then(Value::as_i64).unwrap_or(0),
            })
        })
        .collect())
}

pub async fn bili_login_start() -> Result<BiliLoginStart, String> {
    let value = bili_json("", "https://passport.bilibili.com/x/passport-login/web/qrcode/generate").await?;
    let data = bili_code(&value)?;
    let url = data.get("url").and_then(Value::as_str).unwrap_or_default().to_string();
    let qrcode_key = data.get("qrcode_key").and_then(Value::as_str).unwrap_or_default().to_string();
    Ok(BiliLoginStart {
        qrcode_key,
        url,
        // The QR is rendered in the webview; the backend only returns the
        // canonical login URL and polling key.
        modules: Vec::new(),
        expires_in: 180,
    })
}

pub async fn bili_login_poll(qrcode_key: &str) -> Result<BiliLoginPoll, String> {
    let url = format!(
        "https://passport.bilibili.com/x/passport-login/web/qrcode/poll?qrcode_key={}",
        urlencoding(qrcode_key)
    );
    let response = bili_headers(client()?.get(&url), "")
        .send()
        .await
        .map_err(|error| format!("B站登录轮询失败：{error}"))?;
    let status = response.status();
    let set_cookie = cookie_from_set_cookie(response.headers());
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("B站登录轮询 {status}：{}", text.chars().take(300).collect::<String>()));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| format!("B站登录返回无法解析：{error}"))?;
    let data = bili_code(&value)?;
    let code = data.get("code").and_then(Value::as_i64).unwrap_or(86101);
    if code == 86101 {
        return Ok(BiliLoginPoll { status: "pending".into(), message: "等待扫码".into(), ..Default::default() });
    }
    if code == 86090 {
        return Ok(BiliLoginPoll { status: "scanned".into(), message: "已扫码，等待确认".into(), ..Default::default() });
    }
    if code == 86038 {
        return Ok(BiliLoginPoll { status: "expired".into(), message: "二维码已过期".into(), ..Default::default() });
    }
    if code != 0 {
        return Ok(BiliLoginPoll {
            status: "error".into(),
            message: data.get("message").and_then(Value::as_str).unwrap_or("登录失败").to_string(),
            ..Default::default()
        });
    }

    // B站成功回调的 Cookie 主要放在 Set-Cookie 响应头里；`data.url` 里的
    // query 只是旧版/兜底来源。之前只读 URL，导致“显示登录成功但没拿到 Cookie”。
    let redirect = data.get("url").and_then(Value::as_str).unwrap_or_default();
    let cookies = if set_cookie.is_empty() {
        cookies_from_redirect(redirect)
    } else {
        set_cookie
    };
    if cookies.is_empty() {
        return Ok(BiliLoginPoll {
            status: "error".into(),
            message: "登录成功但没拿到 Cookie；请关闭二维码后重新扫码".into(),
            ..Default::default()
        });
    }
    let account = match fetch_bili_account(&cookies).await {
        Ok(account) => Some(account),
        Err(error) => {
            return Ok(BiliLoginPoll {
                status: "error".into(),
                message: format!("拿到 Cookie 但账号校验失败：{error}"),
                cookies: Some(cookies),
                account: None,
            });
        }
    };
    Ok(BiliLoginPoll {
        status: "success".into(),
        message: account.as_ref().map(|item| format!("登录成功：{}", item.name)).unwrap_or_else(|| "登录成功".into()),
        account,
        cookies: Some(cookies),
    })
}

fn cookie_from_set_cookie(headers: &reqwest::header::HeaderMap) -> String {
    let wanted = ["SESSDATA", "bili_jct", "DedeUserID", "DedeUserID__ckMd5", "sid"];
    let mut pairs: Vec<String> = Vec::new();
    for value in headers.get_all(SET_COOKIE).iter() {
        let Ok(line) = value.to_str() else { continue };
        let Some(first) = line.split(';').next() else { continue };
        let Some((name, raw_value)) = first.split_once('=') else { continue };
        if !wanted.contains(&name.trim()) {
            continue;
        }
        let pair = format!("{}={}", name.trim(), raw_value.trim());
        if !pairs.iter().any(|item| item.starts_with(&format!("{}=", name.trim()))) {
            pairs.push(pair);
        }
    }
    pairs.join("; ")
}

fn cookies_from_redirect(redirect: &str) -> String {
    let Ok(url) = Url::parse(redirect) else { return String::new() };
    let mut pairs = Vec::new();
    for (key, value) in url.query_pairs() {
        if matches!(key.as_ref(), "SESSDATA" | "bili_jct" | "DedeUserID" | "DedeUserID__ckMd5" | "sid") {
            pairs.push(format!("{key}={value}"));
        }
    }
    pairs.join("; ")
}

fn urlencoding(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(*byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

pub async fn chat(config: &AiConfig, system: &str, user: &str, token: &CancellationToken) -> Result<ChatResult, String> {
    if config.api_key.trim().is_empty() {
        return Err("还没有配置 AI API Key".into());
    }
    let mut last_error = String::new();
    for attempt in 0..=config.retries {
        if token.is_cancelled() {
            return Err("已取消".into());
        }
        let result = match config.protocol.as_str() {
            "anthropic" => chat_anthropic(config, system, user, token).await,
            "gemini" => chat_gemini(config, system, user, token).await,
            _ => chat_openai(config, system, user, token).await,
        };
        match result {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = error;
                if !last_error.contains("HTTP 5")
                    && !last_error.contains("HTTP 429")
                    && !last_error.contains("AI 请求失败")
                {
                    return Err(last_error);
                }
            }
        }
        if attempt < config.retries {
            tokio::select! {
                _ = token.cancelled() => return Err("已取消".into()),
                _ = tokio::time::sleep(Duration::from_millis(800 * (attempt as u64 + 1))) => {}
            }
        }
    }
    Err(last_error)
}

async fn send_chat_request(
    request: reqwest::RequestBuilder,
    token: &CancellationToken,
) -> Result<Value, String> {
    let response = tokio::select! {
        _ = token.cancelled() => return Err("已取消".into()),
        response = request.send() => response.map_err(|error| format!("AI 请求失败：{error}"))?,
    };
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("AI 接口 {status}：{}", compact_error_body(&text)));
    }
    serde_json::from_str(&text).map_err(|error| format!("AI 返回无法解析：{error}"))
}

async fn chat_openai(
    config: &AiConfig,
    system: &str,
    user: &str,
    token: &CancellationToken,
) -> Result<ChatResult, String> {
    let base = config.base_url.trim_end_matches('/');
    let endpoint = format!("{base}/chat/completions");
    let body = json!({
        "model": config.model,
        "temperature": 0.25,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    });
    let request = client()?
        .post(&endpoint)
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    let value = send_chat_request(request, token).await?;
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("AI 返回了空内容".into());
    }
    let usage = value.get("usage");
    let prompt_tokens = usage.and_then(|value| value.get("prompt_tokens")).and_then(Value::as_u64).unwrap_or(0);
    let completion_tokens = usage
        .and_then(|value| value.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached_tokens = usage
        .and_then(|value| value.get("prompt_tokens_details"))
        .and_then(|value| value.get("cached_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| usage.and_then(|value| value.get("prompt_cache_hit_tokens")).and_then(Value::as_u64))
        .or_else(|| usage.and_then(|value| value.get("cached_tokens")).and_then(Value::as_u64))
        .unwrap_or(0);
    let cache_write_tokens = usage
        .and_then(|value| value.get("prompt_tokens_details"))
        .and_then(|value| value.get("cache_write_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| usage.and_then(|value| value.get("cache_write_tokens")).and_then(Value::as_u64))
        .unwrap_or(0);
    let reasoning_tokens = usage
        .and_then(|value| value.get("completion_tokens_details"))
        .and_then(|value| value.get("reasoning_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let provider_cost = usage.and_then(|value| value.get("cost")).and_then(value_number);
    Ok(ChatResult {
        content,
        prompt_tokens,
        completion_tokens,
        uncached_input_tokens: prompt_tokens.saturating_sub(cached_tokens).saturating_sub(cache_write_tokens),
        cached_tokens,
        cache_write_tokens,
        reasoning_tokens,
        provider_cost,
        provider_currency: provider_cost.map(|_| "USD".to_string()),
    })
}

async fn chat_anthropic(
    config: &AiConfig,
    system: &str,
    user: &str,
    token: &CancellationToken,
) -> Result<ChatResult, String> {
    let base = config.base_url.trim_end_matches('/');
    let endpoint = format!("{base}/messages");
    let body = json!({
        "model": config.model,
        "max_tokens": 8192,
        "temperature": 0.25,
        "system": system,
        "messages": [{"role": "user", "content": user}]
    });
    let request = client()?
        .post(&endpoint)
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    let value = send_chat_request(request, token).await?;
    let content = value
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("Anthropic 返回了空内容".into());
    }
    let usage = value.get("usage");
    let input = usage.and_then(|value| value.get("input_tokens")).and_then(Value::as_u64).unwrap_or(0);
    let cached = usage
        .and_then(|value| value.get("cache_read_input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_write = usage
        .and_then(|value| value.get("cache_creation_input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage.and_then(|value| value.get("output_tokens")).and_then(Value::as_u64).unwrap_or(0);
    Ok(ChatResult {
        content,
        prompt_tokens: input + cached + cache_write,
        completion_tokens: output,
        uncached_input_tokens: input,
        cached_tokens: cached,
        cache_write_tokens: cache_write,
        reasoning_tokens: 0,
        provider_cost: None,
        provider_currency: None,
    })
}

async fn chat_gemini(
    config: &AiConfig,
    system: &str,
    user: &str,
    token: &CancellationToken,
) -> Result<ChatResult, String> {
    let base = config.base_url.trim_end_matches('/');
    let model = config.model.trim_start_matches("models/");
    let endpoint = format!("{base}/models/{model}:generateContent?key={}", urlencoding(&config.api_key));
    let body = json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": [{"role": "user", "parts": [{"text": user}]}],
        "generationConfig": {"temperature": 0.25}
    });
    let request = client()?
        .post(&endpoint)
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    let value = send_chat_request(request, token).await?;
    let content = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("Gemini 返回了空内容".into());
    }
    let usage = value.get("usageMetadata");
    let prompt = usage
        .and_then(|value| value.get("promptTokenCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached = usage
        .and_then(|value| value.get("cachedContentTokenCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .and_then(|value| value.get("candidatesTokenCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let reasoning = usage
        .and_then(|value| value.get("thoughtsTokenCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Ok(ChatResult {
        content,
        prompt_tokens: prompt,
        completion_tokens: output,
        uncached_input_tokens: prompt.saturating_sub(cached),
        cached_tokens: cached,
        cache_write_tokens: 0,
        reasoning_tokens: reasoning,
        provider_cost: None,
        provider_currency: None,
    })
}

fn value_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

/// MiMo 的 data URL 上限是 10MB；留出 JSON 包装余量。
pub const MIMO_MAX_AUDIO_BYTES: usize = 7_000_000;

pub async fn transcribe(config: &AsrConfig, path: &Path, token: &CancellationToken) -> Result<AsrResult, String> {
    if config.api_key.trim().is_empty() {
        return Err("还没有配置语音识别 API Key".into());
    }
    if config.engine == "mimo" {
        transcribe_mimo(config, path, token).await
    } else {
        transcribe_openai(config, path, token).await
    }
}

async fn transcribe_openai(config: &AsrConfig, path: &Path, token: &CancellationToken) -> Result<AsrResult, String> {
    let bytes = tokio::fs::read(path).await.map_err(|error| format!("音频打不开：{error}"))?;
    let name = path.file_name().map(|value| value.to_string_lossy().to_string()).unwrap_or_else(|| "audio".into());
    let boundary = format!("----yebian{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::new();
    let field = |body: &mut Vec<u8>, name: &str, value: &str| {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    };
    field(&mut body, "model", &config.model);
    field(&mut body, "response_format", "json");
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let endpoint = format!("{}/audio/transcriptions", config.base_url.trim_end_matches('/'));
    let request = client()?
        .post(&endpoint)
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .header(CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(body);
    let response = tokio::select! {
        _ = token.cancelled() => return Err("已取消".into()),
        response = request.send() => response.map_err(|error| format!("语音识别请求失败：{error}"))?,
    };
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("语音识别 {status}：{}（请求地址 {endpoint}）", compact_error_body(&text)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| format!("语音识别返回无法解析：{error}"))?;
    let content = value
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| value.get("data").and_then(|data| data.get("text")).and_then(Value::as_str))
        .unwrap_or_default()
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("语音识别返回了空文本".into());
    }
    Ok(AsrResult { text: content })
}

async fn transcribe_mimo(config: &AsrConfig, path: &Path, token: &CancellationToken) -> Result<AsrResult, String> {
    let bytes = tokio::fs::read(path).await.map_err(|error| format!("音频打不开：{error}"))?;
    // MiMo 接收 base64 data URL，官方限制 10MB（base64 后）；按原始字节留出编码余量。
    if bytes.len() > MIMO_MAX_AUDIO_BYTES {
        return Err(format!(
            "音频 {:.1} MB，超过 MiMo 单次 10MB base64 限制；请缩短音频或改用 OpenAI 兼容转写",
            bytes.len() as f64 / 1024.0 / 1024.0
        ));
    }
    let extension = path.extension().map(|value| value.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mime = match extension.as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        other => return Err(format!("MiMo 只支持 wav/mp3，当前文件是 {other}；请检查音频预处理")),
    };
    let data = format!("data:{mime};base64,{}", base64_encode(&bytes));
    let body = mimo_body(&config.model, &data);
    let endpoint = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let request = client()?
        .post(&endpoint)
        .header("api-key", &config.api_key)
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    let response = tokio::select! {
        _ = token.cancelled() => return Err("已取消".into()),
        response = request.send() => response.map_err(|error| format!("语音识别请求失败：{error}"))?,
    };
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("语音识别 {status}：{}（请求地址 {endpoint}）", compact_error_body(&text)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| format!("语音识别返回无法解析：{error}"))?;
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("语音识别返回了空文本".into());
    }
    Ok(AsrResult { text: content })
}

fn mimo_body(model: &str, data: &str) -> Value {
    json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [{
                "type": "input_audio",
                "input_audio": { "data": data }
            }]
        }],
        "asr_options": { "language": "auto" },
        "stream": false
    })
}

fn compact_error_body(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "服务没有返回错误内容".into();
    }
    if trimmed.starts_with('<') || trimmed.to_ascii_lowercase().contains("<html") {
        return "接口返回 HTML 错误页，通常是地址、路径或模型名不对".into();
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        for pointer in ["/error/message", "/message", "/error"] {
            if let Some(message) = value.pointer(pointer) {
                if let Some(message) = message.as_str() {
                    return message.chars().take(300).collect();
                }
                if let Some(message) = message.get("message").and_then(Value::as_str) {
                    return message.chars().take(300).collect();
                }
            }
        }
    }
    trimmed.chars().take(300).collect()
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0] as u32;
        let second = chunk.get(1).copied().unwrap_or(0) as u32;
        let third = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (first << 16) | (second << 8) | third;
        output.push(TABLE[((value >> 18) & 63) as usize] as char);
        output.push(TABLE[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 { TABLE[((value >> 6) & 63) as usize] as char } else { '=' });
        output.push(if chunk.len() > 2 { TABLE[(value & 63) as usize] as char } else { '=' });
    }
    output
}

pub async fn search(config: &SearchConfig, query: &str, max: u32, token: &CancellationToken) -> Result<Vec<SearchHit>, String> {
    if config.api_key.trim().is_empty() {
        return Err("还没有配置联网检索 Key".into());
    }
    let (endpoint, body, provider) = match config.provider.as_str() {
        "tavily" => (
            "https://api.tavily.com/search".to_string(),
            json!({ "api_key": config.api_key, "query": query, "max_results": max, "include_answer": false }),
            "tavily",
        ),
        _ => (
            "https://open.bigmodel.cn/api/paas/v4/web_search".to_string(),
            json!({ "search_engine": "search_std", "search_query": query, "count": max }),
            "zhipu",
        ),
    };
    let request = client()?
        .post(endpoint)
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    let response = tokio::select! {
        _ = token.cancelled() => return Err("已取消".into()),
        response = request.send() => response.map_err(|error| format!("联网检索失败：{error}"))?,
    };
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("联网检索 {status}：{}", compact_error_body(&text)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let list = value
        .get("results")
        .or_else(|| value.get("search_result"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut hits = Vec::new();
    for item in list {
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let url = item
            .get("url")
            .or_else(|| item.get("link"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let content = item
            .get("content")
            .or_else(|| item.get("snippet"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !url.is_empty() || !content.is_empty() {
            hits.push(SearchHit { title, url, content });
        }
    }
    if hits.is_empty() && provider == "zhipu" {
        return Err("检索已连通，但没有返回可引用的结果".into());
    }
    Ok(hits)
}

pub fn strip_fence(value: &str) -> String {
    let trimmed = value.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim_start_matches('\n');
    without_open
        .strip_suffix("```")
        .unwrap_or(without_open)
        .trim()
        .to_string()
}

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub provider: String,
    pub protocol: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub retries: u32,
}

#[derive(Debug, Clone)]
pub struct AsrConfig {
    pub engine: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub provider: String,
    pub api_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_json_fence() {
        assert_eq!(strip_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fence("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn builds_cookie_from_set_cookie_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.append(reqwest::header::SET_COOKIE, reqwest::header::HeaderValue::from_static("SESSDATA=abc123; Path=/; HttpOnly"));
        headers.append(reqwest::header::SET_COOKIE, reqwest::header::HeaderValue::from_static("bili_jct=csrf; Path=/"));
        headers.append(reqwest::header::SET_COOKIE, reqwest::header::HeaderValue::from_static("DedeUserID=42; Path=/"));
        let cookie = cookie_from_set_cookie(&headers);
        assert!(cookie.contains("SESSDATA=abc123"));
        assert!(cookie.contains("bili_jct=csrf"));
        assert!(cookie.contains("DedeUserID=42"));
        assert_eq!(cookie_from_set_cookie(&reqwest::header::HeaderMap::new()), "");
    }

    #[test]
    fn redirect_cookie_is_fallback() {
        let cookie = cookies_from_redirect("https://www.bilibili.com/?SESSDATA=s1&bili_jct=j1&DedeUserID=7");
        assert_eq!(cookie, "SESSDATA=s1; bili_jct=j1; DedeUserID=7");
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"hello world"), "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn builds_mimo_audio_message() {
        let body = mimo_body("mimo-v2.5-asr", "data:audio/mpeg;base64,Zm9v");
        assert_eq!(body["model"], "mimo-v2.5-asr");
        assert_eq!(body["messages"][0]["content"][0]["type"], "input_audio");
        assert_eq!(body["messages"][0]["content"][0]["input_audio"]["data"], "data:audio/mpeg;base64,Zm9v");
        assert_eq!(body["asr_options"]["language"], "auto");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn compacts_html_and_json_errors() {
        assert!(compact_error_body("<html><body>404</body></html>").contains("HTML 错误页"));
        assert_eq!(compact_error_body("{\"error\":{\"message\":\"bad key\"}}"), "bad key");
        assert_eq!(compact_error_body("plain failure"), "plain failure");
    }

    #[test]
    fn finds_bvid() {
        assert_eq!(find_bvid("https://www.bilibili.com/video/BV1DTWw6UE1y"), Some("BV1DTWw6UE1y".into()));
        assert_eq!(find_bvid("https://example.com/nope"), None);
    }
}
