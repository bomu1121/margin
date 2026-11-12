use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use md5::{Digest as Md5Digest, Md5};
use reqwest::header::{ACCEPT_ENCODING, AUTHORIZATION, CONTENT_TYPE, REFERER, SET_COOKIE, USER_AGENT};
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

/// 音轨下载是长流式请求，不能套 120 秒总超时；连接仍然快速失败。
fn download_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(900))
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
    #[serde(default)]
    pub owner_mid: i64,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrResult {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

fn bili_transport_error(context: &str, error: &reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "请求超时"
    } else if error.is_connect() {
        "连接失败（DNS/代理/TLS）"
    } else if error.is_body() {
        "响应读取失败"
    } else {
        "网络请求失败"
    };
    format!("{context}{kind}")
}

fn retryable_transport(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

/// B站空间接口偶发 HTTP/2 reset / 短暂连接失败；重试三次，避免一次抖动
/// 把订阅检查推到一小时后的冷却。HTTP 412 / code -352 不在这里重试。
async fn send_bili(
    request: reqwest::RequestBuilder,
    retries: u32,
    context: &str,
) -> Result<reqwest::Response, String> {
    let mut attempt = 0u32;
    loop {
        let Some(cloned) = request.try_clone() else {
            return Err(format!("{context}请求无法重试"));
        };
        match cloned.send().await {
            Ok(response) => return Ok(response),
            Err(error) if attempt < retries && retryable_transport(&error) => {
                eprintln!("{context}{error:?}");
                tokio::time::sleep(Duration::from_millis(250 * (1u64 << attempt))).await;
                attempt += 1;
            }
            Err(error) => return Err(bili_transport_error(context, &error)),
        }
    }
}

async fn bili_json(cookies: &str, url: &str) -> Result<Value, String> {
    let response = send_bili(
        bili_headers(client()?.get(url), cookies),
        2,
        "B站请求失败：",
    )
    .await?;
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
        if code == -101 {
            return Err(format!("B站登录可能已过期，请重新扫码（{message}，code {code}）"));
        }
        return Err(format!("{message}（code {code}）"));
    }
    value.get("data").ok_or_else(|| "B站返回缺少 data".to_string())
}

fn bili_code_auth(value: &Value) -> Result<&Value, String> {
    match bili_code(value) {
        Err(error) if error.contains("code -400") => {
            Err("B站登录可能已过期，请重新扫码（请求错误，code -400）".into())
        }
        other => other,
    }
}

/// B站 WBI 的固定混排表。官方文档把它当成实现细节，但算法本身是稳定的；
/// 这里单独成函数是为了能用官方向量做回归测试。
const WBI_MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9,
    42, 19, 29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0,
    1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

fn wbi_mixin_key(img_key: &str, sub_key: &str) -> String {
    let raw = format!("{img_key}{sub_key}");
    WBI_MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|index| raw.as_bytes().get(*index).copied())
        .map(char::from)
        .take(32)
        .collect()
}

fn wbi_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn wbi_clean(value: &str) -> String {
    // 官方 JS 在签名前会去掉这些字符；服务端只校验签名，因此保留它们
    // 会导致同一组参数算出不同的 w_rid。
    value.chars().filter(|ch| !matches!(ch, '!' | '\'' | '(' | ')' | '*')).collect()
}

fn wbi_signed_query(
    params: &BTreeMap<String, String>,
    img_key: &str,
    sub_key: &str,
    wts: i64,
) -> String {
    let mut pairs: Vec<(String, String)> = params
        .iter()
        .map(|(key, value)| (key.clone(), wbi_encode(&wbi_clean(value))))
        .collect();
    pairs.push(("wts".into(), wts.to_string()));
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let query = pairs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let mixin = wbi_mixin_key(img_key, sub_key);
    let mut hasher = Md5::new();
    hasher.update(format!("{query}{mixin}").as_bytes());
    let digest = hasher.finalize();
    let w_rid = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    format!("{query}&w_rid={w_rid}")
}

#[derive(Clone, Debug)]
struct WbiKeys {
    img_key: String,
    sub_key: String,
}

static WBI_CACHE: OnceLock<tokio::sync::Mutex<Option<(i64, WbiKeys)>>> = OnceLock::new();
static BUVID_CACHE: OnceLock<tokio::sync::Mutex<Option<(i64, String)>>> = OnceLock::new();

async fn wbi_keys(cookies: &str) -> Result<WbiKeys, String> {
    let cache = WBI_CACHE.get_or_init(|| tokio::sync::Mutex::new(None));
    let now = chrono::Utc::now().timestamp();
    {
        let guard = cache.lock().await;
        if let Some((expires, keys)) = guard.as_ref() {
            if *expires > now {
                return Ok(keys.clone());
            }
        }
    }
    let value = bili_json(cookies, "https://api.bilibili.com/x/web-interface/nav").await?;
    let data = bili_code_auth(&value)?;
    let image = data
        .get("wbi_img")
        .and_then(|item| item.get("img_url"))
        .and_then(Value::as_str)
        .ok_or_else(|| "B站没有返回 WBI 图片 key".to_string())?;
    let sub = data
        .get("wbi_img")
        .and_then(|item| item.get("sub_url"))
        .and_then(Value::as_str)
        .ok_or_else(|| "B站没有返回 WBI 子 key".to_string())?;
    let stem = |url: &str| {
        url.rsplit('/')
            .next()
            .unwrap_or(url)
            .split('.')
            .next()
            .unwrap_or(url)
            .to_string()
    };
    let keys = WbiKeys { img_key: stem(image), sub_key: stem(sub) };
    if keys.img_key.is_empty() || keys.sub_key.is_empty() {
        return Err("B站 WBI key 为空".into());
    }
    *cache.lock().await = Some((now + 6 * 3600, keys.clone()));
    Ok(keys)
}

/// Space 接口对没有 buvid3 的请求更敏感。登录 Cookie 里通常没有这两个值，
/// 用 fingerprint/spi 补齐；缓存 6 小时，避免每次轮询都多打一个请求。
async fn cookies_with_buvid(cookies: &str, mid: i64) -> Result<String, String> {
    if cookies.contains("buvid3=") {
        return Ok(cookies.to_string());
    }
    let cache = BUVID_CACHE.get_or_init(|| tokio::sync::Mutex::new(None));
    let now = chrono::Utc::now().timestamp();
    let extra = {
        let guard = cache.lock().await;
        guard.as_ref().filter(|(expires, _)| *expires > now).map(|(_, value)| value.clone())
    };
    let extra = match extra {
        Some(value) => value,
        None => {
            let value = bili_json(cookies, "https://api.bilibili.com/x/frontend/finger/spi").await?;
            let data = bili_code(&value)?;
            let b3 = data.get("b_3").and_then(Value::as_str).unwrap_or_default();
            let b4 = data.get("b_4").and_then(Value::as_str).unwrap_or_default();
            if b3.is_empty() {
                return Err("B站没有返回 buvid3".into());
            }
            let value = if b4.is_empty() {
                format!("buvid3={b3}")
            } else {
                format!("buvid3={b3}; buvid4={b4}")
            };
            *cache.lock().await = Some((now + 6 * 3600, value.clone()));
            value
        }
    };
    let _ = mid;
    Ok(if cookies.trim().is_empty() {
        extra
    } else {
        format!("{cookies}; {extra}")
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BiliFollowing {
    pub mid: i64,
    pub name: String,
    #[serde(default)]
    pub face: String,
    #[serde(default)]
    pub sign: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiliFollowingPage {
    pub items: Vec<BiliFollowing>,
    pub page: u32,
    pub has_more: bool,
    pub total: i64,
}

/// 关注列表是“选 UP”最自然的入口。这个接口不需要 WBI，但需要登录 Cookie；
/// 只读取第一页 50 条，更多由用户点“加载更多”。
pub async fn fetch_bili_followings(
    cookies: &str,
    vmid: i64,
    page: u32,
) -> Result<BiliFollowingPage, String> {
    let page = page.max(1);
    let url = format!(
        "https://api.bilibili.com/x/relation/followings?vmid={vmid}&pn={page}&ps=50&order=desc&order_type=attention"
    );
    let referer = format!("https://space.bilibili.com/{vmid}/fans/follow");
    let response = send_bili(
        bili_headers(client()?.get(&url), cookies).header(REFERER, referer),
        2,
        "B站关注列表请求失败：",
    )
    .await?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        if status.as_u16() == 412 {
            return Err("B站风控拦截（HTTP 412）".into());
        }
        return Err(format!("B站关注列表 {status}：{}", text.chars().take(300).collect::<String>()));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| format!("B站关注列表无法解析：{error}"))?;
    let data = match bili_code_auth(&value) {
        Ok(data) => data,
        Err(error) if error.contains("code -352") => return Err("B站风控拦截（code -352）".into()),
        Err(error) => return Err(error),
    };
    let total = data.get("total").and_then(Value::as_i64).unwrap_or(0);
    let items = data
        .get("list")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let mid = item.get("mid").and_then(Value::as_i64)?;
                    Some(BiliFollowing {
                        mid,
                        name: item.get("uname").and_then(Value::as_str).unwrap_or_default().to_string(),
                        face: item
                            .get("face")
                            .and_then(Value::as_str)
                            .map(|value| if value.starts_with("//") { format!("https:{value}") } else { value.to_string() })
                            .unwrap_or_default(),
                        sign: item.get("sign").and_then(Value::as_str).unwrap_or_default().to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(BiliFollowingPage {
        has_more: (page as i64 * 50) < total && !items.is_empty(),
        page,
        total,
        items,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BiliUpProfile {
    pub mid: i64,
    pub name: String,
    pub face: String,
    pub fans: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiliUpVideoPage {
    pub items: Vec<BiliVideoRef>,
    pub page: u32,
    pub has_more: bool,
    pub total: i64,
}

/// 从用户输入的 UID / 空间链接里取 mid。只接受 B 站空间链接和纯数字，
/// 不做泛化的 URL 抓取。
pub fn parse_up_mid(input: &str) -> Option<i64> {
    let trimmed = input.trim();
    if let Ok(mid) = trimmed.parse::<i64>() {
        return (mid > 0).then_some(mid);
    }
    let lower = trimmed.to_ascii_lowercase();
    for marker in ["space.bilibili.com/", "bilibili.com/space/"] {
        if let Some(index) = lower.find(marker) {
            let digits = lower[index + marker.len()..]
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            if let Ok(mid) = digits.parse::<i64>() {
                return (mid > 0).then_some(mid);
            }
        }
    }
    for marker in ["mid=", "uid="] {
        if let Some(index) = lower.find(marker) {
            let digits = lower[index + marker.len()..]
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            if let Ok(mid) = digits.parse::<i64>() {
                return (mid > 0).then_some(mid);
            }
        }
    }
    None
}

pub async fn fetch_bili_up_profile(mid: i64) -> Result<BiliUpProfile, String> {
    let url = format!("https://api.bilibili.com/x/web-interface/card?mid={mid}");
    let value = bili_json("", &url).await?;
    let data = bili_code(&value)?;
    let card = data.get("card").ok_or_else(|| "B站没有返回 UP 主资料".to_string())?;
    let name = card.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    if name.is_empty() {
        return Err("没有找到这个 UP 主".into());
    }
    Ok(BiliUpProfile {
        mid: card.get("mid").and_then(Value::as_i64).unwrap_or(mid),
        name,
        face: card.get("face").and_then(Value::as_str).unwrap_or_default().to_string(),
        fans: card.get("fans").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn parse_bili_length(value: &str) -> i64 {
    let mut total = 0i64;
    for part in value.split(':') {
        let Ok(number) = part.trim().parse::<i64>() else { return 0 };
        total = total * 60 + number;
    }
    total
}

pub async fn fetch_bili_up_videos(
    cookies: &str,
    mid: i64,
    page: u32,
) -> Result<BiliUpVideoPage, String> {
    let page = page.max(1);
    let keys = wbi_keys(cookies).await?;
    let cookies = cookies_with_buvid(cookies, mid).await?;
    let mut params = BTreeMap::new();
    params.insert("mid".into(), mid.to_string());
    params.insert("pn".into(), page.to_string());
    params.insert("ps".into(), "50".into());
    params.insert("order".into(), "pubdate".into());
    params.insert("platform".into(), "web".into());
    // web_location 是空间页当前使用的来源标记；缺失时更容易被风控。
    params.insert("web_location".into(), "1550101".into());
    let query = wbi_signed_query(&params, &keys.img_key, &keys.sub_key, chrono::Utc::now().timestamp());
    let url = format!("https://api.bilibili.com/x/space/wbi/arc/search?{query}");
    let referer = format!("https://space.bilibili.com/{mid}/video");
    let response = send_bili(
        client()?
            .get(&url)
            .header(USER_AGENT, BILI_UA)
            .header(REFERER, referer)
            .header("Cookie", &cookies)
            .header("Origin", "https://space.bilibili.com"),
        2,
        "B站空间请求失败：",
    )
    .await?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        if status.as_u16() == 412 {
            return Err("B站风控拦截（HTTP 412），已暂停这个 UP 的检查".into());
        }
        return Err(format!("B站空间接口 {status}：{}", text.chars().take(300).collect::<String>()));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| format!("B站空间返回无法解析：{error}"))?;
    let data = match bili_code(&value) {
        Ok(data) => data,
        Err(error) if error.contains("code -352") => {
            return Err("B站风控拦截（code -352），已暂停这个 UP 的检查".into());
        }
        Err(error) if error.contains("code -101") => {
            return Err("B站登录已过期，请重新扫码".into());
        }
        Err(error) => return Err(error),
    };
    let list = data.get("list").cloned().unwrap_or(Value::Null);
    let vlist = list.get("vlist").and_then(Value::as_array).cloned().unwrap_or_default();
    let items = vlist
        .iter()
        .filter_map(|item| {
            let bvid = item.get("bvid").and_then(Value::as_str)?.to_string();
            Some(BiliVideoRef {
                bvid,
                title: item.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
                author: item.get("author").and_then(Value::as_str).unwrap_or_default().to_string(),
                duration: item
                    .get("length")
                    .and_then(Value::as_str)
                    .map(parse_bili_length)
                    .unwrap_or(0),
                pubdate: item.get("created").and_then(Value::as_i64).unwrap_or(0),
                cover: item
                    .get("pic")
                    .and_then(Value::as_str)
                    .map(|value| if value.starts_with("//") { format!("https:{value}") } else { value.to_string() })
                    .unwrap_or_default(),
                cid: 0,
            })
        })
        .collect::<Vec<_>>();
    let total = list.get("page").and_then(|page| page.get("count")).and_then(Value::as_i64).unwrap_or(0);
    let reported_page = list
        .get("page")
        .and_then(|page| page.get("pn"))
        .and_then(Value::as_i64)
        .unwrap_or(page as i64)
        .max(1) as u32;
    Ok(BiliUpVideoPage {
        has_more: items.len() >= 50 && (reported_page as i64 * 50) < total,
        items,
        page: reported_page,
        total,
    })
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
    bili_code(&value)
        .map(|data| data.clone())
        .map_err(|error| {
            if error.contains("code -400") {
                "视频不存在或链接无效（B站 code -400）".into()
            } else {
                error
            }
        })
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
        owner_mid: data
            .get("owner")
            .and_then(|owner| owner.get("mid"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
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
        // ASR 只需要可懂度，选最低码率音轨能显著缩短下载；B站 dash.audio
        // 常同时给 64k/132k/192k，first() 会优先拿最大的那条。
        .and_then(|items| {
            items.iter().min_by_key(|item| {
                item.get("bandwidth")
                    .and_then(Value::as_u64)
                    .filter(|value| *value > 0)
                    .unwrap_or_else(|| {
                        item.get("id").and_then(Value::as_u64).unwrap_or(u64::MAX / 2) * 1000
                    })
            })
        });
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
    let request = bili_headers(download_client()?.get(url), "")
        // 音轨是二进制流；要求 identity，避免 CDN 返回压缩体再由 reqwest 解码。
        .header(ACCEPT_ENCODING, "identity");
    let response = tokio::select! {
        _ = token.cancelled() => return Err("已取消".into()),
        response = request.send() => response.map_err(|error| format!("下载 B站音轨失败：{error}"))?,
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
    let data = bili_code_auth(&value)?;
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
    let data = bili_code_auth(&value)?;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiliVideoPage {
    pub items: Vec<BiliVideoRef>,
    pub page: u32,
    pub has_more: bool,
    pub total: i64,
}

pub async fn fetch_bili_folder_videos(cookies: &str, media_id: i64, page: u32) -> Result<BiliVideoPage, String> {
    let page = page.max(1);
    let url = format!(
        "https://api.bilibili.com/x/v3/fav/resource/list?media_id={media_id}&pn={page}&ps=20&platform=web"
    );
    let value = bili_json(cookies, &url).await?;
    let data = bili_code_auth(&value)?;
    let medias = data
        .get("medias")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let items = medias
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
        .collect();
    Ok(BiliVideoPage {
        items,
        page,
        has_more: data.get("has_more").and_then(Value::as_bool).unwrap_or(false),
        total: data
            .get("info")
            .and_then(|info| info.get("media_count"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    })
}

pub async fn fetch_bili_watch_later(cookies: &str) -> Result<Vec<BiliVideoRef>, String> {
    let value = bili_json(cookies, "https://api.bilibili.com/x/v2/history/toview").await?;
    let data = bili_code_auth(&value)?;
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
    const MAX_ITEMS: usize = 100;
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut max = 0i64;
    let mut view_at = 0i64;
    // B站历史接口一页固定约 20 条，用 cursor.max / cursor.view_at 翻页。
    for _ in 0..6 {
        let url = format!(
            "https://api.bilibili.com/x/web-interface/history/cursor?max={max}&view_at={view_at}&business=archive&ps=20"
        );
        let value = bili_json(cookies, &url).await?;
        let data = bili_code_auth(&value)?;
        let list = data.get("list").and_then(Value::as_array).cloned().unwrap_or_default();
        for item in list {
            let Some(history) = item.get("history") else { continue };
            if history.get("business").and_then(Value::as_str) != Some("archive") {
                continue;
            }
            let Some(bvid) = history.get("bvid").and_then(Value::as_str) else { continue };
            if !seen.insert(bvid.to_string()) {
                continue;
            }
            items.push(BiliVideoRef {
                bvid: bvid.to_string(),
                title: item.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
                author: item.get("author_name").and_then(Value::as_str).unwrap_or_default().to_string(),
                duration: item.get("duration").and_then(Value::as_i64).unwrap_or(0),
                pubdate: item.get("pubdate").and_then(Value::as_i64).unwrap_or(0),
                cover: item.get("cover").and_then(Value::as_str).unwrap_or_default().to_string(),
                cid: history.get("cid").and_then(Value::as_i64).unwrap_or(0),
            });
            if items.len() >= MAX_ITEMS {
                break;
            }
        }
        if items.len() >= MAX_ITEMS {
            break;
        }
        let cursor = data.get("cursor");
        let next_max = cursor.and_then(|value| value.get("max")).and_then(Value::as_i64).unwrap_or(0);
        let next_view = cursor.and_then(|value| value.get("view_at")).and_then(Value::as_i64).unwrap_or(0);
        if (next_max == 0 && next_view == 0) || (next_max == max && next_view == view_at) {
            break;
        }
        max = next_max;
        view_at = next_view;
    }
    Ok(items)
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
    let response = send_bili(
        bili_headers(client()?.get(&url), ""),
        2,
        "B站登录轮询失败：",
    )
    .await?;
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
/// MiMo 单次 8K 上下文 / 2K 输出，长音频会丢中间段；按 60 秒切分。
pub const MIMO_PART_SECONDS: u32 = 60;

pub async fn transcribe(config: &AsrConfig, path: &Path, token: &CancellationToken) -> Result<AsrResult, String> {
    transcribe_single(config, path, token).await
}

/// Transcribe one already-prepared file. Long MiMo files are split by
/// `media::prepare_asr_parts` before this call so the pipeline can checkpoint
/// each segment independently.
pub async fn transcribe_single(config: &AsrConfig, path: &Path, token: &CancellationToken) -> Result<AsrResult, String> {
    transcribe_single_inner(config, path, token, false).await
}

/// Chunked ASR: a silent 60s part is allowed to return empty text; the
/// pipeline's coverage gate judges the joined result.
pub async fn transcribe_part(config: &AsrConfig, path: &Path, token: &CancellationToken) -> Result<AsrResult, String> {
    transcribe_single_inner(config, path, token, true).await
}

async fn transcribe_single_inner(
    config: &AsrConfig,
    path: &Path,
    token: &CancellationToken,
    allow_empty: bool,
) -> Result<AsrResult, String> {
    if config.api_key.trim().is_empty() {
        return Err("还没有配置语音识别 API Key".into());
    }
    if config.engine == "mimo" {
        let size = tokio::fs::metadata(path).await.map_err(|error| format!("音频打不开：{error}"))?.len();
        if size > MIMO_MAX_AUDIO_BYTES as u64 {
            return Err(format!(
                "音频 {:.1} MB，超过 MiMo 单次 10MB base64 限制；分段预处理没有生效",
                size as f64 / 1024.0 / 1024.0
            ));
        }
        let text = mimo_request(config, path, token, allow_empty).await?;
        return Ok(AsrResult { text });
    }
    transcribe_openai(config, path, token, allow_empty).await
}

async fn transcribe_openai(
    config: &AsrConfig,
    path: &Path,
    token: &CancellationToken,
    allow_empty: bool,
) -> Result<AsrResult, String> {
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
    if content.is_empty() && !allow_empty {
        return Err("语音识别返回了空文本".into());
    }
    Ok(AsrResult { text: content })
}

async fn mimo_request(
    config: &AsrConfig,
    path: &Path,
    token: &CancellationToken,
    allow_empty: bool,
) -> Result<String, String> {
    let bytes = tokio::fs::read(path).await.map_err(|error| format!("音频打不开：{error}"))?;
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
    if content.is_empty() && !allow_empty {
        return Err("语音识别返回了空文本".into());
    }
    Ok(content)
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
        "deepseek" => (
            "https://api.deepseek.com/v1/tools/search".to_string(),
            json!({
                "query": query.chars().take(128).collect::<String>(),
                "count": max.clamp(1, 20),
            }),
            "deepseek",
        ),
        _ => (
            "https://open.bigmodel.cn/api/paas/v4/web_search".to_string(),
            json!({ "search_engine": "search_std", "search_query": query, "count": max }),
            "zhipu",
        ),
    };
    let mut attempt = 0u32;
    let (status, text) = loop {
        let request = client()?
            .post(endpoint.clone())
            .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
            .header(CONTENT_TYPE, "application/json")
            .json(&body);
        let response = tokio::select! {
            _ = token.cancelled() => return Err("已取消".into()),
            response = request.send() => response.map_err(|error| format!("联网检索失败：{error}"))?,
        };
        let status = response.status();
        let text = response.text().await.map_err(|error| error.to_string())?;
        // DeepSeek Search 账号级限流是 5 QPS；429/408 是并发抖动，不是参数错误。
        // 只重试一次，避免把真正的配额耗尽拖成长时间等待。
        if (status.as_u16() == 429 || status.as_u16() == 408) && attempt == 0 {
            attempt += 1;
            tokio::time::sleep(Duration::from_millis(600)).await;
            if token.is_cancelled() {
                return Err("已取消".into());
            }
            continue;
        }
        break (status, text);
    };
    if !status.is_success() {
        return Err(format!("联网检索 {status}：{}", compact_error_body(&text)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let hits = parse_search_hits(&value, provider);
    if hits.is_empty() && (provider == "zhipu" || provider == "deepseek") {
        return Err("检索已连通，但没有返回可引用的结果".into());
    }
    Ok(hits)
}

fn parse_search_hits(value: &Value, provider: &str) -> Vec<SearchHit> {
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
        let content = if provider == "deepseek" {
            // DeepSeek 的 summary 是面向模型的长摘要，比给界面看的 snippet 更适合核查。
            item.get("summary")
                .or_else(|| item.get("snippet"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        } else {
            item.get("content")
                .or_else(|| item.get("snippet"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        if !url.is_empty() || !content.is_empty() {
            hits.push(SearchHit { title, url, content });
        }
    }
    hits
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
    pub ffmpeg_path: String,
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

    #[tokio::test]
    #[ignore]
    async fn live_fetch_up_431055518_videos() {
        let cookies = std::env::var("BILI_COOKIES").expect("BILI_COOKIES");
        let page = fetch_bili_up_videos(&cookies, 431_055_518, 1).await.expect("up videos");
        for item in &page.items {
            println!("{}\t{}s\t{}", item.bvid, item.duration, item.title);
        }
        assert!(!page.items.is_empty());
    }

    #[test]
    fn parses_deepseek_and_zhipu_search_shapes() {
        let deepseek = json!({
            "results": [
                {"title": "A", "url": "https://example.com/a", "summary": "长摘要", "snippet": "短摘要"},
                {"title": "B", "url": "https://example.com/b", "snippet": "只有短摘要"},
                {"title": "empty"}
            ]
        });
        let hits = parse_search_hits(&deepseek, "deepseek");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].content, "长摘要");
        assert_eq!(hits[1].content, "只有短摘要");
        let zhipu = json!({
            "search_result": [
                {"title": "C", "link": "https://example.com/c", "content": "正文"}
            ]
        });
        let hits = parse_search_hits(&zhipu, "zhipu");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/c");
        assert_eq!(hits[0].content, "正文");
    }

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
    fn tells_user_to_relogin_on_auth_codes() {
        let value = json!({"code": -101, "message": "账号未登录"});
        let error = bili_code(&value).unwrap_err();
        assert!(error.contains("重新扫码"), "{error}");
        let value = json!({"code": -400, "message": "请求错误"});
        let error = bili_code_auth(&value).unwrap_err();
        assert!(error.contains("重新扫码"), "{error}");
        assert!(bili_code(&value).is_err());
    }

    #[test]
    fn finds_bvid() {
        assert_eq!(find_bvid("https://www.bilibili.com/video/BV1DTWw6UE1y"), Some("BV1DTWw6UE1y".into()));
        assert_eq!(find_bvid("https://example.com/nope"), None);
    }

    #[test]
    fn wbi_mixin_key_matches_published_derivation() {
        // 公开文档一致的 mixin_key 推导结果（img_key + sub_key 按固定表重排后取 32 位）。
        assert_eq!(
            wbi_mixin_key(
                "7cd084941338484aae1ad9425b84077c",
                "4932caff0ff746eab6f01bf08b70ac45",
            ),
            "ea1db124af3c7062474693fa704f4ff8"
        );
    }

    #[test]
    fn wbi_signing_is_stable_against_independent_implementation() {
        // 这个期望值由独立的 Python/Node MD5 实现按公开算法算出；网上流传的
        // `bb97...` 与它自己给出的参数/算法对不上，因此没有把它当断言。
        // 真实空间接口已用同一算法请求成功（见 docs/experiment-log.txt）。
        let img_key = "7cd084941338484aae1ad9425b84077c";
        let sub_key = "4932caff0ff746eab6f01bf08b70ac45";
        let mut params = BTreeMap::new();
        params.insert("foo".into(), "114".into());
        params.insert("bar".into(), "514".into());
        params.insert("baz".into(), "1919810".into());
        assert_eq!(
            wbi_signed_query(&params, img_key, sub_key, 1684805578),
            "bar=514&baz=1919810&foo=114&wts=1684805578&w_rid=c9864ec9f22928db4a94a49ae7986c59"
        );
    }

    #[test]
    fn wbi_cleans_forbidden_chars_before_signing() {
        let mut params = BTreeMap::new();
        params.insert("keyword".into(), "a!b'c(d)e*f".into());
        let query = wbi_signed_query(&params, "7cd084941338484aae1ad9425b84077c", "4932caff0ff746eab6f01bf08b70ac45", 1);
        // 值里的 !'()* 会被移除；其余字符按 RFC 3986 编码。
        assert!(query.starts_with("keyword=abcdef&wts=1&w_rid="), "{query}");
    }

    #[test]
    fn parses_up_mid_from_input_forms() {
        assert_eq!(parse_up_mid("12345"), Some(12345));
        assert_eq!(parse_up_mid("https://space.bilibili.com/946974/video"), Some(946974));
        assert_eq!(parse_up_mid("space.bilibili.com/2?spm_id_from=333"), Some(2));
        assert_eq!(parse_up_mid("https://m.bilibili.com/space/777?share_source=copy"), Some(777));
        assert_eq!(parse_up_mid("mid=777"), Some(777));
        assert_eq!(parse_up_mid("uid=888"), Some(888));
        assert_eq!(parse_up_mid("https://www.bilibili.com/video/BV1xx"), None);
        assert_eq!(parse_up_mid("0"), None);
    }
}
