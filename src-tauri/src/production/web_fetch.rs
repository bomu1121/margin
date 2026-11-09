//! 受控网页/PDF 正文抓取，供溯源核查使用。
//!
//! 这里不是通用爬虫：只抓单页、跟随有限重定向、限制大小和超时，并拒绝
//! 内网/回环地址。抓到正文只用于核查引文，不改变来源 URL。

use std::net::IpAddr;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_TYPE, LOCATION, USER_AGENT};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use url::Url;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_PDF_BYTES: usize = 4 * 1024 * 1024;
const MAX_PDF_EXTRACT_SECS: u64 = 20;
const MAX_REDIRECTS: usize = 5;
const MAX_TEXT_CHARS: usize = 20_000;
const FETCH_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedPage {
    pub url: String,
    pub title: String,
    pub text: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent(FETCH_UA)
        .build()
        .map_err(|error| error.to_string())
}

pub async fn fetch(url: &str) -> Result<FetchedPage, String> {
    let mut current = Url::parse(url.trim()).map_err(|error| format!("来源 URL 无效：{error}"))?;
    let client = client()?;
    for _ in 0..=MAX_REDIRECTS {
        ensure_public_url(&current).await?;
        let response = client
            .get(current.clone())
            .header(ACCEPT, "text/html,application/xhtml+xml,application/pdf,text/plain;q=0.9,*/*;q=0.5")
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(|error| format!("抓取来源失败：{error}"))?;
        let status = response.status();
        if status.is_redirection() {
            let Some(location) = response.headers().get(LOCATION).and_then(|value| value.to_str().ok()) else {
                return Err(format!("来源返回 {status}，但没有 Location"));
            };
            current = current.join(location).map_err(|error| format!("来源重定向地址无效：{error}"))?;
            continue;
        }
        if !status.is_success() {
            return Err(format!("来源返回 HTTP {status}"));
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if let Some(length) = response.content_length() {
            if length as usize > MAX_BYTES {
                return Err(format!("来源超过 {} MB 上限", MAX_BYTES / 1024 / 1024));
            }
        }
        let bytes = read_limited(response).await?;
        let final_url = current.to_string();
        let is_pdf = content_type.contains("application/pdf") || bytes.starts_with(b"%PDF");
        if is_pdf {
            if bytes.len() > MAX_PDF_BYTES {
                return Err(format!("PDF 超过 {} MB 提取上限", MAX_PDF_BYTES / 1024 / 1024));
            }
            let pdf_bytes = bytes.clone();
            let extracted = tokio::time::timeout(
                Duration::from_secs(MAX_PDF_EXTRACT_SECS),
                tokio::task::spawn_blocking(move || pdf_extract::extract_text_from_mem(&pdf_bytes)),
            )
            .await;
            let raw = match extracted {
                Ok(Ok(Ok(text))) => text,
                Ok(Ok(Err(error))) => return Err(format!("PDF 正文提取失败：{error}")),
                Ok(Err(error)) => return Err(format!("PDF 提取任务失败：{error}")),
                Err(_) => return Err(format!("PDF 正文提取超时（{}s）", MAX_PDF_EXTRACT_SECS)),
            };
            let text = normalize_text(&raw);
            if text.is_empty() {
                return Err("PDF 没有可提取的文字（可能是扫描件）".into());
            }
            return Ok(FetchedPage {
                url: final_url,
                title: current.path().rsplit('/').next().unwrap_or("PDF").to_string(),
                text: text.chars().take(MAX_TEXT_CHARS).collect(),
            });
        }
        if !content_type.is_empty()
            && !content_type.contains("text/html")
            && !content_type.contains("application/xhtml+xml")
            && !content_type.contains("text/plain")
        {
            return Err(format!("来源内容类型不支持：{content_type}"));
        }
        let html = String::from_utf8_lossy(&bytes);
        let (title, text) = extract_html(&html);
        if text.is_empty() {
            return Err("网页没有提取到正文".into());
        }
        return Ok(FetchedPage {
            url: final_url,
            title: if title.is_empty() { current.to_string() } else { title },
            text: text.chars().take(MAX_TEXT_CHARS).collect(),
        });
    }
    Err("来源重定向次数过多".into())
}

async fn ensure_public_url(url: &Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("只允许 http/https 来源".into());
    }
    let host = url.host_str().ok_or_else(|| "来源没有主机名".to_string())?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err("拒绝抓取内网/回环地址".into());
        }
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("来源域名解析失败：{error}"))?;
    let mut any = false;
    for address in addresses {
        any = true;
        if is_blocked_ip(address.ip()) {
            return Err("来源解析到内网/回环地址，已拒绝抓取".into());
        }
    }
    if !any {
        return Err("来源域名没有解析结果".into());
    }
    Ok(())
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(value) => {
            value.is_private()
                || value.is_loopback()
                || value.is_link_local()
                || value.is_unspecified()
                || value.is_multicast()
                || value.is_broadcast()
        }
        IpAddr::V6(value) => {
            if let Some(mapped) = value.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(mapped));
            }
            let segments = value.segments();
            value.is_loopback()
                || value.is_unspecified()
                || value.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
        }
    }
}

async fn read_limited(response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("读取来源失败：{error}"))?;
        if output.len() + chunk.len() > MAX_BYTES {
            return Err(format!("来源超过 {} MB 上限", MAX_BYTES / 1024 / 1024));
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn remove_tag_blocks(input: &str, tag: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0usize;
    while let Some(relative_start) = lower[cursor..].find(&open) {
        let start = cursor + relative_start;
        output.push_str(&input[cursor..start]);
        let Some(relative_end) = lower[start..].find(&close) else {
            cursor = input.len();
            break;
        };
        let end = start + relative_end;
        let Some(relative_gt) = lower[end..].find('>') else {
            cursor = input.len();
            break;
        };
        cursor = end + relative_gt + 1;
    }
    output.push_str(&input[cursor..]);
    output
}

pub(crate) fn extract_html(html: &str) -> (String, String) {
    let mut cleaned = html.to_string();
    for tag in ["script", "style", "noscript", "svg", "template"] {
        cleaned = remove_tag_blocks(&cleaned, tag);
    }
    let document = scraper::Html::parse_document(&cleaned);
    let title = scraper::Selector::parse("title")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|node| normalize_text(&node.text().collect::<Vec<_>>().join(" ")))
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let selector = scraper::Selector::parse(".mw-parser-output, article, main, [role=main], .article, .content, #content, body").ok();
    let text = selector
        .and_then(|selector| document.select(&selector).next())
        .map(|node| normalize_text(&node.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_default();
    (title, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_loopback_and_link_local_addresses() {
        for ip in ["127.0.0.1", "10.0.0.1", "172.16.0.1", "192.168.1.1", "169.254.1.1", "::1", "fe80::1", "fc00::1"] {
            assert!(is_blocked_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(!is_blocked_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_blocked_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    #[ignore]
    async fn live_fetches_aspe_pdf_text() {
        let page = fetch("https://aspe.hhs.gov/sites/default/files/documents/423242373901e0901d36fc56c3d55a21/CCDF%20Eligibility%20and%20Receipt_FY22.pdf")
            .await
            .expect("ASPE fetch");
        assert!(page.text.contains("11.8 million children"), "{}", &page.text[..page.text.len().min(500)]);
    }

    #[test]
    fn extracts_main_html_text_and_strips_scripts() {
        let html = r#"<html><head><title>测试标题</title><script>alert(1)</script></head><body><nav>导航</nav><main><h1>正文</h1><p>11.8 million children were eligible.</p></main></body></html>"#;
        let (title, text) = extract_html(html);
        assert_eq!(title, "测试标题");
        assert!(text.contains("11.8 million children were eligible."));
        assert!(!text.contains("alert(1)"));
    }
}
