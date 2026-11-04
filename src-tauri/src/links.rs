//! Link rewriting for file operations.
//!
//! Two link dialects are understood:
//!   - Obsidian-style wikilinks: `[[研究/AI/推理]]`, `[[推理|别名]]`, `[[推理#小节]]`
//!   - Markdown inline links/images: `[文字](../研究/x.md)`、`![图](img/a.png)`
//!
//! When a note or folder moves, every link that points into the moved subtree
//! has to be rewritten, and relative links *inside* the moved subtree have to
//! be recomputed because their base directory changed. Both cases fall out of
//! one rule: resolve the link against the old location, map the target through
//! the move, then render against the new location.
//!
//! Deliberate limits (documented in README): fenced code blocks and inline code
//! are skipped so examples are not rewritten; reference-style links
//! (`[a][ref]`) are not followed; ambiguous bare wikilinks are left untouched
//! rather than guessed.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Wiki,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub kind: LinkKind,
    /// Target exactly as written (no anchor, no alias).
    pub target: String,
    /// `#section` including the `#`, or empty.
    pub anchor: String,
    /// `|alias` including the `|`, or empty.
    pub alias: String,
    /// Markdown label text.
    pub label: String,
    /// Markdown image marker.
    pub bang: bool,
}

impl Link {
    pub fn raw<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.end]
    }
}

#[derive(Default, Debug, Clone)]
pub struct LinkIndex {
    /// Lowercased vault-relative path -> canonical path.
    exact: HashMap<String, String>,
    /// Lowercased file stem -> canonical paths (Markdown only).
    by_stem: HashMap<String, Vec<String>>,
}

impl LinkIndex {
    pub fn build(paths: impl IntoIterator<Item = String>) -> Self {
        let mut index = LinkIndex::default();
        for path in paths {
            let lower = path.to_lowercase();
            index.exact.insert(lower, path.clone());
            if path.to_lowercase().ends_with(".md") {
                if let Some(stem) = file_stem(&path) {
                    index.by_stem.entry(stem.to_lowercase()).or_default().push(path);
                }
            }
        }
        for paths in index.by_stem.values_mut() {
            paths.sort();
        }
        index
    }

    /// Every known path, used to find notes inside a moved subtree.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.exact.values()
    }

    pub fn resolve(&self, from_rel: &str, kind: LinkKind, target: &str) -> Option<String> {
        let target = target.trim().replace('\\', "/");
        if target.is_empty() {
            return None;
        }
        match kind {
            LinkKind::Wiki => self.resolve_wiki(from_rel, &target),
            LinkKind::Markdown => self.resolve_markdown(from_rel, &target),
        }
    }

    fn resolve_wiki(&self, from_rel: &str, target: &str) -> Option<String> {
        let with_ext = if target.to_lowercase().ends_with(".md") {
            target.to_string()
        } else {
            format!("{target}.md")
        };
        if let Some(hit) = self.exact.get(&with_ext.to_lowercase()) {
            return Some(hit.clone());
        }
        if target.contains('/') {
            // Obsidian allows a suffix instead of the full path.
            let needle = format!("/{}", with_ext.to_lowercase());
            let hits: Vec<&String> = self
                .exact
                .iter()
                .filter(|(key, _)| key.ends_with(&needle))
                .map(|(_, value)| value)
                .collect();
            return match hits.len() {
                1 => Some(hits[0].clone()),
                _ => None,
            };
        }
        let stem = with_ext
            .strip_suffix(".md")
            .unwrap_or(&with_ext)
            .to_lowercase();
        let candidates = self.by_stem.get(&stem)?;
        if candidates.len() == 1 {
            return Some(candidates[0].clone());
        }
        // Ambiguous: prefer the note in the same folder, then the shallowest.
        let from_dir = parent_dir(from_rel);
        if let Some(same_dir) = candidates.iter().find(|path| parent_dir(path) == from_dir) {
            return Some(same_dir.clone());
        }
        let mut sorted = candidates.clone();
        sorted.sort_by_key(|path| path.split('/').count());
        if sorted.len() >= 2 && sorted[0].split('/').count() == sorted[1].split('/').count() {
            return None;
        }
        sorted.first().cloned()
    }

    fn resolve_markdown(&self, from_rel: &str, target: &str) -> Option<String> {
        let joined = if let Some(rest) = target.strip_prefix('/') {
            rest.to_string()
        } else {
            join_rel(parent_dir(from_rel), target)
        };
        let normalized = normalize_rel(&joined)?;
        self.exact.get(&normalized.to_lowercase()).cloned()
    }
}

/// One or more path rewrites, expressed as prefix pairs. A file move is a
/// single exact pair; a folder move is a directory pair that also covers every
/// descendant.
#[derive(Debug, Clone, Default)]
pub struct MoveSet {
    pairs: Vec<(String, String)>,
}

impl MoveSet {
    pub fn new(old_rel: &str, new_rel: &str) -> Self {
        MoveSet {
            pairs: vec![(old_rel.to_string(), new_rel.to_string())],
        }
    }

    pub fn map(&self, path: &str) -> Option<String> {
        for (old, new) in &self.pairs {
            if path == old {
                return Some(new.clone());
            }
            let prefix = format!("{old}/");
            if let Some(rest) = path.strip_prefix(&prefix) {
                return Some(format!("{new}/{rest}"));
            }
        }
        None
    }
}

/// Rewrites every link in `text` for a move. `source_old`/`source_new` are the
/// containing file's paths before and after the move (equal when the file did
/// not move). Returns the new text and how many links changed.
pub fn rewrite_document(
    text: &str,
    source_old: &str,
    source_new: &str,
    moves: &MoveSet,
    index: &LinkIndex,
) -> (String, usize) {
    let links = scan_links(text);
    if links.is_empty() {
        return (text.to_string(), 0);
    }

    let mut out = String::with_capacity(text.len() + 16);
    let mut cursor = 0usize;
    let mut changed = 0usize;

    for link in &links {
        let Some(target) = index.resolve(source_old, link.kind, &link.target) else {
            continue;
        };
        let mapped = moves.map(&target).unwrap_or(target.clone());
        let source_moved = source_old != source_new;
        if !source_moved && mapped == target {
            continue;
        }
        let rendered = render(link, &target, &mapped, source_new, index);
        if rendered == link.raw(text) {
            continue;
        }
        out.push_str(&text[cursor..link.start]);
        out.push_str(&rendered);
        cursor = link.end;
        changed += 1;
    }

    if changed == 0 {
        return (text.to_string(), 0);
    }
    out.push_str(&text[cursor..]);
    (out, changed)
}

fn render(
    link: &Link,
    old_target: &str,
    new_target: &str,
    source_new: &str,
    index: &LinkIndex,
) -> String {
    match link.kind {
        LinkKind::Wiki => {
            // Shape follows what the author wrote, not the resolved path.
            let had_path = link.target.contains('/');
            let stem = file_stem(new_target).unwrap_or(new_target);
            // The index still reflects the pre-move vault, so "unique" means
            // no *other* note claims this stem once the moved one is excluded.
            let claimed_by_others = index
                .by_stem
                .get(&stem.to_lowercase())
                .map(|paths| paths.iter().filter(|path| path.as_str() != old_target).count())
                .unwrap_or(0);
            let name = if had_path {
                strip_md(new_target).to_string()
            } else if claimed_by_others == 0 {
                stem.to_string()
            } else {
                strip_md(new_target).to_string()
            };
            format!("[[{name}{}{}]]", link.anchor, link.alias)
        }
        LinkKind::Markdown => {
            let href = if link.target.starts_with('/') {
                format!("/{}", new_target)
            } else {
                relative_path(parent_dir(source_new), new_target)
            };
            let bang = if link.bang { "!" } else { "" };
            format!("{bang}[{}]({href}{})", link.label, link.anchor)
        }
    }
}

fn strip_md(path: &str) -> &str {
    path.strip_suffix(".md").or_else(|| path.strip_suffix(".MD")).unwrap_or(path)
}

fn file_stem(path: &str) -> Option<&str> {
    path.rsplit('/').next()?.rsplit_once('.').map(|(stem, _)| stem)
}

fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

fn join_rel(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        dir.to_string()
    } else {
        format!("{dir}/{rel}")
    }
}

fn normalize_rel(path: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                if out.pop().is_none() {
                    return None;
                }
            }
            other => out.push(other),
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join("/"))
    }
}

fn relative_path(from_dir: &str, target: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|part| !part.is_empty()).collect();
    let to: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
    let mut shared = 0usize;
    while shared < from.len() && shared < to.len() && from[shared] == to[shared] {
        shared += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in shared..from.len() {
        parts.push("..".to_string());
    }
    for part in &to[shared..] {
        parts.push((*part).to_string());
    }
    if parts.is_empty() {
        target.to_string()
    } else {
        parts.join("/")
    }
}

/// Byte-level scan for links, skipping fenced code blocks and inline code.
pub fn scan_links(text: &str) -> Vec<Link> {
    let mut links = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0usize;

    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();

        let trimmed = line.trim_start();
        if let Some((marker, count)) = fence_marker(trimmed) {
            match fence {
                Some((open, open_count)) if open == marker && count >= open_count => fence = None,
                None => fence = Some((marker, count)),
                _ => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        scan_line(line, line_start, &mut links);
    }
    links
}

fn fence_marker(trimmed: &str) -> Option<(u8, usize)> {
    let bytes = trimmed.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let count = bytes.iter().take_while(|byte| **byte == marker).count();
    if count >= 3 { Some((marker, count)) } else { None }
}

fn scan_line(line: &str, line_start: usize, out: &mut Vec<Link>) {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut code_run = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = bytes[i..].iter().take_while(|byte| **byte == b'`').count();
                if code_run == 0 {
                    code_run = run;
                } else if code_run == run {
                    code_run = 0;
                }
                i += run;
            }
            b'[' if code_run == 0 => {
                if bytes.get(i + 1) == Some(&b'[') {
                    if let Some(link) = parse_wiki(line, i, line_start) {
                        i = link.end - line_start;
                        out.push(link);
                        continue;
                    }
                } else {
                    let bang = i > 0 && bytes[i - 1] == b'!';
                    if let Some(link) = parse_markdown(line, i, line_start, bang) {
                        i = link.end - line_start;
                        out.push(link);
                        continue;
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
}

fn parse_wiki(line: &str, start: usize, line_start: usize) -> Option<Link> {
    let close = line[start + 2..].find("]]")? + start + 2;
    let content = &line[start + 2..close];
    let (before_alias, alias) = match content.split_once('|') {
        Some((left, right)) => (left, format!("|{right}")),
        None => (content, String::new()),
    };
    let (target, anchor) = match before_alias.split_once('#') {
        Some((left, right)) => (left, format!("#{right}")),
        None => (before_alias, String::new()),
    };
    if target.trim().is_empty() {
        return None;
    }
    Some(Link {
        start: line_start + start,
        end: line_start + close + 2,
        kind: LinkKind::Wiki,
        target: target.to_string(),
        anchor,
        alias,
        label: String::new(),
        bang: false,
    })
}

fn parse_markdown(line: &str, start: usize, line_start: usize, bang: bool) -> Option<Link> {
    let close_bracket = line[start + 1..].find(']')? + start + 1;
    if line.as_bytes().get(close_bracket + 1) != Some(&b'(') {
        return None;
    }
    let open_paren = close_bracket + 1;
    let close_paren = find_closing_paren(line, open_paren + 1)?;
    let inner = line[open_paren + 1..close_paren].trim();

    let href = if inner.starts_with('<') {
        &inner[1..inner.find('>')?]
    } else {
        inner.split_whitespace().next().unwrap_or("")
    };
    if href.is_empty()
        || href.starts_with('#')
        || href.contains("://")
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
        || href.starts_with("data:")
    {
        return None;
    }

    let (target, anchor) = match href.split_once('#') {
        Some((left, right)) => (left, format!("#{right}")),
        None => (href, String::new()),
    };
    if target.is_empty() {
        return None;
    }

    Some(Link {
        start: line_start + if bang { start - 1 } else { start },
        end: line_start + close_paren + 1,
        kind: LinkKind::Markdown,
        target: target.to_string(),
        anchor,
        alias: String::new(),
        label: line[start + 1..close_bracket].to_string(),
        bang,
    })
}

fn find_closing_paren(line: &str, from: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut depth = 0usize;
    for i in from..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> LinkIndex {
        LinkIndex::build(
            [
                "研究/AI/推理并行.md",
                "研究/AI/算子融合.md",
                "日记/2026-10-20.md",
                "root.md",
                "assets/a.png",
            ]
            .iter()
            .map(|path| path.to_string()),
        )
    }

    #[test]
    fn scans_wikilinks_with_anchor_and_alias() {
        let links = scan_links("看 [[研究/AI/推理并行#并行|这里]] 和 [[算子融合]]");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "研究/AI/推理并行");
        assert_eq!(links[0].anchor, "#并行");
        assert_eq!(links[0].alias, "|这里");
        assert_eq!(links[1].target, "算子融合");
    }

    #[test]
    fn skips_code() {
        let text = "```\n[[不存在的]]\n```\n`[[也不改]]`\n真链接 [[root]]";
        let links = scan_links(text);
        assert_eq!(links.len(), 1, "only the real link: {:?}", links);
        assert_eq!(links[0].target, "root");
    }

    #[test]
    fn scans_markdown_links_and_images() {
        let links = scan_links("[文字](../研究/AI/推理并行.md) ![图](assets/a.png) [外链](https://x.y)");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "../研究/AI/推理并行.md");
        assert!(links[1].bang);
    }

    #[test]
    fn rewrites_wikilinks_pointing_at_moved_note() {
        let idx = index();
        let text = "见 [[研究/AI/推理并行|别名]] 和 [[研究/AI/推理并行#小节]]";
        let moves = MoveSet::new("研究/AI/推理并行.md", "研究/推理并行.md");
        let (out, changed) = rewrite_document(text, "root.md", "root.md", &moves, &idx);
        assert_eq!(changed, 2);
        assert_eq!(out, "见 [[研究/推理并行|别名]] 和 [[研究/推理并行#小节]]");
    }

    #[test]
    fn keeps_bare_wikilink_shape() {
        let idx = index();
        let moves = MoveSet::new("研究/AI/推理并行.md", "研究/推理并行.md");
        let (out, changed) = rewrite_document("[[推理并行]]", "root.md", "root.md", &moves, &idx);
        assert_eq!(changed, 0, "a bare stem still resolves after the move");
        assert_eq!(out, "[[推理并行]]");
    }

    #[test]
    fn relative_link_survives_when_both_sides_move() {
        let idx = index();
        let text = "[融合](AI/算子融合.md)";
        let moves = MoveSet::new("研究", "归档/研究");
        let (out, changed) =
            rewrite_document(text, "研究/笔记.md", "归档/研究/笔记.md", &moves, &idx);
        assert_eq!(changed, 0, "relative path is still correct, nothing to rewrite");
        assert_eq!(out, "[融合](AI/算子融合.md)");
    }

    #[test]
    fn recomputes_relative_link_to_outside_target() {
        let idx = index();
        let text = "[根](../../root.md)";
        let moves = MoveSet::new("研究/AI/推理并行.md", "研究/推理并行.md");
        let (out, changed) = rewrite_document(
            text,
            "研究/AI/推理并行.md",
            "研究/推理并行.md",
            &moves,
            &idx,
        );
        assert_eq!(changed, 1);
        assert_eq!(out, "[根](../root.md)");
    }

    #[test]
    fn folder_move_updates_every_descendant_link() {
        let idx = index();
        let text = "[[研究/AI/推理并行]] [[研究/AI/算子融合]]";
        let moves = MoveSet::new("研究/AI", "archive/AI");
        let (out, changed) = rewrite_document(text, "root.md", "root.md", &moves, &idx);
        assert_eq!(changed, 2);
        assert_eq!(out, "[[archive/AI/推理并行]] [[archive/AI/算子融合]]");
    }

    #[test]
    fn ambiguous_bare_link_is_left_alone() {
        let idx = LinkIndex::build(
            ["a/同名.md", "b/同名.md"].iter().map(|p| p.to_string()),
        );
        let moves = MoveSet::new("a/同名.md", "a/改名.md");
        let (out, changed) = rewrite_document("[[同名]]", "root.md", "root.md", &moves, &idx);
        assert_eq!(changed, 0, "cannot tell which 同名 is meant");
        assert_eq!(out, "[[同名]]");
    }

    #[test]
    fn image_links_follow_folder_moves() {
        let idx = index();
        let text = "![图](assets/a.png)";
        let moves = MoveSet::new("assets", "media");
        let (out, changed) = rewrite_document(text, "root.md", "root.md", &moves, &idx);
        assert_eq!(changed, 1);
        assert_eq!(out, "![图](media/a.png)");
    }
}
