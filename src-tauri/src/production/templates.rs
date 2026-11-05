use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateStep {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub group: String,
    pub badge: String,
    pub description: String,
    pub pipeline: Vec<String>,
    pub sources: Vec<String>,
    pub steps: Vec<TemplateStep>,
}

fn step(key: &str, label: &str) -> TemplateStep {
    TemplateStep { key: key.into(), label: label.into() }
}

fn make_template(
    id: &str,
    name: &str,
    group: &str,
    badge: &str,
    description: &str,
    pipeline: &[&str],
    sources: &[&str],
    steps: Vec<TemplateStep>,
) -> TemplateInfo {
    TemplateInfo {
        id: id.into(),
        name: name.into(),
        group: group.into(),
        badge: badge.into(),
        description: description.into(),
        pipeline: pipeline.iter().map(|item| (*item).to_string()).collect(),
        sources: sources.iter().map(|item| (*item).to_string()).collect(),
        steps,
    }
}

pub fn builtin_templates() -> Vec<TemplateInfo> {
    vec![
        make_template(
            "video-note",
            "视频笔记",
            "基础",
            "基础",
            "转写、校对、加工成一份可标注的笔记。",
            &["来源", "转写", "校对", "加工", "入库"],
            &["bili", "favorites", "file"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("prompt", "AI 加工"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "text-note",
            "文稿笔记",
            "基础",
            "基础",
            "已有文稿直接校对和加工，不经过转写。",
            &["文本", "校对", "加工", "入库"],
            &["text"],
            vec![step("source", "读取文稿"), step("refine", "AI 校对"), step("prompt", "AI 加工"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "long-note",
            "长文分章",
            "进阶",
            "可选",
            "先按内容切章，再合成一篇带章节的长文。",
            &["来源", "校对", "切章", "合并", "入库"],
            &["bili", "favorites", "file", "text"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("chapter", "章节切分"), step("merge", "合并"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "mindmap",
            "思维导图",
            "进阶",
            "可选",
            "把长内容整理成一张可逐层展开的导图。",
            &["来源", "校对", "导图", "入库"],
            &["bili", "favorites", "file", "text"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("mindmap", "生成导图"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "drill",
            "练一练",
            "进阶",
            "可选",
            "笔记之后提炼考察点、出题和延伸问题。",
            &["来源", "加工", "出题", "入库"],
            &["bili", "favorites", "file", "text"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("prompt", "AI 加工"), step("drill", "生成练习"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "gameguide",
            "阴阳师攻略",
            "垂直",
            "垂直",
            "专名、数值和优先级逐条回文核对。",
            &["来源", "转写", "核对", "攻略", "入库"],
            &["bili", "favorites", "file"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("gameguide", "生成攻略"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "multi-style",
            "多风格对照",
            "进阶",
            "可选",
            "同一份内容并行两套加工，结果页直接比差异。",
            &["来源", "校对", "风格 A", "风格 B", "对照"],
            &["bili", "favorites", "file", "text"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("prompt", "生成两版"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "series",
            "系列综述",
            "进阶",
            "可选",
            "多份素材各出一份笔记，再合成一篇综述。",
            &["来源", "校对", "分篇", "合并", "入库"],
            &["bili", "favorites", "file", "text"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("prompt", "生成分篇"), step("merge", "合并综述"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "trace",
            "速览与溯源",
            "进阶",
            "可选",
            "一版速览用来扫读，一份溯源报告用来查证。",
            &["来源", "校对", "速览", "溯源", "入库"],
            &["bili", "favorites", "file", "text"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("prompt", "生成速览"), step("trace", "联网溯源"), step("publish", "入库页边笔记")],
        ),
        make_template(
            "multi-branch",
            "多路对照",
            "进阶",
            "可选",
            "同一份内容并行两条加工路线，适合比较提示词。",
            &["来源", "校对", "加工 A", "加工 B", "对照"],
            &["bili", "favorites", "file", "text"],
            vec![step("source", "读取来源"), step("transcribe", "转写"), step("refine", "AI 校对"), step("prompt", "生成两版"), step("publish", "入库页边笔记")],
        ),
    ]
}

pub fn template(id: &str) -> Option<TemplateInfo> {
    builtin_templates().into_iter().find(|item| item.id == id)
}

pub fn steps_for(template_id: &str, source_kind: &str, multiple_items: bool) -> Vec<JobStepSeed> {
    let mut steps = template(template_id)
        .map(|item| item.steps)
        .unwrap_or_default()
        .into_iter()
        .filter(|step| source_kind != "text" || step.key != "transcribe")
        .map(|step| JobStepSeed { key: step.key, label: step.label })
        .collect::<Vec<_>>();
    let _ = multiple_items;
    for step in steps.iter_mut() {
        step.label = step.label.trim().to_string();
    }
    steps
}

#[derive(Debug, Clone)]
pub struct JobStepSeed {
    pub key: String,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_source_drops_transcribe_step() {
        let steps = steps_for("text-note", "text", false);
        assert!(steps.iter().all(|step| step.key != "transcribe"));
        assert_eq!(steps.first().map(|step| step.key.as_str()), Some("source"));
        assert_eq!(steps.last().map(|step| step.key.as_str()), Some("publish"));
    }

    #[test]
    fn video_source_keeps_transcribe() {
        let steps = steps_for("video-note", "bili", false);
        assert!(steps.iter().any(|step| step.key == "transcribe"));
    }

    #[test]
    fn every_template_ends_at_publish() {
        for template in builtin_templates() {
            assert_eq!(template.steps.last().map(|step| step.key.as_str()), Some("publish"), "{}", template.id);
        }
    }
}
