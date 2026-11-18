// Booklet generation: a plan JSON, one call per chapter, then deterministic
// Markdown assembly. Figures stay as specs; the reader/build renders them.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BookPlan {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub kicker: String,
    #[serde(default)]
    pub lede: String,
    #[serde(default)]
    pub chapters: Vec<BookChapter>,
    #[serde(default)]
    pub onepage: Vec<String>,
    #[serde(default)]
    pub three: Vec<String>,
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(default)]
    pub glossary: Vec<BookTerm>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BookChapter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub points: Vec<String>,
    #[serde(default)]
    pub figure: Option<BookFigure>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BookFigure {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BookTerm {
    #[serde(default)]
    pub term: String,
    #[serde(default)]
    pub cat: String,
    #[serde(default)]
    pub desc: String,
}

/// Scale the booklet budget to the material: a 2k-char briefing should not
/// produce a 7k-char book. Chapter count grows slowly; per-chapter length is
/// the remainder.
pub fn budget(material_cjk: usize) -> (usize, usize) {
    let chapters = ((material_cjk + 899) / 900).clamp(3, 8);
    let target = ((material_cjk as f64 * 1.4) / chapters as f64 / 50.0).round() as usize * 50;
    (chapters, target.clamp(500, 1_200))
}

pub fn plan_system(chapters: usize, chapter_chars: usize) -> String {
    format!(
        r#"你是技术笔记的总编。把素材规划成一本 {chapters} 章的小册子，只输出 JSON，不要解释。

{{
  "title": "不超过 24 字的判断句",
  "kicker": "技术小册子",
  "lede": "一句话说明这本册子解决什么",
  "chapters": [
    {{
      "title": "判断句标题",
      "points": ["每条一句判断，≤40 字，最多 4 条；不要抄素材原句"],
      "figure": {{
        "kind": "flow",
        "title": "一句判断，≤24 字，不带图号",
        "lines": ["最多 4 条，每条 ≤24 字，如 A -> B"],
        "note": "一句判断，说明这张图在说什么"
      }}
    }}
  ],
  "onepage": ["6 条，每条 ≤24 字"],
  "three": ["3 条，每条 ≤24 字"],
  "checks": ["6 个自检问题，每个 ≤24 字"],
  "glossary": [{{ "term": "术语", "cat": "类别", "desc": "≤30 字" }}]
}}

规则：
- 整个 JSON 控制在 8 KB 以内；points 每条 ≤40 字，不要写长段落或抄素材。
- 正好 {chapters} 章；每章目标 {chapter_chars} 字左右（可浮动 20%）。素材短就不要硬凑。
- 章节标题必须是一句判断，不写「背景」「影响」这类空标签。
- 全书 3–7 张图，只放在比文字更好解释的地方；没有合适的图就省略 figure。
- figure.kind 只能是 flow / layers / bar / quote / compare。
- flow 的 lines 写成 `A -> B`，可以链式 `A -> B -> C`；layers 写成 `层名：项1 | 项2`；bar 写成 `标签 | 数值 | 说明`；quote 写一句原话或判断。
- 有「前后对照 / 拆开前后 / 传统 vs 新」时优先用 compare：图里用 `left: 左栏标签 | 左栏标题`、`left-item: 标题 | 说明`（可多条）、`left-foot: 红色脚注`；右栏用 `right` / `right-item` / `right-foot`。这些行直接写在 figure 的 lines 里。
- 只使用素材里的信息，不编造数字、书名、机构。
- 术语表 8–12 条，只留读这份材料要用的。"#
    )
}

pub fn chapter_system(target_chars: usize) -> String {
    format!(
        r#"你是技术笔记作者。只写这一章，要求：
- 只输出 Markdown 正文，不要一级/二级标题，不要「以下是」这类开场。
- 目标 {target_chars} 字左右，最多不超过 {} 字；超过会被二次压缩。连贯段落，不要用项目符号堆要点。
- 只使用素材里的信息；保留原文的限定条件、反例和数字。
- 术语第一次出现用素材里的类比解释清楚。
- 不写总结段，不写「总之」。"#,
        (target_chars + 400).min(1_600)
    )
}

/// Second pass for chapters that overshoot the length budget. Keeps facts and
/// numbers, drops repetition and transitions.
pub fn compress_system() -> &'static str {
    r#"你是编辑。把下面这一章压缩到 900 字以内（中文字符），只保留：
- 核心判断与因果链；
- 数字、人名、机构、专有名词；
- 必要的限定条件和反例。
删掉重复解释、过渡句和口号。不要新增信息，不要改写数字，不要加标题。只输出正文。"#
}

/// Chinese character count; the length budget is about prose, not Markdown.
pub fn cjk_len(text: &str) -> usize {
    text.chars().filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch)).count()
}

/// The model sometimes wraps JSON in fences or adds prose; keep the object.
pub fn parse_plan(text: &str) -> Result<BookPlan, String> {
    let trimmed = text.trim();
    let start = trimmed.find('{').ok_or_else(|| "大纲里没有 JSON".to_string())?;
    let end = trimmed.rfind('}').ok_or_else(|| "大纲 JSON 不完整".to_string())?;
    let slice = &trimmed[start..=end];
    let plan: BookPlan = serde_json::from_str(slice).map_err(|error| format!("大纲 JSON 解析失败：{error}"))?;
    if plan.chapters.is_empty() {
        return Err("大纲没有章节".into());
    }
    Ok(plan)
}

pub fn chapter_user(chapter: &BookChapter, material: &str) -> String {
    let points = chapter.points.iter().map(|item| format!("- {item}")).collect::<Vec<_>>().join("\n");
    format!(
        "本章标题：{}\n本章要点：\n{}\n\n素材：\n{}",
        chapter.title, points, material
    )
}

/// Assemble the final Markdown. Figures keep their spec form so the reader and
/// the PDF build share one renderer.
pub fn assemble(plan: &BookPlan, chapters: &[String], source_note: &str) -> String {
    let mut out = String::new();
    let title = if plan.title.trim().is_empty() { "未命名小册子" } else { plan.title.trim() };
    out.push_str(&format!("# {title}\n\n"));

    if !plan.onepage.is_empty() {
        out.push_str("## 一页看懂\n\n");
        for item in &plan.onepage {
            out.push_str(&format!("- {}\n", item.trim()));
        }
        out.push('\n');
    }

    let mut figure_no = 0;
    for (index, chapter) in plan.chapters.iter().enumerate() {
        out.push_str(&format!("## {}\n\n", chapter.title.trim()));
        if let Some(text) = chapters.get(index) {
            out.push_str(text.trim());
            out.push_str("\n\n");
        }
        if let Some(figure) = &chapter.figure {
            let kind = match figure.kind.as_str() {
                "layers" | "bar" | "quote" => figure.kind.as_str(),
                _ => "flow",
            };
            if !figure.lines.is_empty() {
                figure_no += 1;
                let caption = figure.title.trim();
                out.push_str("```figure\n");
                out.push_str(&format!("kind: {kind}\n"));
                out.push_str(&format!("title: 图 {figure_no} {caption}\n"));
                for line in &figure.lines {
                    out.push_str(&format!("{}\n", line.trim()));
                }
                if !figure.note.trim().is_empty() {
                    out.push_str(&format!("note: {}\n", figure.note.trim()));
                }
                out.push_str("```\n\n");
            }
        }
    }

    if !plan.three.is_empty() {
        out.push_str("## 三句话\n\n");
        for item in &plan.three {
            out.push_str(&format!("- {}\n", item.trim()));
        }
        out.push('\n');
    }

    if !plan.checks.is_empty() {
        out.push_str("## 自检问题\n\n");
        for item in &plan.checks {
            out.push_str(&format!("- {}\n", item.trim()));
        }
        out.push('\n');
    }

    if !plan.glossary.is_empty() {
        out.push_str("## 术语表\n\n");
        for item in &plan.glossary {
            let cat = if item.cat.trim().is_empty() { "概念" } else { item.cat.trim() };
            out.push_str(&format!("- **{}**（{}）：{}\n", item.term.trim(), cat, item.desc.trim()));
        }
        out.push('\n');
    }

    out.push_str("## 出处与边界\n\n");
    out.push_str(source_note.trim());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plan_wrapped_in_prose() {
        let text = "下面是 JSON：\n```json\n{\"title\":\"T\",\"chapters\":[{\"title\":\"A\"}]}\n```\n完毕";
        let plan = parse_plan(text).expect("plan");
        assert_eq!(plan.title, "T");
        assert_eq!(plan.chapters.len(), 1);
    }

    #[test]
    fn assembles_figures_with_numbers() {
        let text = r#"{
            "title": "T",
            "onepage": ["a"],
            "chapters": [
                {"title": "第一章", "figure": {"kind": "flow", "title": "流向", "lines": ["A -> B"], "note": "说明"}},
                {"title": "第二章", "figure": {"kind": "bar", "title": "对比", "lines": ["x | 1 | n"]}}
            ],
            "three": ["b"], "checks": ["c"], "glossary": [{"term":"t","cat":"c","desc":"d"}]
        }"#;
        let plan = parse_plan(text).expect("plan");
        let body = assemble(&plan, &["正文一".to_string(), "正文二".to_string()], "来源说明");
        assert!(body.contains("图 1 流向"));
        assert!(body.contains("图 2 对比"));
        assert!(body.contains("## 术语表"));
        assert!(body.ends_with("来源说明\n"));
    }
}
