//! Whiteboard storage, image assets and link metadata.
//!
//! The board is a small, free-positioned card surface. Rows live in
//! `yebian.sqlite` so they travel through the normal sync op log; image bytes
//! live in the vault under `whiteboard/assets/` and travel as blobs.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const DEFAULT_BOARD_ID: &str = "board_default";
pub const ASSET_DIR: &str = "whiteboard/assets";
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 20_000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0)
}

fn clip(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn open_db(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 30000;",
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteboardBoard {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteboardItem {
    pub id: String,
    pub board_id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub z: i64,
    pub text: String,
    pub title: String,
    pub url: String,
    pub asset_path: String,
    pub color: String,
    pub checked: bool,
    pub note_id: String,
    pub job_id: String,
    pub data_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteboardItemInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub board_id: String,
    pub kind: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_width")]
    pub w: f64,
    #[serde(default = "default_height")]
    pub h: f64,
    #[serde(default)]
    pub z: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub asset_path: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub note_id: String,
    #[serde(default)]
    pub job_id: String,
    #[serde(default)]
    pub data_json: String,
}

fn default_width() -> f64 {
    220.0
}

fn default_height() -> f64 {
    160.0
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedImage {
    pub path: String,
    pub data_url: String,
    pub size: usize,
    pub mime: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    pub url: String,
    pub title: String,
    pub domain: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchStart {
    pub item: WhiteboardItem,
    pub job_id: String,
}

pub struct WhiteboardStore {
    db_path: PathBuf,
}

impl WhiteboardStore {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        let store = Self { db_path: db_path.to_path_buf() };
        store.ensure_schema()?;
        Ok(store)
    }

    fn conn(&self) -> Result<Connection, String> {
        open_db(&self.db_path)
    }

    fn ensure_schema(&self) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS whiteboard_boards (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL DEFAULT '我的白板',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS whiteboard_items (
              id TEXT PRIMARY KEY,
              board_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              x REAL NOT NULL DEFAULT 0,
              y REAL NOT NULL DEFAULT 0,
              w REAL NOT NULL DEFAULT 220,
              h REAL NOT NULL DEFAULT 160,
              z INTEGER NOT NULL DEFAULT 0,
              text TEXT NOT NULL DEFAULT '',
              title TEXT NOT NULL DEFAULT '',
              url TEXT NOT NULL DEFAULT '',
              asset_path TEXT NOT NULL DEFAULT '',
              color TEXT NOT NULL DEFAULT '',
              checked INTEGER NOT NULL DEFAULT 0,
              note_id TEXT NOT NULL DEFAULT '',
              job_id TEXT NOT NULL DEFAULT '',
              data_json TEXT NOT NULL DEFAULT '{}',
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_whiteboard_items_board
              ON whiteboard_items(board_id, z, created_at);
            ",
        )
        .map_err(|error| error.to_string())?;
        let now = now_ms();
        conn.execute(
            "INSERT OR IGNORE INTO whiteboard_boards(id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![DEFAULT_BOARD_ID, "我的白板", now, now],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn board(&self) -> Result<WhiteboardBoard, String> {
        self.ensure_schema()?;
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id, name, created_at, updated_at FROM whiteboard_boards WHERE id = ?1",
            [DEFAULT_BOARD_ID],
            |row| {
                Ok(WhiteboardBoard {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .map_err(|error| error.to_string())
    }

    pub fn items(&self, board_id: &str) -> Result<Vec<WhiteboardItem>, String> {
        let conn = self.conn()?;
        let board_id = if board_id.trim().is_empty() { DEFAULT_BOARD_ID } else { board_id.trim() };
        let mut statement = conn
            .prepare(
                "SELECT id, board_id, kind, x, y, w, h, z, text, title, url, asset_path,
                        color, checked, note_id, job_id, data_json, created_at, updated_at
                 FROM whiteboard_items WHERE board_id = ?1 ORDER BY z ASC, created_at ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([board_id], read_item)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
    }

    pub fn item(&self, id: &str) -> Result<Option<WhiteboardItem>, String> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id, board_id, kind, x, y, w, h, z, text, title, url, asset_path,
                    color, checked, note_id, job_id, data_json, created_at, updated_at
             FROM whiteboard_items WHERE id = ?1",
            [id],
            read_item,
        )
        .optional()
        .map_err(|error| error.to_string())
    }

    pub fn save(&self, input: WhiteboardItemInput) -> Result<WhiteboardItem, String> {
        let kind = input.kind.trim();
        if !["sticky", "link", "image", "todo", "agent"].contains(&kind) {
            return Err(format!("未知的白板卡片类型：{kind}"));
        }
        let conn = self.conn()?;
        let board_id = if input.board_id.trim().is_empty() { DEFAULT_BOARD_ID } else { input.board_id.trim() };
        self.ensure_board(&conn, board_id)?;
        let id = if input.id.trim().is_empty() {
            format!("wi_{}", Uuid::new_v4().simple())
        } else {
            input.id.trim().to_string()
        };
        let created_at: Option<i64> = conn
            .query_row("SELECT created_at FROM whiteboard_items WHERE id = ?1", [&id], |row| row.get(0))
            .optional()
            .map_err(|error| error.to_string())?;
        let now = now_ms();
        let created_at = created_at.unwrap_or(now);
        let text = clip(&input.text, MAX_TEXT_CHARS);
        let title = clip(&input.title, 500);
        let url = clip(&input.url, 2_000);
        let asset_path = clip(&input.asset_path, 500);
        let color = clip(&input.color, 32);
        let note_id = clip(&input.note_id, 120);
        let job_id = clip(&input.job_id, 120);
        let data_json = if input.data_json.trim().is_empty() {
            "{}".to_string()
        } else {
            clip(&input.data_json, 200_000)
        };
        conn.execute(
            "INSERT INTO whiteboard_items(
                id, board_id, kind, x, y, w, h, z, text, title, url, asset_path,
                color, checked, note_id, job_id, data_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
             ON CONFLICT(id) DO UPDATE SET
                board_id = excluded.board_id,
                kind = excluded.kind,
                x = excluded.x,
                y = excluded.y,
                w = excluded.w,
                h = excluded.h,
                z = excluded.z,
                text = excluded.text,
                title = excluded.title,
                url = excluded.url,
                asset_path = excluded.asset_path,
                color = excluded.color,
                checked = excluded.checked,
                note_id = excluded.note_id,
                job_id = excluded.job_id,
                data_json = excluded.data_json,
                updated_at = excluded.updated_at",
            params![
                id,
                board_id,
                kind,
                input.x.clamp(-200_000.0, 200_000.0),
                input.y.clamp(-200_000.0, 200_000.0),
                input.w.clamp(120.0, 900.0),
                input.h.clamp(60.0, 2_000.0),
                input.z.clamp(0, 1_000_000),
                text,
                title,
                url,
                asset_path,
                color,
                if input.checked { 1 } else { 0 },
                note_id,
                job_id,
                data_json,
                created_at,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
        self.item(&id)?.ok_or_else(|| "白板卡片保存后读不回来".to_string())
    }

    fn ensure_board(&self, conn: &Connection, board_id: &str) -> Result<(), String> {
        let now = now_ms();
        conn.execute(
            "INSERT OR IGNORE INTO whiteboard_boards(id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![board_id, if board_id == DEFAULT_BOARD_ID { "我的白板" } else { "白板" }, now, now],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let conn = self.conn()?;
        let changed = conn
            .execute("DELETE FROM whiteboard_items WHERE id = ?1", [id])
            .map_err(|error| error.to_string())?;
        Ok(changed > 0)
    }

    pub fn attach_job(&self, id: &str, job_id: &str, data_json: &str) -> Result<WhiteboardItem, String> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE whiteboard_items SET job_id = ?1, data_json = ?2, updated_at = ?3 WHERE id = ?4",
            params![clip(job_id, 120), clip(data_json, 200_000), now_ms(), id],
        )
        .map_err(|error| error.to_string())?;
        self.item(id)?.ok_or_else(|| "白板卡片不存在".to_string())
    }
}

fn read_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<WhiteboardItem> {
    Ok(WhiteboardItem {
        id: row.get(0)?,
        board_id: row.get(1)?,
        kind: row.get(2)?,
        x: row.get(3)?,
        y: row.get(4)?,
        w: row.get(5)?,
        h: row.get(6)?,
        z: row.get(7)?,
        text: row.get(8)?,
        title: row.get(9)?,
        url: row.get(10)?,
        asset_path: row.get(11)?,
        color: row.get(12)?,
        checked: row.get::<_, i64>(13)? != 0,
        note_id: row.get(14)?,
        job_id: row.get(15)?,
        data_json: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn image_extension(ext: &str) -> Option<&'static str> {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        "gif" => Some("gif"),
        _ => None,
    }
}

fn mime_for(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    }
}

fn data_url(bytes: &[u8], ext: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{};base64,{}", mime_for(ext), encoded)
}

pub fn import_image(vault: &Path, source: &str) -> Result<ImportedImage, String> {
    let source_path = Path::new(source);
    let ext = source_path
        .extension()
        .and_then(|value| value.to_str())
        .and_then(image_extension)
        .ok_or_else(|| "只支持 png / jpg / webp / gif 图片".to_string())?;
    let metadata = std::fs::metadata(source_path).map_err(|error| format!("读取图片失败：{error}"))?;
    if metadata.len() as usize > MAX_IMAGE_BYTES {
        return Err(format!("图片超过 {} MB", MAX_IMAGE_BYTES / 1024 / 1024));
    }
    let bytes = std::fs::read(source_path).map_err(|error| format!("读取图片失败：{error}"))?;
    write_image(vault, &bytes, ext)
}

pub fn save_image_base64(vault: &Path, value: &str, ext: &str) -> Result<ImportedImage, String> {
    let ext = image_extension(ext).ok_or_else(|| "只支持 png / jpg / webp / gif 图片".to_string())?;
    let encoded = value.rsplit(',').next().unwrap_or(value).trim();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("图片数据无效：{error}"))?;
    if bytes.is_empty() {
        return Err("图片数据为空".to_string());
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!("图片超过 {} MB", MAX_IMAGE_BYTES / 1024 / 1024));
    }
    write_image(vault, &bytes, ext)
}

fn write_image(vault: &Path, bytes: &[u8], ext: &str) -> Result<ImportedImage, String> {
    let hash = hex::encode(Sha256::digest(bytes));
    let relative = format!("{}/{hash}.{ext}", ASSET_DIR);
    let target = vault.join(&relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建图片目录失败：{error}"))?;
    }
    if !target.exists() {
        std::fs::write(&target, bytes).map_err(|error| format!("写入图片失败：{error}"))?;
    }
    Ok(ImportedImage {
        path: relative,
        data_url: data_url(bytes, ext),
        size: bytes.len(),
        mime: mime_for(ext).to_string(),
    })
}

fn safe_asset_path(relative: &str) -> Result<PathBuf, String> {
    let normalized = relative.trim().replace('\\', "/");
    if !normalized.starts_with(&format!("{}/", ASSET_DIR)) {
        return Err("图片路径不在白板素材目录".to_string());
    }
    let path = Path::new(&normalized);
    if path.components().any(|component| !matches!(component, Component::Normal(_) | Component::CurDir)) {
        return Err("图片路径不合法".to_string());
    }
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .and_then(image_extension)
        .ok_or_else(|| "图片扩展名不支持".to_string())?;
    let _ = ext;
    Ok(path.to_path_buf())
}

pub fn asset_bytes(vault: &Path, relative: &str) -> Result<(Vec<u8>, &'static str), String> {
    let safe = safe_asset_path(relative)?;
    let full = vault.join(&safe);
    let metadata = std::fs::metadata(&full).map_err(|error| format!("图片还没有同步到本机：{error}"))?;
    if metadata.len() as usize > MAX_IMAGE_BYTES {
        return Err(format!("图片超过 {} MB", MAX_IMAGE_BYTES / 1024 / 1024));
    }
    let bytes = std::fs::read(&full).map_err(|error| format!("读取图片失败：{error}"))?;
    let ext = safe.extension().and_then(|value| value.to_str()).unwrap_or("jpg");
    Ok((bytes, mime_for(ext)))
}

pub fn asset_data(vault: &Path, relative: &str) -> Result<String, String> {
    let (bytes, mime) = asset_bytes(vault, relative)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

pub async fn link_preview(url: &str) -> LinkPreview {
    let parsed = url::Url::parse(url.trim()).ok();
    let domain = parsed
        .as_ref()
        .and_then(|value| value.host_str())
        .unwrap_or("")
        .to_string();
    let fallback = LinkPreview {
        url: url.trim().to_string(),
        title: if domain.is_empty() { "链接".to_string() } else { domain.clone() },
        domain: domain.clone(),
        description: String::new(),
    };
    let fetched = tokio::time::timeout(
        Duration::from_secs(15),
        crate::production::web_fetch::fetch(url.trim()),
    )
    .await;
    let Ok(Ok(page)) = fetched else { return fallback };
    let title = page.title.trim();
    let description = page
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect::<String>();
    LinkPreview {
        url: page.url,
        title: if title.is_empty() {
            if domain.is_empty() { "链接".to_string() } else { domain.clone() }
        } else {
            title.chars().take(160).collect()
        },
        domain,
        description,
    }
}

pub fn research_material(items: &[WhiteboardItem]) -> String {
    let mut out = String::from("# 白板选中素材\n");
    for (index, item) in items.iter().enumerate() {
        out.push_str(&format!("\n## 卡片 {} · {}\n", index + 1, item.kind));
        if !item.title.trim().is_empty() {
            out.push_str(&format!("标题：{}\n", item.title.trim()));
        }
        if !item.text.trim().is_empty() && item.kind != "image" {
            out.push_str(&format!("内容：{}\n", item.text.trim()));
        }
        if !item.url.trim().is_empty() {
            out.push_str(&format!("链接：{}\n", item.url.trim()));
        }
        if !item.asset_path.trim().is_empty() {
            out.push_str(&format!("图片：{}\n", item.asset_path.trim()));
            if !item.text.trim().is_empty() {
                out.push_str(&format!("图片说明：{}\n", item.text.trim()));
            }
        }
    }
    clip(&out, 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(label: &str) -> (PathBuf, WhiteboardStore) {
        let dir = std::env::temp_dir().join(format!("yebian-wb-{label}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = WhiteboardStore::open(&dir.join("yebian.sqlite")).unwrap();
        (dir, store)
    }

    #[test]
    fn items_round_trip_and_default_board_exists() {
        let (dir, store) = temp_store("crud");
        assert_eq!(store.board().unwrap().id, DEFAULT_BOARD_ID);
        let item = store
            .save(WhiteboardItemInput {
                id: String::new(),
                board_id: String::new(),
                kind: "sticky".into(),
                x: 12.0,
                y: 34.0,
                w: 240.0,
                h: 160.0,
                z: 3,
                text: "记一件事".into(),
                title: String::new(),
                url: String::new(),
                asset_path: String::new(),
                color: "gold".into(),
                checked: false,
                note_id: String::new(),
                job_id: String::new(),
                data_json: String::new(),
            })
            .unwrap();
        assert!(item.id.starts_with("wi_"));
        assert_eq!(store.items(DEFAULT_BOARD_ID).unwrap().len(), 1);
        assert!(store.item(&item.id).unwrap().is_some());
        assert!(store.delete(&item.id).unwrap());
        assert!(store.items(DEFAULT_BOARD_ID).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn image_assets_are_hashed_and_path_scoped() {
        let (dir, _store) = temp_store("image");
        let vault = dir.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let png = b"\x89PNG\r\n\x1a\nfake";
        let saved = write_image(&vault, png, "png").unwrap();
        assert!(saved.path.starts_with("whiteboard/assets/"));
        let again = write_image(&vault, png, "png").unwrap();
        assert_eq!(saved.path, again.path);
        assert!(asset_data(&vault, &saved.path).unwrap().starts_with("data:image/png;base64,"));
        assert!(asset_data(&vault, "../secret.png").is_err());
        assert!(asset_data(&vault, "Attachments/a.png").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn research_material_includes_text_links_and_images() {
        let item = WhiteboardItem {
            id: "wi_1".into(),
            board_id: DEFAULT_BOARD_ID.into(),
            kind: "link".into(),
            x: 0.0,
            y: 0.0,
            w: 220.0,
            h: 160.0,
            z: 0,
            text: "一段说明".into(),
            title: "标题".into(),
            url: "https://example.com".into(),
            asset_path: String::new(),
            color: String::new(),
            checked: false,
            note_id: String::new(),
            job_id: String::new(),
            data_json: "{}".into(),
            created_at: 0,
            updated_at: 0,
        };
        let material = research_material(&[item.clone()]);
        assert!(material.contains("https://example.com"));
        assert!(material.contains("一段说明"));

        let mut image = item;
        image.kind = "image".into();
        image.asset_path = "whiteboard/assets/a.png".into();
        image.text = "画面说明".into();
        let material = research_material(&[image]);
        assert_eq!(material.matches("画面说明").count(), 1);
        assert!(material.contains("图片：whiteboard/assets/a.png"));
    }
}
