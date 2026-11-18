//! Research-note template: a per-note chapter plan plus a deterministic
//! structure gate.
//!
//! The plan is what keeps a one-shot template from being rigid: the sections,
//! their length budget and the figures are decided per note, while the
//! execution order and the checks stay fixed. The gate catches the failure
//! mode the old single-prompt templates had -- a confident but off-spec note.

use serde::{Deserialize, Serialize};

use super::booklet::cjk_len;

/// Figure kinds the note renderer understands. A plan may only ask for these.
pub const FIGURE_KINDS: [&str; 5] = ["flow", "layers", "bar", "quote", "compare"];

/// Sections whose names belong to opinion videos, not to technical research.
pub const FORBIDDEN_SECTIONS: [&str; 3] = ["速览", "弦外之音", "速查"];

pub const MIN_SECTIONS: usize = 8;
pub const MAX_SECTIONS: usize = 12;
pub const MIN_CHARS: usize = 2_000;
pub const MAX_CHARS: usize = 5_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSection {
    pub heading: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub target_chars: u32,
    #[serde(default)]
    pub figures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotePlan {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub total_chars: u32,
    pub sections: Vec<PlanSection>,
}

/// Parse the plan JSON even when the model wrapped it in a code fence.
pub fn parse_plan(raw: &str) -> Result<NotePlan, String> {
    let text = raw.trim();
    let text = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .unwrap_or(text);
    let text = text.trim_end_matches("```").trim();
    let start = text.find('{').ok_or("计划里没有 JSON 对象")?;
    let end = text.rfind('}').ok_or("计划里没有 JSON 对象")?;
    if end <= start {
        return Err("计划 JSON 不完整".into());
    }
    let plan: NotePlan =
        serde_json::from_str(&text[start..=end]).map_err(|error| format!("计划 JSON 解析失败：{error}"))?;
    if plan.sections.len() < 3 || plan.sections.len() > MAX_SECTIONS {
        return Err(format!("计划章节数 {} 超出 3–{MAX_SECTIONS}", plan.sections.len()));
    }
    Ok(plan)
}

struct Scan {
    h1: Vec<String>,
    h2: Vec<String>,
    tables: usize,
    figure_issues: Vec<String>,
}

fn scan(body: &str) -> Scan {
    let mut scan = Scan {
        h1: Vec::new(),
        h2: Vec::new(),
        tables: 0,
        figure_issues: Vec::new(),
    };
    let mut fence: Option<String> = None;
    let mut figure_kind: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim_end();
        if let Some(lang) = &fence {
            if trimmed.trim() == "```" {
                if lang == "figure" {
                    match figure_kind.take() {
                        Some(kind) if FIGURE_KINDS.contains(&kind.as_str()) => {}
                        Some(kind) => scan
                            .figure_issues
                            .push(format!("图 kind「{kind}」不支持，只能是 {}", FIGURE_KINDS.join("/"))),
                        None => scan.figure_issues.push("图块缺少 kind".into()),
                    }
                }
                fence = None;
                continue;
            }
            if lang == "figure" {
                if let Some(rest) = trimmed.trim().strip_prefix("kind:") {
                    figure_kind = Some(rest.trim().to_string());
                }
            }
            continue;
        }
        if trimmed.trim_start().starts_with("```") {
            let lang = trimmed.trim_start().trim_start_matches('`').trim().to_lowercase();
            fence = Some(lang);
            if trimmed.trim_start().starts_with("```figure") {
                figure_kind = None;
            }
            continue;
        }
        if trimmed.starts_with("# ") {
            scan.h1.push(trimmed.trim_start_matches("# ").trim().to_string());
        } else if trimmed.starts_with("## ") {
            scan.h2.push(trimmed.trim_start_matches("## ").trim().to_string());
        }
        if trimmed.trim_start().starts_with('|') {
            scan.tables += 1;
        }
    }
    if fence.is_some() {
        scan.figure_issues.push("代码/图块没有闭合".into());
    }
    scan
}

/// Deterministic checks over the written note. Returns human-readable
/// violations; an empty list means the note may be published.
pub fn validate(body: &str, plan: Option<&NotePlan>) -> Vec<String> {
    let mut issues = Vec::new();
    if body.trim().is_empty() {
        return vec!["正文为空".into()];
    }
    if body.trim_start().starts_with("---") {
        issues.push("正文不能带 front matter".into());
    }
    if body.contains("以下是笔记") {
        issues.push("不要写“以下是笔记”".into());
    }

    let scan = scan(body);
    if scan.h1.len() != 1 {
        issues.push(format!("需要且只需要 1 个一级标题，当前 {}", scan.h1.len()));
    }
    if let Some(title) = scan.h1.first() {
        let chars = cjk_len(title);
        if chars > 30 {
            issues.push(format!("标题 {chars} 字，超过 30 字"));
        }
    }
    if scan.h2.len() < MIN_SECTIONS {
        issues.push(format!("二级小节只有 {} 个，至少 {MIN_SECTIONS} 个", scan.h2.len()));
    }
    if scan.h2.len() > MAX_SECTIONS {
        issues.push(format!("二级小节 {} 个，超过 {MAX_SECTIONS} 个", scan.h2.len()));
    }
    for heading in &scan.h2 {
        if let Some(word) = FORBIDDEN_SECTIONS.iter().find(|word| heading.contains(*word)) {
            issues.push(format!("禁用小节名「{word}」：{heading}"));
        }
    }
    if scan.tables > 0 {
        issues.push(format!("正文不允许 Markdown 表格（发现 {} 行）", scan.tables));
    }
    issues.extend(scan.figure_issues);

    let chars = cjk_len(body);
    if chars < MIN_CHARS || chars > MAX_CHARS {
        issues.push(format!("正文 {chars} 字，目标 {MIN_CHARS}–{MAX_CHARS}"));
    }

    if let Some(plan) = plan {
        let expected = plan.sections.len();
        if expected >= 3 {
            if scan.h2.len() + 2 < expected {
                issues.push(format!("计划 {expected} 节，正文只有 {} 节", scan.h2.len()));
            }
            if scan.h2.len() > expected + 3 {
                issues.push(format!("计划 {expected} 节，正文写了 {} 节", scan.h2.len()));
            }
        }
        if plan.total_chars > 0 {
            let target = plan.total_chars as usize;
            if chars * 10 < target * 6 || chars * 10 > target * 15 {
                issues.push(format!(
                    "计划总字数 {target}，正文 {chars} 字，偏差超过 40%"
                ));
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(index: usize) -> String {
        format!(
            "## {index}. 小节标题\n\n{}",
            "这一段写机制、限定条件和反例。".repeat(16)
        )
    }

    fn body(sections: usize) -> String {
        let mut text = String::from("# 值得抄的是进展判定\n\n");
        for index in 1..=sections {
            text.push_str(&section(index));
            text.push_str("\n\n");
        }
        text
    }

    fn plan(sections: usize) -> NotePlan {
        NotePlan {
            title: "进展判定".into(),
            total_chars: 2_500,
            sections: (0..sections)
                .map(|index| PlanSection {
                    heading: format!("{}. 小节", index + 1),
                    goal: "说明机制".into(),
                    target_chars: 250,
                    figures: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn accepts_a_note_that_follows_the_spec() {
        let text = body(10);
        assert!(cjk_len(&text) >= MIN_CHARS);
        assert_eq!(validate(&text, None), Vec::<String>::new());
    }

    #[test]
    fn rejects_opinion_video_sections() {
        let mut text = body(10);
        text.push_str("## 弦外之音\n\n一句判断。\n");
        let issues = validate(&text, None);
        assert!(issues.iter().any(|item| item.contains("弦外之音")), "{issues:?}");
    }

    #[test]
    fn rejects_tables() {
        let mut text = body(10);
        text.push_str("| 列 | 值 |\n| --- | --- |\n");
        let issues = validate(&text, None);
        assert!(issues.iter().any(|item| item.contains("表格")), "{issues:?}");
    }

    #[test]
    fn rejects_short_and_long_notes() {
        let short = body(10).chars().take(600).collect::<String>();
        assert!(validate(&short, None).iter().any(|item| item.contains("目标")));
        let mut long = body(10);
        long.push_str(&"重复内容".repeat(1200));
        assert!(validate(&long, None).iter().any(|item| item.contains("目标")));
    }

    #[test]
    fn rejects_unknown_figure_kind_and_unclosed_block() {
        let mut text = body(10);
        text.push_str("```figure\nkind: timeline\nlines: A -> B\n```\n");
        assert!(validate(&text, None).iter().any(|item| item.contains("timeline")));
        let mut broken = body(10);
        broken.push_str("```figure\nkind: flow\n");
        assert!(validate(&broken, None).iter().any(|item| item.contains("没有闭合")));
    }

    #[test]
    fn accepts_known_figure_kind() {
        let mut text = body(10);
        text.push_str("```figure\nkind: flow\nlines: A -> B\n```\n");
        assert_eq!(validate(&text, None), Vec::<String>::new());
    }

    #[test]
    fn checks_plan_adherence() {
        let plan = plan(10);
        assert!(validate(&body(10), Some(&plan)).is_empty());
        let fewer = body(6);
        assert!(validate(&fewer, Some(&plan)).iter().any(|item| item.contains("计划")));
    }

    #[test]
    fn parses_fenced_plan_json() {
        let raw = "```json\n{\"title\":\"T\",\"totalChars\":2500,\"sections\":[{\"heading\":\"1\"},{\"heading\":\"2\"},{\"heading\":\"3\"}]}\n```";
        let plan = parse_plan(raw).expect("plan");
        assert_eq!(plan.sections.len(), 3);
        assert!(parse_plan("不是 JSON").is_err());
        assert!(parse_plan("{\"sections\":[]}").is_err());
    }
}
