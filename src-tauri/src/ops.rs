//! Vault write operations: create, rename, move, delete-to-trash.
//!
//! Everything here keeps three invariants:
//!   1. Nothing outside the vault is ever touched (segment validation plus a
//!      canonical prefix check on the resolved parent).
//!   2. A note's identity survives a move: its `id` is written into front
//!      matter *before* the file is renamed.
//!   3. Links are rewritten before the rename, using an index of the old
//!      layout, so the rewrite sees the vault as the links currently describe
//!      it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use uuid::Uuid;

use crate::db::Store;
use crate::links::{self, LinkIndex, MoveSet};
use crate::vault;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    pub rel_path: String,
    pub links_rewritten: usize,
    pub files_scanned: usize,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedNote {
    pub rel_path: String,
    pub id: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedEntry {
    pub rel_path: String,
    pub trash_path: String,
}

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validates a user-typed file or folder name. Rejects the characters Windows
/// refuses rather than silently mangling them: the user should see why.
pub fn sanitize_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("名字不能为空".into());
    }
    if name == "." || name == ".." {
        return Err("这个名字不可用".into());
    }
    if let Some(bad) = name.chars().find(|c| {
        matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0')
            || c.is_control()
    }) {
        return Err(format!("名字里不能有 {bad}"));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err("名字不能以点或空格结尾".into());
    }
    if name.chars().count() > 120 {
        return Err("名字太长（上限 120 字）".into());
    }
    let stem = name.split('.').next().unwrap_or(name).to_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(format!("{stem} 是系统保留名"));
    }
    Ok(name.to_string())
}

/// Resolves a vault-relative path, refusing anything that could escape.
pub fn resolve_rel(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let normalized = rel.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err("只接受笔记库内的相对路径".into());
    }
    let mut path = root.to_path_buf();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return Err("路径不能越出笔记库".into()),
            other => path.push(other),
        }
    }
    // Symlinks inside the vault could still point outside, so verify the
    // deepest existing ancestor resolves under the root. Comparing the parent
    // would reject the vault root itself (its parent is outside by design).
    let real_root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut probe = path.clone();
    loop {
        if probe.exists() {
            let real = probe.canonicalize().map_err(|error| error.to_string())?;
            if !real.starts_with(&real_root) {
                return Err("路径不能越出笔记库".into());
            }
            break;
        }
        match probe.parent() {
            Some(parent) => probe = parent.to_path_buf(),
            None => break,
        }
    }
    Ok(path)
}

fn join_rel(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn unique_name(parent_abs: &Path, base: &str, ext: &str) -> String {
    for n in 1..10_000 {
        let candidate = if n == 1 {
            format!("{base}{ext}")
        } else {
            format!("{base} {n}{ext}")
        };
        if !parent_abs.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}{ext}", &Uuid::new_v4().simple().to_string()[..6])
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        Ok(())
    } else {
        Err("目标文件夹不存在".into())
    }
}

pub fn create_note(root: &Path, parent_rel: &str, title: &str) -> Result<CreatedNote, String> {
    let parent = resolve_rel(root, parent_rel)?;
    ensure_dir(&parent)?;

    let raw_title = if title.trim().is_empty() { "未命名" } else { title.trim() };
    let stem = sanitize_name(&raw_title.replace(['/', '\\'], "-"))?;
    let file_name = unique_name(&parent, &stem, ".md");
    let rel_path = join_rel(parent_rel.trim_matches('/'), &file_name);
    let id = Uuid::new_v4().to_string();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let content = format!(
        "---\nid: {id}\n标题: {stem}\n创建日期: {today}\n---\n\n# {stem}\n\n",
    );

    let abs = parent.join(&file_name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&abs)
        .map_err(|error| format!("创建失败：{error}"))?;
    use std::io::Write;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("写入失败：{error}"))?;

    Ok(CreatedNote { rel_path, id })
}

pub fn create_folder(root: &Path, parent_rel: &str, name: &str) -> Result<String, String> {
    let parent = resolve_rel(root, parent_rel)?;
    ensure_dir(&parent)?;
    let safe = sanitize_name(name)?;
    let folder_name = unique_name(&parent, &safe, "");
    let abs = parent.join(&folder_name);
    fs::create_dir(&abs).map_err(|error| format!("创建失败：{error}"))?;
    Ok(join_rel(parent_rel.trim_matches('/'), &folder_name))
}

/// Renames the last path segment, keeping the entry in place.
pub fn rename_entry(
    root: &Path,
    store: &Store,
    rel: &str,
    new_name: &str,
) -> Result<MoveReport, String> {
    let abs = resolve_rel(root, rel)?;
    if !abs.exists() {
        return Err("找不到这个条目".into());
    }
    let is_dir = abs.is_dir();
    let mut safe = sanitize_name(new_name)?;
    if is_dir {
        if let Some(index) = safe.rfind('.') {
            safe.truncate(index);
            safe = safe.trim().to_string();
            if safe.is_empty() {
                return Err("名字不能为空".into());
            }
        }
    } else if !safe.to_lowercase().ends_with(".md") {
        safe.push_str(".md");
    }
    let parent_rel = parent_rel_of(rel);
    let new_rel = join_rel(&parent_rel, &safe);
    if new_rel == rel {
        return Ok(MoveReport {
            rel_path: new_rel,
            links_rewritten: 0,
            files_scanned: 0,
        });
    }
    if abs.parent().map(|parent| parent.join(&safe).exists()).unwrap_or(false) {
        return Err("这里已经有同名条目了".into());
    }
    perform_move(root, store, rel, &new_rel)
}

/// Moves an entry into another folder.
pub fn move_entry(
    root: &Path,
    store: &Store,
    rel: &str,
    new_parent_rel: &str,
) -> Result<MoveReport, String> {
    let abs = resolve_rel(root, rel)?;
    if !abs.exists() {
        return Err("找不到这个条目".into());
    }
    let target_parent = resolve_rel(root, new_parent_rel)?;
    ensure_dir(&target_parent)?;

    let name = abs
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| "无法确定名称".to_string())?;
    let new_rel = join_rel(new_parent_rel.trim_matches('/'), &name);
    if new_rel == rel {
        return Ok(MoveReport {
            rel_path: new_rel,
            links_rewritten: 0,
            files_scanned: 0,
        });
    }
    if abs.is_dir() {
        let prefix = format!("{rel}/");
        if new_rel == rel || new_rel.starts_with(&prefix) {
            return Err("不能把文件夹移动到自己里面".into());
        }
    }
    if target_parent.join(&name).exists() {
        return Err("目标文件夹里已经有同名条目了".into());
    }
    perform_move(root, store, rel, &new_rel)
}


fn stem_of(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".md").or_else(|| name.strip_suffix(".MD")).unwrap_or(name)
}

/// Keeps the displayed title in sync with the file name, but only when they
/// already agreed. A note may deliberately have a long title and a short file
/// name; renaming the file must not clobber that.
fn rename_note_text(text: &str, old_stem: &str, new_stem: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + 8);
    let mut in_front = false;
    let mut front_seen = false;
    let mut front_done = !text.starts_with("---");
    let mut title_done = false;
    let mut h1_done = false;
    let mut changed = false;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let eol = if line.ends_with('\n') { "\n" } else { "" };

        if !front_done {
            if trimmed.starts_with("---") {
                if front_seen {
                    in_front = false;
                    front_done = true;
                } else {
                    front_seen = true;
                    in_front = true;
                }
                out.push_str(line);
                continue;
            }
            if in_front && !title_done {
                if let Some((key, value)) = line.split_once(':') {
                    let unquoted = value.trim().trim_matches(|c| c == '"' || c == '\'');
                    if key.trim() == "标题" && unquoted == old_stem {
                        out.push_str(&format!("{key}: {new_stem}{eol}"));
                        title_done = true;
                        changed = true;
                        continue;
                    }
                }
            }
        }

        if front_done && !h1_done {
            if let Some(value) = trimmed.strip_prefix("# ") {
                if value.trim_end_matches(['\r', '\n']).trim() == old_stem {
                    out.push_str(&format!("# {new_stem}{eol}"));
                    h1_done = true;
                    changed = true;
                    continue;
                }
            }
        }

        out.push_str(line);
    }

    if changed { Some(out) } else { None }
}

fn parent_rel_of(rel: &str) -> String {
    match rel.rfind('/') {
        Some(index) => rel[..index].to_string(),
        None => String::new(),
    }
}

fn perform_move(
    root: &Path,
    store: &Store,
    old_rel: &str,
    new_rel: &str,
) -> Result<MoveReport, String> {
    let old_abs = resolve_rel(root, old_rel)?;
    let new_abs = resolve_rel(root, new_rel)?;
    if !old_abs.exists() {
        return Err("找不到这个条目".into());
    }
    if new_abs.exists() {
        return Err("目标位置已经有同名条目了".into());
    }

    // Identity first: a note without a front-matter `id` would otherwise be
    // re-keyed by its new path and lose every annotation pointing at it.
    // Folders need this for every note inside, before the rename.
    if old_abs.is_file() && is_markdown(old_rel) {
        let current = vault::front_id_of(
            &fs::read_to_string(&old_abs).map_err(|error| error.to_string())?,
        )
        .unwrap_or_else(|| vault::note_id(old_rel));
        vault::ensure_id(&old_abs, &current)?;
    } else if old_abs.is_dir() {
        for path in vault::walk_notes(&old_abs) {
            let note_rel = vault::rel_path(&path, root);
            let current = vault::front_id_of(
                &fs::read_to_string(&path).map_err(|error| error.to_string())?,
            )
            .unwrap_or_else(|| vault::note_id(&note_rel));
            vault::ensure_id(&path, &current)?;
        }
    }

    if old_abs.is_file() && is_markdown(old_rel) {
        let old_stem = stem_of(old_rel);
        let new_stem = stem_of(new_rel);
        if old_stem != new_stem {
            let text = fs::read_to_string(&old_abs).map_err(|error| error.to_string())?;
            if let Some(updated) = rename_note_text(&text, old_stem, new_stem) {
                vault::write_atomic(&old_abs, &updated)?;
            }
        }
    }

    // Link resolution has to see the vault as it is *before* the rename.
    let files: Vec<String> = vault::walk_all_files(root)
        .iter()
        .map(|path| vault::rel_path(path, root))
        .collect();
    let index = LinkIndex::build(files.iter().cloned());

    if let Some(parent) = new_abs.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::rename(&old_abs, &new_abs).map_err(|error| format!("移动失败：{error}"))?;

    // Only after the rename succeeded: a failed rename must not leave links
    // pointing at a path that never came into existence.
    let moves = MoveSet::new(old_rel, new_rel);
    let (rewritten, scanned) = rewrite_links(root, store, old_rel, &moves, &index)?;

    Ok(MoveReport {
        rel_path: new_rel.to_string(),
        links_rewritten: rewritten,
        files_scanned: scanned,
    })
}

fn is_markdown(rel: &str) -> bool {
    rel.to_lowercase().ends_with(".md")
}

/// Rewrites links in every note that might mention the moved entry, plus every
/// note inside the moved subtree (their relative links change base directory).
fn rewrite_links(
    root: &Path,
    store: &Store,
    moved_rel: &str,
    moves: &MoveSet,
    index: &LinkIndex,
) -> Result<(usize, usize), String> {

    // A link has to mention either the file name or a path segment; use that
    // to avoid reading the whole vault on large libraries.
    let name = moved_rel.rsplit('/').next().unwrap_or(moved_rel);
    let stem = name.strip_suffix(".md").unwrap_or(name);
    let mut needles = vec![stem.to_string(), moved_rel.to_string()];
    if let Some(folder) = moved_rel.rsplit('/').next() {
        if !folder.contains('.') {
            needles.push(folder.to_string());
        }
    }

    let mut candidates: BTreeSet<String> = store.notes_containing(&needles)?.into_iter().collect();
    let prefix = format!("{moved_rel}/");
    for rel in index.paths() {
        if rel == moved_rel || rel.starts_with(&prefix) {
            if is_markdown(rel) {
                candidates.insert(rel.clone());
            }
        }
    }

    let scanned = candidates.len();
    let mut total = 0usize;
    for rel in candidates {
        // The rename already happened, so the file lives at its new path.
        let source_new = moves.map(&rel).unwrap_or_else(|| rel.clone());
        let abs = root.join(&source_new);
        let Ok(text) = fs::read_to_string(&abs) else {
            continue;
        };
        let (updated, changed) = links::rewrite_document(&text, &rel, &source_new, moves, index);
        if changed > 0 {
            vault::write_atomic(&abs, &updated)?;
            total += changed;
        }
    }
    Ok((total, scanned))
}

/// Moves an entry into `.trash/<timestamp>/<original path>`.
pub fn delete_entry(root: &Path, rel: &str) -> Result<DeletedEntry, String> {
    let abs = resolve_rel(root, rel)?;
    if !abs.exists() {
        return Err("找不到这个条目".into());
    }
    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let batch = format!(".trash/{stamp}-{}", &Uuid::new_v4().simple().to_string()[..6]);
    let target = resolve_rel(root, &format!("{batch}/{rel}"))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::rename(&abs, &target).map_err(|error| format!("删除失败：{error}"))?;
    Ok(DeletedEntry {
        rel_path: rel.to_string(),
        trash_path: format!("{batch}/{rel}"),
    })
}

pub fn restore_entry(root: &Path, trash_rel: &str, original_rel: &str) -> Result<String, String> {
    let source = resolve_rel(root, trash_rel)?;
    if !source.exists() {
        return Err("回收站里找不到这个条目".into());
    }
    let target = resolve_rel(root, original_rel)?;
    if target.exists() {
        return Err("原位置已经有同名条目了".into());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::rename(&source, &target).map_err(|error| format!("恢复失败：{error}"))?;
    Ok(original_rel.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_and_reserved_names() {
        assert!(sanitize_name("..").is_err());
        assert!(sanitize_name("a/b").is_err());
        assert!(sanitize_name("CON").is_err());
        assert!(sanitize_name("名字.").is_err());
        assert_eq!(sanitize_name(" 正常的名字 ").unwrap(), "正常的名字");
    }

    #[test]
    fn resolves_only_inside_the_vault() {
        let root = std::env::temp_dir();
        assert!(resolve_rel(&root, "../x.md").is_err());
        assert!(resolve_rel(&root, "C:/Windows/x.md").is_err());
        assert!(resolve_rel(&root, "/etc/passwd").is_err());
        assert!(resolve_rel(&root, "a/b.md").is_ok());
    }

    #[test]
    fn unique_name_adds_a_counter() {
        let dir = std::env::temp_dir().join(format!("yebian-test-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("笔记.md"), "x").unwrap();
        assert_eq!(unique_name(&dir, "笔记", ".md"), "笔记 2.md");
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use crate::db::Store;

    struct Fixture {
        root: PathBuf,
        store: Store,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn fixture(files: &[(&str, &str)]) -> Fixture {
        let base = std::env::temp_dir().join(format!("yebian-ops-{}", Uuid::new_v4().simple()));
        let root = base.join("vault");
        fs::create_dir_all(&root).unwrap();
        for (rel, body) in files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
        }
        let store = Store::open(&base.join("index.sqlite")).unwrap();
        store.index_vault(&root, true).unwrap();
        Fixture { root, store }
    }

    fn note_body(title: &str, link: &str) -> String {
        format!("---\n标题: {title}\n---\n\n# {title}\n\n{link}\n")
    }

    #[test]
    fn rename_preserves_identity_and_rewrites_links() {
        let fx = fixture(&[
            ("a.md", &note_body("甲", "见 [[b]] 和 ![图](img/p.png)")),
            ("b.md", &note_body("乙", "正文")),
        ]);
        fs::create_dir_all(fx.root.join("img")).unwrap();
        fs::write(fx.root.join("img/p.png"), b"png").unwrap();
        fx.store.index_vault(&fx.root, true).unwrap();

        let b = fx.store.list_notes().unwrap().into_iter().find(|n| n.title == "乙").unwrap();
        let b_id = b.id.clone();
        fx.store.add_annotation(&b_id, "blk", "引用", "注", "标记").unwrap();

        let report = rename_entry(&fx.root, &fx.store, "b.md", "乙二").unwrap();
        assert_eq!(report.rel_path, "乙二.md");
        assert_eq!(report.links_rewritten, 1, "{report:?}");

        let a = fs::read_to_string(fx.root.join("a.md")).unwrap();
        assert!(a.contains("[[乙二]]"), "link rewritten, got: {a}");

        fx.store.index_vault(&fx.root, false).unwrap();
        let renamed = fx
            .store
            .list_notes()
            .unwrap()
            .into_iter()
            .find(|n| n.rel_path == "乙二.md")
            .expect("renamed note indexed");
        assert_eq!(renamed.id, b_id, "id must survive a rename");
        assert_eq!(fx.store.list_annotations(&b_id).unwrap().len(), 1);
    }

    #[test]
    fn moving_a_folder_keeps_every_id_and_rebases_links() {
        let fx = fixture(&[
            ("研究/AI/x.md", &note_body("推理", "见 [[丙]]")),
            ("研究/note.md", &note_body("笔记", "[丙](../丙.md)")),
            ("丙.md", &note_body("丙", "正文")),
        ]);
        let before = fx.store.list_notes().unwrap();
        let x_id = before.iter().find(|n| n.rel_path == "研究/AI/x.md").unwrap().id.clone();
        fx.store.add_annotation(&x_id, "blk", "引用", "", "标记").unwrap();

        create_folder(&fx.root, "", "归档").unwrap();
        let report = move_entry(&fx.root, &fx.store, "研究", "归档").unwrap();
        assert_eq!(report.rel_path, "归档/研究");
        assert!(!fx.root.join("研究").exists());

        fx.store.index_vault(&fx.root, false).unwrap();
        let after = fx.store.list_notes().unwrap();
        let moved = after.iter().find(|n| n.rel_path == "归档/研究/AI/x.md").unwrap();
        assert_eq!(moved.id, x_id, "folder move must keep note identity");

        // Relative link inside the moved folder now points at a target whose
        // absolute location did not change: it has to be rebased.
        let note = fs::read_to_string(fx.root.join("归档/研究/note.md")).unwrap();
        assert!(note.contains("[丙](../../丙.md)"), "got: {note}");
    }

    #[test]
    fn delete_goes_to_trash_and_restore_brings_it_back() {
        let fx = fixture(&[("b.md", &note_body("乙", "正文"))]);
        let deleted = delete_entry(&fx.root, "b.md").unwrap();
        assert!(!fx.root.join("b.md").exists());
        assert!(deleted.trash_path.starts_with(".trash/"));
        assert!(fx.root.join(&deleted.trash_path).exists());

        restore_entry(&fx.root, &deleted.trash_path, "b.md").unwrap();
        assert!(fx.root.join("b.md").exists());

        fx.store.index_vault(&fx.root, false).unwrap();
        assert_eq!(fx.store.list_notes().unwrap().len(), 1);
    }

    #[test]
    fn external_move_is_reconciled_by_content() {
        let fx = fixture(&[("旧名字.md", &note_body("要保留标注的笔记", "正文"))]);
        let before = fx.store.list_notes().unwrap();
        let id = before[0].id.clone();
        fx.store.add_annotation(&id, "blk", "引用", "", "标记").unwrap();

        // Simulate Explorer: the app never sees a write, only the new path.
        fs::rename(fx.root.join("旧名字.md"), fx.root.join("新名字.md")).unwrap();
        fx.store.index_vault(&fx.root, false).unwrap();

        let after = fx.store.list_notes().unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].rel_path, "新名字.md");
        assert_eq!(after[0].id, id, "content fingerprint re-attached the old id");
        assert_eq!(fx.store.list_annotations(&id).unwrap().len(), 1);

        let content = fs::read_to_string(fx.root.join("新名字.md")).unwrap();
        assert!(content.contains("id:"), "the recovered id is written back");
    }

    #[test]
    fn create_note_writes_a_stable_id_and_unique_name() {
        let fx = fixture(&[]);
        let first = create_note(&fx.root, "", "新想法").unwrap();
        let second = create_note(&fx.root, "", "新想法").unwrap();
        assert_eq!(first.rel_path, "新想法.md");
        assert_eq!(second.rel_path, "新想法 2.md");
        let text = fs::read_to_string(fx.root.join(&first.rel_path)).unwrap();
        assert!(text.contains(&format!("id: {}", first.id)));
        assert!(text.contains("# 新想法"));
    }
}

#[cfg(test)]
mod title_sync {
    use super::*;
    use crate::db::Store;

    fn fixture(files: &[(&str, &str)]) -> (PathBuf, Store) {
        let base = std::env::temp_dir().join(format!("yebian-title-{}", Uuid::new_v4().simple()));
        let root = base.join("vault");
        fs::create_dir_all(&root).unwrap();
        for (rel, body) in files {
            fs::write(root.join(rel), body).unwrap();
        }
        let store = Store::open(&base.join("index.sqlite")).unwrap();
        store.index_vault(&root, true).unwrap();
        (root, store)
    }

    #[test]
    fn rename_syncs_title_and_heading_when_they_match() {
        let (root, store) = fixture(&[("旧名.md", "---\n标题: 旧名\n---\n\n# 旧名\n\n正文\n")]);
        rename_entry(&root, &store, "旧名.md", "新名").unwrap();
        let text = fs::read_to_string(root.join("新名.md")).unwrap();
        assert!(text.contains("标题: 新名"), "{text}");
        assert!(text.contains("# 新名"), "{text}");
        assert!(!text.contains("旧名"), "{text}");
    }

    #[test]
    fn rename_keeps_a_deliberate_long_title() {
        let (root, store) = fixture(&[
            ("短名.md", "---\n标题: 一个很长很长的标题\n---\n\n# 一个很长很长的标题\n\n正文\n"),
        ]);
        rename_entry(&root, &store, "短名.md", "新短名").unwrap();
        let text = fs::read_to_string(root.join("新短名.md")).unwrap();
        assert!(text.contains("标题: 一个很长很长的标题"), "{text}");
        assert!(text.contains("# 一个很长很长的标题"), "{text}");
    }
}
