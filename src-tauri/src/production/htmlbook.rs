// HTML booklet authoring chain: a fixed design/plan/skeleton, a bounded HTML
// author in the webview, then render/print/page-check/judge in Rust.
//
// This is the experimental counterpart to the Markdown `booklet` template: the
// primary artifact is a self-contained HTML/CSS booklet plus an A4 PDF, not a
// Markdown note.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::booklet::{BookPlan, BookTerm};
use super::clients::{self, AiConfig};

pub const DESIGN: &str = r##"{
  "preset": "paper-vermilion",
  "paper": "#f7f2e8",
  "surface": "#fffdf8",
  "ink": "#1b1813",
  "ink2": "#514a3f",
  "ink3": "#686055",
  "line": "#d8cdb8",
  "accent": "#b4232f",
  "serif": "'Noto Serif SC', 'Songti SC', 'SimSun', serif",
  "sans": "'Sora', 'Microsoft YaHei', sans-serif",
  "mono": "'Bricolage Grotesque', 'Cascadia Code', Consolas, monospace"
}"##;

const JUDGE_SYSTEM: &str = "你是最终视觉审校。逐页看 A4 页面截图，只判断确凿的版面问题：文字被裁、文字压线、图空框/错位、内容超出页面、整页异常留白。页面上下边缘是截图裁切，不算问题。只输出 JSON 数组，不要解释。";

pub struct AuthorPaths {
    pub dir: PathBuf,
    pub book: PathBuf,
    pub plan: PathBuf,
    pub material: PathBuf,
    pub design: PathBuf,
    pub brief: PathBuf,
}

fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn list(items: &[String], class_name: &str) -> String {
    if items.is_empty() {
        return String::new();
    }
    let body = items
        .iter()
        .map(|item| format!("<li>{}</li>", esc(item)))
        .collect::<Vec<_>>()
        .join("");
    format!("<ul class=\"{class_name}\">{body}</ul>")
}

fn glossary(items: &[BookTerm]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let body = items
        .iter()
        .map(|item| {
            format!(
                "<div class=\"term\"><b>{}</b><span>{}</span>{}</div>",
                esc(&item.term),
                esc(&item.cat),
                if item.desc.is_empty() { String::new() } else { format!("<p>{}</p>", esc(&item.desc)) }
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!("<div class=\"terms\">{body}</div>")
}

pub fn skeleton(plan: &BookPlan) -> String {
    let chapters = plan
        .chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| {
            format!(
                "<section class=\"page chapter-page\">\
                 <section class=\"sec\" data-ch=\"{}\">\
                 <!-- CHAPTER-{:02} -->\
                 </section>\
                 <span class=\"page-no\">{:02}</span>\
                 </section>",
                index + 1,
                index + 1,
                index + 2
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let toc = plan
        .chapters
        .iter()
        .map(|chapter| format!("<li><span>{}</span></li>", esc(&chapter.title)))
        .collect::<Vec<_>>()
        .join("");
    let title = if plan.title.trim().is_empty() { "技术小册子" } else { plan.title.trim() };
    let kicker = if plan.kicker.trim().is_empty() { "技术小册子" } else { plan.kicker.trim() };
    let lede = plan.lede.trim();
    let onepage = list(&plan.onepage, "onepage");
    let three = list(&plan.three, "three");
    let checks = if plan.checks.is_empty() {
        String::new()
    } else {
        format!(
            "<ol class=\"checks\">{}</ol>",
            plan.checks.iter().map(|item| format!("<li>{}</li>", esc(item))).collect::<Vec<_>>().join("")
        )
    };
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
:root{{--paper:#f7f2e8;--surface:#fffdf8;--ink:#1b1813;--ink2:#514a3f;--ink3:#686055;--line:#d8cdb8;--accent:#b4232f;--serif:"Noto Serif SC","Songti SC","SimSun",serif;--sans:"Sora","Microsoft YaHei",sans-serif;--mono:"Bricolage Grotesque","Cascadia Code",Consolas,monospace}}
*{{box-sizing:border-box}}
html,body{{margin:0;background:#d8d2c6;color:var(--ink);font-family:var(--serif);font-size:10.5pt;line-height:1.85}}
.doc{{width:210mm;margin:0 auto;padding:0}}
.page{{width:210mm;height:297mm;background:var(--paper);margin:0 auto 18px;padding:20mm 18mm;position:relative;overflow:hidden;page-break-after:always;break-after:page;box-shadow:0 2px 14px rgba(0,0,0,.18)}}
@media print{{html,body{{background:#fff}}.page{{margin:0;box-shadow:none}}}}
@page{{size:A4;margin:0}}
.cover{{display:flex;flex-direction:column;justify-content:center;background:linear-gradient(160deg,#f7f2e8,#efe7d6)}}
.cover .kicker{{font-family:var(--sans);font-size:9pt;letter-spacing:.18em;color:var(--accent);text-transform:uppercase}}
.cover h1{{font-size:30pt;line-height:1.3;margin:8mm 0 6mm;font-weight:500}}
.cover .lede{{font-size:12pt;line-height:1.9;color:var(--ink2);max-width:155mm}}
.cover .meta{{margin-top:14mm;font-family:var(--sans);font-size:9pt;color:var(--ink3);letter-spacing:.06em}}
.toc h2,.page h2{{font-family:var(--sans);font-size:13pt;letter-spacing:.08em;margin:0 0 8mm;color:var(--ink)}}
.toc ol{{list-style:none;padding:0;margin:0;counter-reset:toc}}
.toc li{{display:flex;gap:8px;align-items:baseline;border-bottom:1px solid var(--line);padding:3.2mm 0;font-size:11pt;counter-increment:toc}}
.toc li:before{{content:counter(toc,decimal-leading-zero);font-family:var(--mono);font-size:9pt;color:var(--ink3)}}
.sec{{max-width:174mm}}
.sec h2{{font-size:18pt;line-height:1.45;margin:0 0 4mm;font-weight:500}}
.sec h3{{font-size:12pt;margin:5mm 0 2mm;font-weight:600;color:var(--ink)}}
.sec p{{margin:0 0 3mm;color:var(--ink2)}}
.sec ul{{margin:0 0 3mm 5mm;padding:0;color:var(--ink2)}}
.sec li{{margin:0 0 1.5mm}}
.sec strong{{color:var(--ink);font-weight:600}}
.fig{{margin:5mm 0 4mm;padding:0}}
.fig svg{{width:100%;height:auto;display:block}}
.fig svg rect{{fill:var(--surface);stroke:var(--line);stroke-width:1}}
.fig svg rect.chip{{fill:#f0e6d2;stroke:var(--accent)}}
.fig svg text{{fill:var(--ink);font-family:var(--sans)}}
.fig svg path{{stroke:var(--ink2);fill:none}}
.fig svg line{{stroke:var(--ink2)}}
.fig svg marker path{{fill:var(--ink2)}}
.fig-title{{font-family:var(--sans);font-size:9pt;letter-spacing:.04em;color:var(--ink3);margin:0 0 2mm}}
.fig-note{{font-size:9pt;color:var(--ink3);margin:2mm 0 0}}
.onepage,.three,.checks{{margin:0 0 5mm 5mm;padding:0;color:var(--ink2)}}
.onepage li,.three li,.checks li{{margin:0 0 2mm}}
.terms{{display:grid;grid-template-columns:1fr 1fr;gap:4mm 8mm}}
.term{{border-top:1px solid var(--line);padding-top:2mm}}
.term b{{display:block;font-size:10pt;color:var(--ink)}}
.term span{{font-family:var(--sans);font-size:9pt;color:var(--accent)}}
.term p{{margin:1mm 0 0;font-size:9pt;color:var(--ink3)}}
.page-no{{position:absolute;right:18mm;bottom:10mm;font-family:var(--mono);font-size:9pt;color:var(--ink3)}}
</style>
</head>
<body>
<main class="doc">
<section class="page cover">
  <div class="kicker">{kicker}</div>
  <h1>{title}</h1>
  <p class="lede">{lede}</p>
  <div class="meta">技术小册子 · 页边笔记</div>
</section>
<section class="page toc">
  <h2>目录</h2>
  <ol>{toc}</ol>
</section>
<section class="page summary-page">
  <h2>一页看懂</h2>
  {onepage}
</section>
{chapters}
<section class="page closing-page">
  <h2>三句话</h2>
  {three}
  <h2 style="margin-top:8mm">自检</h2>
  {checks}
</section>
<section class="page glossary-page">
  <h2>术语表</h2>
  {glossary}
</section>
</main>
</body>
</html>"#,
        title = esc(title),
        kicker = esc(kicker),
        lede = esc(lede),
        toc = toc,
        onepage = onepage,
        chapters = chapters,
        three = three,
        checks = checks,
        glossary = glossary(&plan.glossary),
    )
}

fn shorten(value: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn brief(plan: &BookPlan) -> String {
    let mut out = String::from("# 作者简报

");
    out.push_str(&format!("标题：{}
导语：{}

", shorten(plan.title.trim(), 40), shorten(plan.lede.trim(), 100)));
    out.push_str(&format!(
        "骨架占位符：{} 个章节，对应 <!-- CHAPTER-01 --> … <!-- CHAPTER-{:02} -->；用 edit_file 替换，不要新增章节。

",
        plan.chapters.len(),
        plan.chapters.len()
    ));
    for (index, chapter) in plan.chapters.iter().enumerate() {
        out.push_str(&format!("## {}. {}
", index + 1, chapter.title.trim()));
        for point in chapter.points.iter().take(3) {
            out.push_str(&format!("- {}
", shorten(point, 90)));
        }
        if let Some(figure) = &chapter.figure {
            out.push_str(&format!(
                "- 图：{} · {}{}
",
                figure.kind,
                shorten(&figure.title, 30),
                if figure.lines.is_empty() { String::new() } else { format!(" · {}", figure.lines.iter().take(3).map(|line| shorten(line, 30)).collect::<Vec<_>>().join(" / ")) }
            ));
        }
        out.push_str("\n");
    }
    if !plan.onepage.is_empty() {
        out.push_str("## 一页看懂
");
        for item in plan.onepage.iter().take(6) {
            out.push_str(&format!("- {}
", shorten(item, 40)));
        }
        out.push_str("\n");
    }
    if !plan.three.is_empty() {
        out.push_str("## 三句话
");
        for item in plan.three.iter().take(3) {
            out.push_str(&format!("- {}
", shorten(item, 40)));
        }
        out.push_str("\n");
    }
    if !plan.checks.is_empty() {
        out.push_str("## 自检
");
        for item in plan.checks.iter().take(6) {
            out.push_str(&format!("- {}
", shorten(item, 40)));
        }
        out.push_str("\n");
    }
    if !plan.glossary.is_empty() {
        out.push_str("## 术语表
");
        for item in plan.glossary.iter().take(10) {
            out.push_str(&format!("- {}（{}）：{}
", item.term, item.cat, shorten(&item.desc, 40)));
        }
    }
    out
}

pub fn author_system(target_chars: usize) -> String {
    format!(
        r#"你是技术小册子的 HTML 作者。只写这一章，输出 HTML 片段；不要 <html>/<head>/<body>/<style>/<script>，不要 Markdown 围栏。
要求：
- 以 <h2>判断句标题</h2> 开头；3–5 个 <p> 或 <ul>，总长约 {target_chars} 字，最多 1500 字。
- 只使用素材里的信息，保留数字、限定条件和反例；不要编造。
- 至少一张图：<figure class="fig"><figcaption class="fig-title">一句判断</figcaption><svg viewBox="0 0 640 220" role="img">…</svg><p class="fig-note">图注</p></figure>。
- SVG 里每个文字都要放进 <rect class="box"> 或 <rect class="chip"> 里；文字坐标必须在 viewBox 内，不能压线或越界。用 <path> + <marker> 画箭头也可以。
- 不要写 <style>，CSS 类已经在骨架里（.sec/.fig/.fig-title/.fig-note/.box/.chip）。
- 不要新增章节，不要写页眉页脚。"#
    )
}

pub fn chapter_user(plan: &BookPlan, chapter: &super::booklet::BookChapter, material: &str, index: usize) -> String {
    let mut out = format!("第 {} 章 / 共 {} 章
标题：{}

要点：
", index + 1, plan.chapters.len(), chapter.title);
    for point in &chapter.points {
        out.push_str(&format!("- {point}
"));
    }
    if let Some(figure) = &chapter.figure {
        out.push_str(&format!(
            "
计划图：{} · {}
",
            figure.kind,
            figure.title
        ));
        for line in &figure.lines {
            out.push_str(&format!("- {line}
"));
        }
        if !figure.note.is_empty() {
            out.push_str(&format!("图注：{}
", figure.note));
        }
    }
    out.push_str(&format!("
全书标题：{}
导语：{}

素材：
{}", plan.title, plan.lede, material));
    out
}

fn strip_fences(raw: &str) -> String {
    let text = raw.trim();
    if !text.starts_with("```") {
        return text.to_string();
    }
    let mut lines = text.lines();
    lines.next();
    let rest = lines.collect::<Vec<_>>().join("
");
    match rest.rfind("```") {
        Some(index) => rest[..index].trim().to_string(),
        None => rest.trim().to_string(),
    }
}

pub fn clean_fragment(raw: &str) -> Result<String, String> {
    let text = strip_fences(raw);
    if text.trim().is_empty() {
        return Err("模型没有返回 HTML 片段".into());
    }
    let lower = text.to_ascii_lowercase();
    for bad in ["<script", "javascript:", " onerror=", " onload="] {
        if lower.contains(bad) {
            return Err(format!("章节 HTML 里不允许 {bad}"));
        }
    }
    if lower.contains("<html") || lower.contains("<body") || lower.contains("<style") {
        return Err("章节只允许片段，不要输出 html/body/style".into());
    }
    Ok(text.trim().to_string())
}

pub fn insert_chapter(html: &str, index: usize, fragment: &str) -> Result<String, String> {
    let marker = format!("<!-- CHAPTER-{:02} -->", index + 1);
    if !html.contains(&marker) {
        return Err(format!("找不到章节占位符 {marker}"));
    }
    Ok(html.replacen(&marker, fragment, 1))
}

pub fn prepare(
    root: &Path,
    job_id: &str,
    material: &str,
    plan_json: &str,
    plan: &BookPlan,
) -> Result<AuthorPaths, String> {
    let dir = root.join(".yebian").join("runs").join(job_id).join("author");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let paths = AuthorPaths {
        book: dir.join("book.html"),
        plan: dir.join("plan.json"),
        material: dir.join("material.md"),
        design: dir.join("design.json"),
        brief: dir.join("brief.md"),
        dir,
    };
    std::fs::write(&paths.material, material.as_bytes()).map_err(|error| error.to_string())?;
    std::fs::write(&paths.plan, plan_json.as_bytes()).map_err(|error| error.to_string())?;
    std::fs::write(&paths.design, DESIGN.as_bytes()).map_err(|error| error.to_string())?;
    std::fs::write(&paths.brief, brief(plan).as_bytes()).map_err(|error| error.to_string())?;
    std::fs::write(&paths.book, skeleton(plan).as_bytes()).map_err(|error| error.to_string())?;
    Ok(paths)
}

pub fn read_book(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|error| format!("HTML 还没写出来：{error}"))?;
    if meta.len() > 768 * 1024 {
        return Err(format!("HTML {:.0} KB，超过 768 KB", meta.len() as f64 / 1024.0));
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let html = String::from_utf8_lossy(&bytes).into_owned();
    validate(&html)?;
    Ok(html)
}

pub fn validate(html: &str) -> Result<(), String> {
    if html.trim().is_empty() {
        return Err("HTML 是空的".into());
    }
    let lower = html.to_ascii_lowercase();
    for bad in ["<script", "javascript:", " onerror=", " onload="] {
        if lower.contains(bad) {
            return Err(format!("HTML 里不允许 {bad}"));
        }
    }
    if !html.contains("class=\"doc\"") {
        return Err("缺少 .doc 容器".into());
    }
    if !html.contains("class=\"page") {
        return Err("缺少 .page 分页容器".into());
    }
    if html.contains("<!-- CHAPTER-") {
        return Err("还有章节占位符没有替换".into());
    }
    Ok(())
}

pub fn page_offsets(metrics: &Value) -> Vec<i64> {
    metrics
        .get("pageOffsets")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

pub fn pages(metrics: &Value) -> i64 {
    metrics.get("pages").and_then(Value::as_i64).unwrap_or(0)
}

/// A deterministic gate over the same metrics the author saw.
pub fn layout_issues(metrics: &Value) -> Vec<String> {
    let count = |key: &str| {
        metrics
            .get(key)
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0)
    };
    let mut issues = Vec::new();
    for (key, label) in [
        ("overflow", "溢出"),
        ("outOfBounds", "越界"),
        ("under12", "小字"),
        ("pageOverflow", "页溢出"),
        ("emptyBoxes", "空框"),
        ("labelOverlaps", "标签重叠"),
        ("textOverflow", "文字溢出"),
    ] {
        let value = count(key);
        if value > 0 {
            issues.push(format!("{label} {value}"));
        }
    }
    let clipped = metrics.get("svgClipped").and_then(Value::as_i64).unwrap_or(0);
    if clipped > 0 {
        issues.push(format!("SVG 越界 {clipped}"));
    }
    issues
}

pub fn compact_css(level: usize) -> &'static str {
    if level >= 2 {
        r#".page{padding:16mm}.sec p{font-size:10pt;line-height:1.62;margin:0 0 2mm}.sec li{font-size:10pt;margin-bottom:0.8mm}.sec h3{font-size:11.5pt;margin:3.5mm 0 1.5mm}.fig{margin:2.5mm 0}.fig svg{max-height:56mm}.fig-title,.fig-note{font-size:9pt}.sec h2{font-size:17pt;margin-bottom:2.5mm}"#
    } else {
        r#".page{padding:17mm}.sec p{line-height:1.70;margin:0 0 2.4mm}.sec li{margin-bottom:1mm}.fig{margin:3mm 0 2.5mm}.fig svg{max-height:64mm}.sec h2{margin-bottom:3mm}"#
    }
}

pub fn with_css(html: &str, css: &str, level: usize) -> String {
    let tag = format!("<style id=\"fix-css-{level}\">{css}</style>");
    if html.contains("</head>") {
        html.replacen("</head>", &format!("{tag}</head>"), 1)
    } else {
        format!("{html}{tag}")
    }
}

pub fn projection(plan: &BookPlan, pages: i64) -> String {
    let title = if plan.title.trim().is_empty() { "技术小册子" } else { plan.title.trim() };
    let mut body = format!(
        "# {title}\n\n> {}\n\n## 成品\n\n- HTML 小册子：`book.html`（任务流程页可打开）\n- A4 PDF：`book.pdf`（{pages} 页）\n\n这条笔记是投影与索引；正文、版式和图在 HTML/PDF 成品里。\n\n## 一页看懂\n\n",
        plan.lede.trim()
    );
    for item in &plan.onepage {
        body.push_str(&format!("- {item}\n"));
    }
    body.push_str("\n## 三句话\n\n");
    for item in &plan.three {
        body.push_str(&format!("- {item}\n"));
    }
    body
}

pub fn judge_prompt(start: usize, end: usize, total: usize) -> String {
    format!(
        "这是第 {start}-{end} 页，共 {total} 页。逐页判断，输出 JSON 数组，每项：{{\"page\": 绝对页码, \"pass\": true/false, \"issues\": [\"一句话问题\"]}}。没有问题就 pass=true、issues=[]。不要输出 Markdown 围栏。"
    )
}

/// Ask the vision model to judge page PNGs in batches. Returns per-page
/// verdicts plus summed token usage.
pub async fn judge_pages(
    ai: &AiConfig,
    pages: &[String],
    token: &CancellationToken,
) -> Result<(Vec<Value>, u64, u64, u64), String> {
    if pages.is_empty() {
        return Err("没有页面截图可审".into());
    }
    let mut verdicts = Vec::new();
    let mut prompt_tokens = 0;
    let mut completion_tokens = 0;
    let mut reasoning_tokens = 0;
    for (batch_index, batch) in pages.chunks(3).enumerate() {
        let start = batch_index * 3 + 1;
        let end = start + batch.len() - 1;
        let mut images = Vec::new();
        for path in batch {
            let bytes = std::fs::read(path).map_err(|error| format!("页面图打不开：{error}"))?;
            images.push(format!("data:image/png;base64,{}", clients::base64_encode(&bytes)));
        }
        let result = clients::chat_vision(
            ai,
            JUDGE_SYSTEM,
            &judge_prompt(start, end, pages.len()),
            &images,
            token,
        )
        .await?;
        prompt_tokens += result.prompt_tokens;
        completion_tokens += result.completion_tokens;
        reasoning_tokens += result.reasoning_tokens;
        let text = result.content.trim();
        let text = text.strip_prefix("```json").or_else(|| text.strip_prefix("```")).unwrap_or(text);
        let text = text.trim_end_matches("```").trim();
        let parsed: Value = serde_json::from_str(text).map_err(|error| {
            format!("审校返回不是 JSON（第 {start}-{end} 页）：{error}；原文：{}", text.chars().take(200).collect::<String>())
        })?;
        let items = parsed
            .as_array()
            .cloned()
            .or_else(|| parsed.get("pages").and_then(Value::as_array).cloned())
            .unwrap_or_default();
        if items.is_empty() {
            return Err(format!("审校第 {start}-{end} 页没有返回逐页结果"));
        }
        for (index, item) in items.into_iter().enumerate() {
            let page = item
                .get("page")
                .and_then(Value::as_i64)
                .unwrap_or((start + index) as i64);
            let pass = item.get("pass").and_then(Value::as_bool).unwrap_or(false);
            let issues = item.get("issues").cloned().unwrap_or_else(|| json!([]));
            verdicts.push(json!({ "page": page, "pass": pass, "issues": issues }));
        }
    }
    verdicts.sort_by_key(|item| item.get("page").and_then(Value::as_i64).unwrap_or(0));
    if verdicts.len() < pages.len() {
        return Err(format!("审校只返回了 {}/{} 页", verdicts.len(), pages.len()));
    }
    if verdicts.iter().any(|item| !item.get("pass").and_then(Value::as_bool).unwrap_or(false)) {
        let failed = verdicts
            .iter()
            .filter(|item| !item.get("pass").and_then(Value::as_bool).unwrap_or(false))
            .map(|item| {
                format!(
                    "第 {} 页：{}",
                    item.get("page").and_then(Value::as_i64).unwrap_or(0),
                    item.get("issues").map(Value::to_string).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("；");
        return Err(format!("视觉审校未通过：{failed}"));
    }
    Ok((verdicts, prompt_tokens, completion_tokens, reasoning_tokens))
}
