//! Runtime plugin host metadata. Third-party plugin code is executed in a
//! sandboxed Web Worker on the frontend; this module only owns the manifest
//! contract, the local plugin directory and the per-device enabled/permission
//! state. It never exposes Tauri IPC or the vault to plugin code.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const API_VERSION: u32 = 2;
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAX_SOURCE_BYTES: u64 = 512 * 1024;
pub const MAX_COMMANDS: usize = 20;
pub const MAX_SETTINGS: usize = 20;
pub const MAX_PANELS: usize = 5;
pub const MAX_PANEL_FIELDS: usize = 6;
pub const ALLOWED_PERMISSIONS: &[&str] = &["notes:read"];
const EXAMPLE_PLUGINS: &[(&str, &str, &str)] = &[
    (
        "example.word-count",
        include_str!("../plugins/examples/word-count/plugin.json"),
        include_str!("../plugins/examples/word-count/main.js"),
    ),
    (
        "example.heading-audit",
        include_str!("../plugins/examples/heading-audit/plugin.json"),
        include_str!("../plugins/examples/heading-audit/main.js"),
    ),
    (
        "example.link-audit",
        include_str!("../plugins/examples/link-audit/plugin.json"),
        include_str!("../plugins/examples/link-audit/main.js"),
    ),
    (
        "jiangshan.atlas",
        include_str!("../plugins/examples/jiangshan-atlas/plugin.json"),
        include_str!("../plugins/examples/jiangshan-atlas/main.js"),
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub api_version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub main: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub settings: Vec<PluginSetting>,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    #[serde(default)]
    pub panels: Vec<PluginPanel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSetting {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub default: Value,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub option_labels: BTreeMap<String, String>,
    #[serde(default)]
    pub placeholder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_context")]
    pub context: String,
    /// Primary commands get a direct button in the note action bar; all other
    /// commands stay in the overflow menu.
    #[serde(default)]
    pub primary: bool,
    /// Host icon token; unknown values fall back to a puzzle piece.
    #[serde(default = "default_icon")]
    pub icon: String,
    /// `always` or `history`; the host hides history-only actions on other notes.
    #[serde(default = "default_when")]
    pub when: String,
}

fn default_context() -> String {
    "note".into()
}

fn default_icon() -> String {
    "puzzle".into()
}

fn default_when() -> String {
    "always".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPanelField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type", default = "default_field_kind")]
    pub kind: String,
    #[serde(default)]
    pub placeholder: String,
}

fn default_field_kind() -> String {
    "text".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPanel {
    pub id: String,
    pub title: String,
    /// HTTPS URL template. Placeholders are `{key}` and are filled by the host
    /// from the plugin's settings, command params and panel fields.
    pub url: String,
    #[serde(default = "default_panel_width")]
    pub width: u32,
    #[serde(default = "default_panel_height")]
    pub height: u32,
    #[serde(default)]
    pub fields: Vec<PluginPanelField>,
    /// Optional host context resolver. `historical-map` enables the AI locate
    /// action in the panel header; empty panels get no host-specific behavior.
    #[serde(default)]
    pub context: String,
}

fn default_panel_width() -> u32 {
    1120
}

fn default_panel_height() -> u32 {
    780
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub description: String,
    pub author: String,
    pub main: String,
    pub permissions: Vec<String>,
    pub granted_permissions: Vec<String>,
    pub enabled: bool,
    pub needs_permission: bool,
    pub settings: Vec<PluginSetting>,
    pub settings_values: Value,
    pub commands: Vec<PluginCommand>,
    #[serde(default)]
    pub panels: Vec<PluginPanel>,
    pub error: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PluginState {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    settings: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    plugins: BTreeMap<String, PluginState>,
}

pub struct PluginManager {
    root: PathBuf,
    state: Mutex<StateFile>,
}

impl PluginManager {
    pub fn open(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        let state = load_state(&root);
        Self { root, state: Mutex::new(state) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Read every plugin directory and merge it with the local enabled state.
    /// Invalid manifests stay in the list with an `error` so the manager can
    /// explain what failed instead of hiding the folder.
    pub fn scan(&self) -> Vec<PluginInfo> {
        let state = self.state.lock().map(|guard| guard.clone()).unwrap_or_default();
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.root) else {
            return out;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let dir_id = dir.file_name().and_then(|name| name.to_str()).unwrap_or("").to_string();
            if dir_id.is_empty() || dir_id == "assets" {
                continue;
            }
            let manifest_path = dir.join("plugin.json");
            let fallback = PluginInfo {
                id: dir_id.clone(),
                name: dir_id.clone(),
                version: String::new(),
                api_version: 0,
                description: String::new(),
                author: String::new(),
                main: String::new(),
                permissions: Vec::new(),
                granted_permissions: Vec::new(),
                enabled: false,
                needs_permission: false,
                settings: Vec::new(),
                settings_values: json!({}),
                commands: Vec::new(),
                panels: Vec::new(),
                error: String::new(),
                path: dir.to_string_lossy().to_string(),
            };
            let manifest_bytes = match fs::metadata(&manifest_path) {
                Ok(meta) if meta.len() <= MAX_MANIFEST_BYTES => match fs::read_to_string(&manifest_path) {
                    Ok(text) => text,
                    Err(error) => {
                        out.push(PluginInfo { error: format!("读取 plugin.json 失败：{error}"), ..fallback });
                        continue;
                    }
                },
                Ok(meta) => {
                    out.push(PluginInfo {
                        error: format!("plugin.json 超过 {} KiB", MAX_MANIFEST_BYTES / 1024),
                        ..fallback
                    });
                    continue;
                }
                Err(_) => {
                    out.push(PluginInfo { error: "缺少 plugin.json".into(), ..fallback });
                    continue;
                }
            };
            let manifest: PluginManifest = match serde_json::from_str(&manifest_bytes) {
                Ok(manifest) => manifest,
                Err(error) => {
                    out.push(PluginInfo { error: format!("plugin.json 解析失败：{error}"), ..fallback });
                    continue;
                }
            };
            let mut info = info_from_manifest(&manifest, &dir);
            if info.error.is_empty() && manifest.id != dir_id {
                info.error = format!("manifest id「{}」与目录名「{dir_id}」不一致", manifest.id);
            }
            if info.error.is_empty() {
                let saved = state.plugins.get(&manifest.id).cloned().unwrap_or_default();
                let granted = saved.permissions.clone();
                let missing = manifest
                    .permissions
                    .iter()
                    .any(|permission| !granted.contains(permission));
                info.enabled = saved.enabled;
                info.granted_permissions = granted;
                info.needs_permission = saved.enabled && missing;
                info.settings_values = normalize_settings(&manifest.settings, saved.settings);
            }
            out.push(info);
        }
        out.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        out
    }

    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
        permissions: Vec<String>,
    ) -> Result<PluginInfo, String> {
        let manifest = self.load_manifest(id)?;
        for permission in &permissions {
            if !ALLOWED_PERMISSIONS.contains(&permission.as_str()) {
                return Err(format!("不支持的权限：{permission}"));
            }
            if !manifest.permissions.contains(permission) {
                return Err(format!("插件没有声明权限：{permission}"));
            }
        }
        if enabled {
            for permission in &manifest.permissions {
                if !permissions.contains(permission) {
                    return Err(format!("启用前需要授予权限：{permission}"));
                }
            }
        }
        let mut state = self.state.lock().map_err(|_| "插件状态锁损坏")?;
        let entry = state.plugins.entry(id.to_string()).or_default();
        entry.enabled = enabled;
        entry.permissions = if enabled { permissions } else { entry.permissions.clone() };
        save_state(&self.root, &state)?;
        drop(state);
        self.find_info(id)
    }

    pub fn save_settings(&self, id: &str, settings: Value) -> Result<PluginInfo, String> {
        let manifest = self.load_manifest(id)?;
        validate_settings(&manifest.settings, &settings)?;
        let mut state = self.state.lock().map_err(|_| "插件状态锁损坏")?;
        let entry = state.plugins.entry(id.to_string()).or_default();
        entry.settings = settings;
        save_state(&self.root, &state)?;
        drop(state);
        self.find_info(id)
    }

    pub fn source(&self, id: &str) -> Result<String, String> {
        let manifest = self.load_manifest(id)?;
        let info = self.find_info(id)?;
        if info.error.is_empty() && !info.enabled {
            return Err("插件未启用".into());
        }
        if info.needs_permission {
            return Err("插件缺少已授权的权限，请在插件页重新授权".into());
        }
        let path = self.root.join(id).join(&manifest.main);
        let meta = fs::metadata(&path).map_err(|_| "插件入口文件不存在".to_string())?;
        if meta.len() > MAX_SOURCE_BYTES {
            return Err(format!("插件入口超过 {} KiB", MAX_SOURCE_BYTES / 1024));
        }
        fs::read_to_string(&path).map_err(|error| format!("读取插件入口失败：{error}"))
    }

    /// Installs bundled plugins once, and upgrades them when the bundled
    /// manifest version is newer. User state (enabled/permissions/settings) is
    /// kept because only the plugin files are replaced.
    pub fn install_examples(&self) -> Result<Vec<PluginInfo>, String> {
        for (id, manifest, main) in EXAMPLE_PLUGINS {
            let dir = self.root.join(id);
            if dir.exists() {
                let existing = fs::read_to_string(dir.join("plugin.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<PluginManifest>(&text).ok())
                    .map(|value| value.version)
                    .unwrap_or_default();
                let bundled = serde_json::from_str::<PluginManifest>(manifest)
                    .map(|value| value.version)
                    .unwrap_or_default();
                if version_cmp(&bundled, &existing) <= 0 {
                    continue;
                }
            }
            fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            fs::write(dir.join("plugin.json"), manifest).map_err(|error| error.to_string())?;
            fs::write(dir.join("main.js"), main).map_err(|error| error.to_string())?;
        }
        Ok(self.scan())
    }

    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        let dir = self.root.join(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|error| format!("删除插件目录失败：{error}"))?;
        }
        let mut state = self.state.lock().map_err(|_| "插件状态锁损坏")?;
        state.plugins.remove(id);
        save_state(&self.root, &state)
    }

    pub fn open_dir(&self) -> Result<String, String> {
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer.exe")
                .arg(&self.root)
                .spawn()
                .map_err(|error| error.to_string())?;
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&self.root)
                .spawn()
                .map_err(|error| error.to_string())?;
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            std::process::Command::new("xdg-open")
                .arg(&self.root)
                .spawn()
                .map_err(|error| error.to_string())?;
        }
        Ok(self.root.to_string_lossy().to_string())
    }

    fn find_info(&self, id: &str) -> Result<PluginInfo, String> {
        self.scan()
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "插件不存在".to_string())
    }

    fn load_manifest(&self, id: &str) -> Result<PluginManifest, String> {
        validate_id(id)?;
        let path = self.root.join(id).join("plugin.json");
        let text = fs::read_to_string(path).map_err(|_| "插件不存在".to_string())?;
        let manifest: PluginManifest = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let info = info_from_manifest(&manifest, &self.root.join(id));
        if !info.error.is_empty() {
            return Err(info.error);
        }
        Ok(manifest)
    }
}

fn info_from_manifest(manifest: &PluginManifest, dir: &Path) -> PluginInfo {
    let mut error = validate_manifest(manifest);
    let main_path = dir.join(&manifest.main);
    if error.is_empty() {
        match fs::metadata(&main_path) {
            Ok(meta) if meta.len() <= MAX_SOURCE_BYTES => {}
            Ok(_) => error = format!("入口文件超过 {} KiB", MAX_SOURCE_BYTES / 1024),
            Err(_) => error = format!("入口文件不存在：{}", manifest.main),
        }
    }
    PluginInfo {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        api_version: manifest.api_version,
        description: manifest.description.clone(),
        author: manifest.author.clone(),
        main: manifest.main.clone(),
        permissions: manifest.permissions.clone(),
        granted_permissions: Vec::new(),
        enabled: false,
        needs_permission: false,
        settings: manifest.settings.clone(),
        settings_values: normalize_settings(&manifest.settings, Value::Null),
        commands: manifest.commands.clone(),
        panels: manifest.panels.clone(),
        error,
        path: dir.to_string_lossy().to_string(),
    }
}

fn validate_manifest(manifest: &PluginManifest) -> String {
    if let Err(error) = validate_id(&manifest.id) {
        return error;
    }
    if manifest.name.trim().is_empty() || manifest.name.chars().count() > 60 {
        return "name 必须是 1–60 个字符".into();
    }
    if !valid_version(&manifest.version) {
        return "version 必须是 x.y.z".into();
    }
    if !matches!(manifest.api_version, 1 | 2) {
        return format!("不支持的 apiVersion：{}（支持 1、2）", manifest.api_version);
    }
    if !manifest.panels.is_empty() && manifest.api_version < 2 {
        return "panels 需要 apiVersion 2".into();
    }
    let main = Path::new(&manifest.main);
    if main.file_name().and_then(|name| name.to_str()) != Some(manifest.main.as_str())
        || !manifest.main.to_ascii_lowercase().ends_with(".js")
    {
        return "main 必须是插件目录下的单个 .js 文件".into();
    }
    for permission in &manifest.permissions {
        if !ALLOWED_PERMISSIONS.contains(&permission.as_str()) {
            return format!("不支持的权限：{permission}");
        }
    }
    if manifest.commands.len() > MAX_COMMANDS {
        return format!("commands 最多 {MAX_COMMANDS} 个");
    }
    let mut command_ids = std::collections::HashSet::new();
    for command in &manifest.commands {
        if !valid_key(&command.id) {
            return format!("command id 不合法：{}", command.id);
        }
        if !command_ids.insert(command.id.clone()) {
            return format!("command id 重复：{}", command.id);
        }
        if command.title.trim().is_empty() {
            return format!("command「{}」缺少 title", command.id);
        }
        if command.context != "note" {
            return format!("command「{}」的 context 目前只支持 note", command.id);
        }
        if !matches!(command.when.as_str(), "always" | "history") {
            return format!("command「{}」的 when 不支持：{}", command.id, command.when);
        }
        if !valid_key(&command.icon) || command.icon.len() > 32 {
            return format!("command「{}」的 icon 不合法", command.id);
        }
    }
    if manifest.settings.len() > MAX_SETTINGS {
        return format!("settings 最多 {MAX_SETTINGS} 个");
    }
    let mut setting_keys = std::collections::HashSet::new();
    for setting in &manifest.settings {
        if !valid_key(&setting.key) {
            return format!("setting key 不合法：{}", setting.key);
        }
        if !setting_keys.insert(setting.key.clone()) {
            return format!("setting key 重复：{}", setting.key);
        }
        if !matches!(setting.kind.as_str(), "boolean" | "text" | "select" | "number") {
            return format!("setting「{}」的 type 不支持：{}", setting.key, setting.kind);
        }
        if !setting.default.is_null() && !setting_type_matches(&setting.kind, &setting.default) {
            return format!("setting「{}」的 default 类型不匹配", setting.key);
        }
        if !setting.option_labels.is_empty() {
            if setting.kind != "select" {
                return format!("setting「{}」只有 select 可以使用 optionLabels", setting.key);
            }
            for key in setting.option_labels.keys() {
                if !setting.options.contains(key) {
                    return format!("setting「{}」的 optionLabel 没有对应选项：{key}", setting.key);
                }
            }
        }
    }
    if manifest.panels.len() > MAX_PANELS {
        return format!("panels 最多 {MAX_PANELS} 个");
    }
    let mut panel_ids = std::collections::HashSet::new();
    for panel in &manifest.panels {
        if !valid_key(&panel.id) {
            return format!("panel id 不合法：{}", panel.id);
        }
        if !panel_ids.insert(panel.id.clone()) {
            return format!("panel id 重复：{}", panel.id);
        }
        if panel.title.trim().is_empty() || panel.title.chars().count() > 60 {
            return format!("panel「{}」的 title 必须是 1–60 个字符", panel.id);
        }
        if !valid_panel_url(&panel.url) {
            return format!("panel「{}」的 url 必须是不带未知占位符的 https 地址", panel.id);
        }
        if !(480..=1800).contains(&panel.width) || !(320..=1200).contains(&panel.height) {
            return format!("panel「{}」的尺寸超出范围", panel.id);
        }
        if !matches!(panel.context.as_str(), "" | "historical-map") {
            return format!("panel「{}」的 context 不支持：{}", panel.id, panel.context);
        }
        if panel.fields.len() > MAX_PANEL_FIELDS {
            return format!("panel「{}」最多 {MAX_PANEL_FIELDS} 个字段", panel.id);
        }
        let mut field_keys = std::collections::HashSet::new();
        for field in &panel.fields {
            if !valid_key(&field.key) {
                return format!("panel「{}」的字段 key 不合法：{}", panel.id, field.key);
            }
            if !field_keys.insert(field.key.clone()) {
                return format!("panel「{}」的字段 key 重复：{}", panel.id, field.key);
            }
            if field.label.trim().is_empty() {
                return format!("panel「{}」的字段缺少 label", panel.id);
            }
            if !matches!(field.kind.as_str(), "text" | "number") {
                return format!("panel「{}」的字段类型不支持：{}", panel.id, field.kind);
            }
        }
    }
    String::new()
}

/// Panel URLs are templates like `https://host/?year={year}`. Replace known
/// placeholders with a benign value before parsing so braces don't hide a
/// malformed URL, and reject anything that is not HTTPS.
fn valid_panel_url(template: &str) -> bool {
    let trimmed = template.trim();
    if !trimmed.starts_with("https://") {
        return false;
    }
    let mut parsed = String::with_capacity(trimmed.len());
    let mut rest = trimmed;
    while let Some(start) = rest.find('{') {
        parsed.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            return false;
        };
        let key = &rest[start + 1..start + end];
        if !valid_key(key) {
            return false;
        }
        parsed.push('x');
        rest = &rest[start + end + 1..];
    }
    parsed.push_str(rest);
    url::Url::parse(&parsed).is_ok()
}

fn validate_settings(schema: &[PluginSetting], values: &Value) -> Result<(), String> {
    let Some(object) = values.as_object() else {
        return Err("settings 必须是对象".into());
    };
    for (key, value) in object {
        let Some(setting) = schema.iter().find(|item| &item.key == key) else {
            return Err(format!("未知设置项：{key}"));
        };
        if !setting_type_matches(&setting.kind, value) {
            return Err(format!("设置「{key}」类型不匹配"));
        }
        if setting.kind == "select"
            && !setting.options.is_empty()
            && value.as_str().map(|text| setting.options.iter().any(|option| option == text)) != Some(true)
        {
            return Err(format!("设置「{key}」不在可选值里"));
        }
    }
    Ok(())
}

fn setting_type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "boolean" => value.is_boolean(),
        "text" | "select" => value.is_string(),
        "number" => value.is_number(),
        _ => false,
    }
}

fn normalize_settings(schema: &[PluginSetting], saved: Value) -> Value {
    let mut out = serde_json::Map::new();
    let saved = saved.as_object().cloned().unwrap_or_default();
    for setting in schema {
        if let Some(value) = saved.get(&setting.key) {
            if setting_type_matches(&setting.kind, value) {
                out.insert(setting.key.clone(), value.clone());
                continue;
            }
        }
        out.insert(setting.key.clone(), setting.default.clone());
    }
    Value::Object(out)
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.len() < 2 || id.len() > 64 {
        return Err("插件 id 长度必须是 2–64".into());
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return Err("插件 id 不能为空".into());
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("插件 id 必须以小写字母或数字开头".into());
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')) {
        return Err("插件 id 只能包含小写字母、数字、. _ -".into());
    }
    Ok(())
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn valid_version(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn version_cmp(left: &str, right: &str) -> i32 {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let a = parse(left);
    let b = parse(right);
    for index in 0..3 {
        let x = a.get(index).copied().unwrap_or(0);
        let y = b.get(index).copied().unwrap_or(0);
        if x != y {
            return if x > y { 1 } else { -1 };
        }
    }
    0
}

fn load_state(root: &Path) -> StateFile {
    fs::read_to_string(root.join("state.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_state(root: &Path, state: &StateFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
    let temp = root.join("state.json.tmp");
    fs::write(&temp, text).map_err(|error| error.to_string())?;
    let target = root.join("state.json");
    if target.exists() {
        fs::remove_file(&target).map_err(|error| error.to_string())?;
    }
    fs::rename(&temp, target).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("yebian-plugins-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn plugin(root: &Path, id: &str, permissions: &str) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("plugin.json"),
            format!(
                r#"{{
  "id": "{id}",
  "name": "测试插件",
  "version": "1.0.0",
  "apiVersion": 1,
  "main": "main.js",
  "permissions": [{permissions}],
  "commands": [{{ "id": "run", "title": "运行", "context": "note" }}]
}}"#
            ),
        )
        .unwrap();
        fs::write(dir.join("main.js"), "margin.commands.run = async () => ({ message: 'ok' })").unwrap();
    }

    #[test]
    fn scan_requires_permission_before_enable() {
        let root = temp_root();
        plugin(&root, "test.plugin", "\"notes:read\"");
        let manager = PluginManager::open(root.clone());
        let list = manager.scan();
        assert_eq!(list.len(), 1);
        assert!(list[0].error.is_empty());
        assert!(manager.set_enabled("test.plugin", true, vec![]).is_err());
        let enabled = manager.set_enabled("test.plugin", true, vec!["notes:read".into()]).unwrap();
        assert!(enabled.enabled);
        assert!(manager.source("test.plugin").unwrap().contains("margin.commands"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_path_traversal_and_unknown_permissions() {
        let root = temp_root();
        plugin(&root, "test.plugin", "\"notes:read\"");
        let manager = PluginManager::open(root.clone());
        assert!(manager.set_enabled("test.plugin", true, vec!["net:all".into()]).is_err());

        let dir = root.join("bad.plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("plugin.json"),
            r#"{"id":"bad.plugin","name":"坏插件","version":"1.0.0","apiVersion":1,"main":"../x.js"}"#,
        )
        .unwrap();
        let list = manager.scan();
        let bad = list.iter().find(|item| item.id == "bad.plugin").unwrap();
        assert!(bad.error.contains("单个 .js"), "{}", bad.error);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn settings_are_normalized_and_validated() {
        let root = temp_root();
        let dir = root.join("test.settings");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("plugin.json"),
            r#"{
  "id":"test.settings","name":"设置","version":"1.0.0","apiVersion":1,"main":"main.js",
  "settings":[{"key":"includeCode","label":"包含代码","type":"boolean","default":true}],
  "commands":[{"id":"run","title":"运行"}]
}"#,
        )
        .unwrap();
        fs::write(dir.join("main.js"), "margin.commands.run = () => ({})").unwrap();
        let manager = PluginManager::open(root.clone());
        let info = manager.scan().pop().unwrap();
        assert_eq!(info.settings_values["includeCode"], json!(true));
        assert!(manager.save_settings("test.settings", json!({"includeCode":"yes"})).is_err());
        assert!(manager.save_settings("test.settings", json!({"unknown":true})).is_err());
        let saved = manager.save_settings("test.settings", json!({"includeCode":false})).unwrap();
        assert_eq!(saved.settings_values["includeCode"], json!(false));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn panel_manifests_require_https_and_api_v2() {
        let root = temp_root();
        let dir = root.join("test.panel");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("plugin.json"),
            r#"{
  "id":"test.panel","name":"面板","version":"1.0.0","apiVersion":2,"main":"main.js",
  "panels":[{"id":"view","title":"预览","url":"https://example.com/?year={year}","fields":[{"key":"year","label":"年份","type":"number"}]}],
  "commands":[{"id":"open","title":"打开"}]
}"#,
        )
        .unwrap();
        fs::write(dir.join("main.js"), "margin.commands.open = () => ({ panel: { id: 'view', params: {} } })").unwrap();
        let manager = PluginManager::open(root.clone());
        let list = manager.scan();
        assert!(list[0].error.is_empty(), "{}", list[0].error);
        assert_eq!(list[0].panels.len(), 1);
        assert_eq!(list[0].panels[0].fields[0].kind, "number");

        fs::write(
            dir.join("plugin.json"),
            r#"{
  "id":"test.panel","name":"面板","version":"1.0.0","apiVersion":2,"main":"main.js",
  "panels":[{"id":"view","title":"预览","url":"http://example.com/?year={year}"}]
}"#,
        )
        .unwrap();
        let list = manager.scan();
        assert!(list[0].error.contains("https"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn examples_install_once_without_overwriting() {
        let root = temp_root();
        let manager = PluginManager::open(root.clone());
        let first = manager.install_examples().unwrap();
        assert!(first.iter().any(|item| item.id == "example.word-count"));
        fs::write(root.join("example.word-count/main.js"), "custom").unwrap();
        manager.install_examples().unwrap();
        assert_eq!(fs::read_to_string(root.join("example.word-count/main.js")).unwrap(), "custom");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bundled_plugin_upgrade_replaces_files_and_keeps_state() {
        let root = temp_root();
        let manager = PluginManager::open(root.clone());
        manager.install_examples().unwrap();
        let manifest_path = root.join("example.word-count/plugin.json");
        let mut manifest: Value = serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["version"] = json!("0.9.0");
        fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();
        fs::write(root.join("example.word-count/main.js"), "old").unwrap();
        manager
            .set_enabled("example.word-count", true, vec!["notes:read".into()])
            .unwrap();

        let list = manager.install_examples().unwrap();
        let word = list.iter().find(|item| item.id == "example.word-count").unwrap();
        assert_eq!(word.version, "1.0.0");
        assert!(word.enabled);
        assert!(fs::read_to_string(root.join("example.word-count/main.js")).unwrap().contains("margin.commands"));
        let _ = fs::remove_dir_all(root);
    }
}
