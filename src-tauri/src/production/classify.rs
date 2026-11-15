//! Choose an existing folder for a generated note.
//!
//! The model is a closed-list classifier: it only sees folders that already
//! exist and must answer with one of them (or 待整理). Anything invalid, low
//! confidence, cancelled or failed falls back to 待整理 so publishing never
//! blocks on classification.

use std::path::Path;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::artifact::{STAGING_FOLDER, STUDIO_FOLDER};
use crate::vault;

use super::clients::{self, AiConfig};

const MAX_BODY_CHARS: usize = 2000;

pub struct ClassifyResult {
    pub folder: String,
    pub confidence: String,
    pub reason: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

pub fn candidates(root: &Path) -> Vec<String> {
    vault::walk_folders(root)
        .into_iter()
        .filter(|folder| !folder.is_empty())
        .filter(|folder| folder != STAGING_FOLDER)
        .filter(|folder| {
            folder != STUDIO_FOLDER && !folder.starts_with(&format!("{STUDIO_FOLDER}/"))
        })
        .collect()
}

pub async fn classify(
    ai: &AiConfig,
    root: &Path,
    title: &str,
    body: &str,
    token: &CancellationToken,
) -> ClassifyResult {
    let candidates = candidates(root);
    if candidates.is_empty() {
        return fallback("没有可用的已有文件夹", "low", 0, 0);
    }
    if ai.api_key.trim().is_empty() {
        return fallback("没有配置模型 Key", "low", 0, 0);
    }

    let system = "你是笔记归档助手。只能从下面给出的已有文件夹里选择一个，不要新建文件夹。\
如果内容与任何文件夹都不匹配，或你不确定，选择「待整理」。\
只输出 JSON：{\"folder\":\"文件夹相对路径\",\"confidence\":\"high|medium|low\",\"reason\":\"不超过30字的中文理由\"}";
    let body: String = body.chars().take(MAX_BODY_CHARS).collect();
    let list = candidates
        .iter()
        .map(|folder| format!("- {folder}"))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "标题：{title}\n\n正文（前 {MAX_BODY_CHARS} 字）：\n{body}\n\n已有文件夹：\n{list}\n- {STAGING_FOLDER}"
    );

    match clients::chat(ai, system, &user, token).await {
        Ok(result) => {
            let (chosen, confidence, reason) = parse_decision(&result.content, &candidates);
            match chosen {
                Some(folder) => ClassifyResult {
                    folder,
                    confidence,
                    reason,
                    prompt_tokens: result.prompt_tokens,
                    completion_tokens: result.completion_tokens,
                },
                None => fallback(
                    &reason,
                    &confidence,
                    result.prompt_tokens,
                    result.completion_tokens,
                ),
            }
        }
        Err(error) => fallback(&format!("模型调用失败：{error}"), "low", 0, 0),
    }
}

fn fallback(reason: &str, confidence: &str, prompt: u64, completion: u64) -> ClassifyResult {
    ClassifyResult {
        folder: STAGING_FOLDER.to_string(),
        confidence: confidence.to_string(),
        reason: reason.to_string(),
        prompt_tokens: prompt,
        completion_tokens: completion,
    }
}

/// Returns `None` when the model did not pick a valid, sufficiently confident
/// existing folder. The caller then uses 待整理.
pub fn parse_decision(content: &str, candidates: &[String]) -> (Option<String>, String, String) {
    let Some(json) = extract_json(content) else {
        return (None, "low".into(), "模型没有返回 JSON".into());
    };
    let folder = json
        .get("folder")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .trim_matches('/')
        .replace('\\', "/");
    let confidence = json
        .get("confidence")
        .and_then(Value::as_str)
        .unwrap_or("low")
        .trim()
        .to_ascii_lowercase();
    let reason = json
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if folder.is_empty() {
        return (None, confidence, "模型没有给出文件夹".into());
    }
    let matched = candidates
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(&folder))
        .cloned()
        .or_else(|| {
            if folder.eq_ignore_ascii_case(STAGING_FOLDER) {
                Some(STAGING_FOLDER.to_string())
            } else {
                None
            }
        });
    let Some(matched) = matched else {
        return (None, confidence, format!("文件夹不在候选里：{folder}"));
    };
    if confidence != "high" && confidence != "medium" {
        return (None, confidence, reason);
    }
    (Some(matched), confidence, reason)
}

fn extract_json(content: &str) -> Option<Value> {
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&content[start..=end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<String> {
        vec!["历史".into(), "时事".into(), "科技".into()]
    }

    #[test]
    fn accepts_existing_medium_confidence_folder() {
        let (folder, confidence, _) = parse_decision(
            r#"{"folder":"历史","confidence":"medium","reason":"讲的是明清史"}"#,
            &candidates(),
        );
        assert_eq!(folder.as_deref(), Some("历史"));
        assert_eq!(confidence, "medium");
    }

    #[test]
    fn accepts_json_inside_a_code_fence() {
        let content = "```json\n{\"folder\":\"科技\",\"confidence\":\"high\",\"reason\":\"AI 产业\"}\n```";
        assert_eq!(parse_decision(content, &candidates()).0.as_deref(), Some("科技"));
    }

    #[test]
    fn rejects_a_folder_that_does_not_exist() {
        let (folder, _, reason) = parse_decision(
            r#"{"folder":"新建文件夹","confidence":"high","reason":"随便"}"#,
            &candidates(),
        );
        assert!(folder.is_none());
        assert!(reason.contains("不在候选"));
    }

    #[test]
    fn rejects_low_confidence() {
        let (folder, _, _) = parse_decision(
            r#"{"folder":"历史","confidence":"low","reason":"不太确定"}"#,
            &candidates(),
        );
        assert!(folder.is_none());
    }

    #[test]
    fn rejects_non_json() {
        let (folder, _, reason) = parse_decision("我觉得应该放历史", &candidates());
        assert!(folder.is_none());
        assert!(reason.contains("JSON"));
    }

    #[test]
    fn accepts_the_staging_folder_and_is_case_insensitive() {
        assert_eq!(
            parse_decision(
                r#"{"folder":"待整理","confidence":"high","reason":"没有合适的"}"#,
                &candidates()
            )
            .0
            .as_deref(),
            Some("待整理")
        );
        assert_eq!(
            parse_decision(
                r#"{"folder":"HISTORY","confidence":"high","reason":"x"}"#,
                &["history".to_string()]
            )
            .0
            .as_deref(),
            Some("history")
        );
    }
}
