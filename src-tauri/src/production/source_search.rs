//! 史料检索助手：只做公开、可链接的发现，不绕过登录/付费墙。
//!
//! 目前自动检索 Wikisource 与 Internet Archive；ctext、中华古籍资源库、
//! 一史馆、全国报刊索引、HathiTrust 等作为外部检索链接。
//! 检索结果只是候选；**不要从 snippet 直接生成 citation**。未来接自动入库时，
//! 必须先 fetch 原文、建立 passage，并做逐字对齐，才能写 citation。
//! 当前 UI 不暴露这个入口（史料库改为自动溯源结果索引）。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSearchHit {
    pub provider: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSearchLink {
    pub provider: String,
    pub label: String,
    pub url: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSearchResult {
    pub hits: Vec<SourceSearchHit>,
    pub links: Vec<SourceSearchLink>,
    pub notes: Vec<String>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .user_agent("Yebian/0.1 (local history research; source discovery)")
        .build()
        .map_err(|error| error.to_string())
}

fn encode(value: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", value)
        .finish()
        .trim_start_matches("q=")
        .to_string()
}

fn wikisource_url(title: &str) -> String {
    // MediaWiki 的标题可以带 "/"（如 明史/卷三百八）；路径段要分别编码，
    // 不能把 "/" 整体编码成 %2F，否则部分部署会解析成另一个标题。
    let path = title
        .split('/')
        .map(|part| encode(part).replace('+', "%20"))
        .collect::<Vec<_>>()
        .join("/");
    format!("https://zh.wikisource.org/wiki/{path}")
}

fn network_error(error: reqwest::Error) -> String {
    // reqwest 会把完整 URL（含长 query）塞进 Display；UI 只需要原因。
    let text = error.to_string();
    text.split(" for url (").next().unwrap_or(&text).trim().to_string()
}

fn strip_html(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>().join("、"),
        _ => String::new(),
    }
}

pub(crate) async fn search_wikisource(query: &str, limit: usize) -> Result<Vec<SourceSearchHit>, String> {
    let endpoint = format!(
        "https://zh.wikisource.org/w/api.php?action=query&list=search&srsearch={}&srlimit={}&format=json&utf8=1",
        encode(query),
        limit
    );
    let response = client()?.get(endpoint).send().await.map_err(network_error)?;
    if !response.status().is_success() {
        return Err(format!("Wikisource HTTP {}", response.status()));
    }
    let value: Value = response.json().await.map_err(|error| error.to_string())?;
    let hits = value
        .get("query")
        .and_then(|item| item.get("search"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let title = item.get("title").and_then(Value::as_str)?.to_string();
                    if title.is_empty() {
                        return None;
                    }
                    let url = wikisource_url(&title);
                    let snippet = strip_html(item.get("snippet").and_then(Value::as_str).unwrap_or(""));
                    Some(SourceSearchHit {
                        provider: "Wikisource".into(),
                        title,
                        url,
                        snippet,
                        source_type: "primary_candidate".into(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(hits)
}

async fn search_internet_archive(query: &str, limit: usize) -> Result<Vec<SourceSearchHit>, String> {
    let endpoint = format!(
        "https://archive.org/advancedsearch.php?q={}&fl[]=identifier&fl[]=title&fl[]=creator&fl[]=year&rows={}&page=1&output=json",
        encode(query),
        limit
    );
    let response = client()?.get(endpoint).send().await.map_err(network_error)?;
    if !response.status().is_success() {
        return Err(format!("Internet Archive HTTP {}", response.status()));
    }
    let value: Value = response.json().await.map_err(|error| error.to_string())?;
    let hits = value
        .get("response")
        .and_then(|item| item.get("docs"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let identifier = item.get("identifier").and_then(Value::as_str)?;
                    if identifier.is_empty() {
                        return None;
                    }
                    let title = value_text(item.get("title"));
                    let creator = value_text(item.get("creator"));
                    let year = value_text(item.get("year"));
                    let snippet = [creator, year].into_iter().filter(|value| !value.is_empty()).collect::<Vec<_>>().join(" · ");
                    Some(SourceSearchHit {
                        provider: "Internet Archive".into(),
                        title: if title.is_empty() { identifier.to_string() } else { title },
                        url: format!("https://archive.org/details/{identifier}"),
                        snippet,
                        source_type: "scan_candidate".into(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(hits)
}

fn external_links(query: &str) -> Vec<SourceSearchLink> {
    let encoded = encode(query);
    vec![
        SourceSearchLink {
            provider: "ctext".into(),
            label: "ctext 全文".into(),
            url: format!("https://ctext.org/searchbooks.pl?if=gb&searchu={encoded}"),
            note: "转录+扫描；引用前核对扫描页".into(),
        },
        SourceSearchLink {
            provider: "nlc".into(),
            label: "中华古籍资源库".into(),
            url: "https://read.nlc.cn/".into(),
            note: "国图影像库；按书名/作者检索，不是全文 OCR".into(),
        },
        SourceSearchLink {
            provider: "shuge".into(),
            label: "书格".into(),
            url: "https://www.shuge.org/".into(),
            note: "高清影像/PDF；适合看版本，不是全文检索".into(),
        },
        SourceSearchLink {
            provider: "fhac".into(),
            label: "一史馆".into(),
            url: "https://fhac.com.cn/".into(),
            note: "明清档案目录；公开全文有限".into(),
        },
        SourceSearchLink {
            provider: "cnbksy".into(),
            label: "全国报刊索引".into(),
            url: "https://www.cnbksy.com/".into(),
            note: "晚清/民国报刊；通常需机构订阅".into(),
        },
        SourceSearchLink {
            provider: "hathitrust".into(),
            label: "HathiTrust".into(),
            url: format!("https://babel.hathitrust.org/cgi/ls?q1={encoded};a=srchls;anyall1=all;lmt=ft"),
            note: "外文/世界史全文；注意版权与 Full view".into(),
        },
    ]
}

pub async fn search(query: &str) -> SourceSearchResult {
    let query = query.trim();
    if query.is_empty() {
        return SourceSearchResult {
            hits: Vec::new(),
            links: Vec::new(),
            notes: vec!["输入事件、人物或书名后再搜索".into()],
        };
    }
    let mut hits = Vec::new();
    let mut notes = Vec::new();
    let (wikisource, internet_archive) = tokio::join!(
        search_wikisource(query, 6),
        search_internet_archive(query, 6)
    );
    match wikisource {
        Ok(mut items) => hits.append(&mut items),
        Err(error) => notes.push(format!("Wikisource 检索失败：{error}")),
    }
    match internet_archive {
        Ok(mut items) => hits.append(&mut items),
        Err(error) => notes.push(format!("Internet Archive 检索失败：{error}")),
    }
    notes.push("检索结果是候选；必须打开原文/扫描页，确认逐字 passage 后才能引用为史料。".into());
    SourceSearchResult {
        hits,
        links: external_links(query),
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_snippets() {
        assert_eq!(strip_html("<b>赤壁</b>&nbsp;之战"), "赤壁 之战");
    }

    #[test]
    fn builds_external_links_with_encoded_query() {
        let links = external_links("赤壁 之战");
        assert!(links.iter().any(|link| link.provider == "ctext" && link.url.contains("%E8%B5%A4%E5%A3%81")));
        assert!(links.iter().any(|link| link.provider == "hathitrust"));
    }

    #[test]
    fn keeps_title_slashes_in_wikisource_path() {
        let url = wikisource_url("明史/卷三百八");
        assert!(url.contains("/wiki/%E6%98%8E%E5%8F%B2/%E5%8D%B7%E4%B8%89%E7%99%BE%E5%85%AB"), "{url}");
        assert!(!url.contains("%2F"));
    }
}
