//! Minimal MCP client used by the Rust note engine.
//!
//! The app is an MCP host: each configured external server is connected as a
//! child process (stdio) or Streamable HTTP endpoint. Tools are namespaced as
//! `mcp__<server>__<tool>` so a server cannot silently shadow a local tool.
//! Servers are opt-in via `mcp.json`; no file means the engine behaves exactly
//! as before.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use rmcp::service::RunningService;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::production::clients::ToolSpec;

const CONFIG_FILE: &str = "mcp.json";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_RESULT_CHARS: usize = 20_000;
const MAX_TOOLS_PER_SERVER: usize = 20;
const MAX_TOOL_NAME: usize = 64;
const MAX_CONFIG_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub tools: McpToolFilter,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolFilter {
    #[serde(default)]
    pub allow: Vec<String>,
}

/// What the settings panel needs to display. The raw text is returned so the
/// page can edit it without a second round trip; secrets never leave the disk
/// unless the user opens this panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigView {
    pub path: String,
    pub exists: bool,
    pub text: String,
    pub server_count: usize,
    pub enabled_count: usize,
    pub allowed_tool_count: usize,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTestResult {
    pub id: String,
    pub transport: String,
    pub ok: bool,
    pub tools: Vec<String>,
    pub error: String,
    pub elapsed_ms: u128,
}

fn example_config() -> String {
    r#"{
  "servers": [
    {
      "id": "playwright",
      "transport": "stdio",
      "command": "npx",
      "args": ["-y", "@playwright/mcp@latest"],
      "enabled": false,
      "timeoutMs": 30000,
      "tools": {
        "allow": ["browser_navigate", "browser_snapshot"]
      }
    }
  ]
}"#
    .into()
}

fn default_transport() -> String {
    "stdio".into()
}

fn default_enabled() -> bool {
    true
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

struct ServerConnection {
    id: String,
    timeout: Duration,
    client: Arc<RunningService<RoleClient, ()>>,
}

#[derive(Clone)]
struct McpToolBinding {
    server: String,
    tool: String,
    model_name: String,
    description: String,
    parameters: Value,
}

pub struct McpSession {
    connections: Vec<ServerConnection>,
    tools: Vec<McpToolBinding>,
}

impl McpSession {
    /// Connect every enabled server. Missing/invalid config is deliberately
    /// non-fatal: the note engine must keep working without MCP.
    pub async fn connect_from_app(app: &tauri::AppHandle) -> Option<Arc<Self>> {
        use tauri::Manager;
        let dir = app.path().app_data_dir().ok()?;
        Self::connect_from_dir(&dir).await
    }

    pub async fn connect_from_dir(dir: &Path) -> Option<Arc<Self>> {
        let path = dir.join(CONFIG_FILE);
        if !path.is_file() {
            return None;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                log::warn!("读取 {} 失败：{error}", path.display());
                return None;
            }
        };
        let config: McpConfig = match serde_json::from_str(&text) {
            Ok(config) => config,
            Err(error) => {
                log::warn!("解析 {} 失败：{error}", path.display());
                return None;
            }
        };
        Self::connect_config(config).await
    }

    pub async fn connect_config(config: McpConfig) -> Option<Arc<Self>> {
        let mut connections = Vec::new();
        let mut tools = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        let mut seen_tools = std::collections::HashSet::new();
        for server in config.servers.into_iter().filter(|server| server.enabled) {
            if !seen_ids.insert(server.id.clone()) {
                log::warn!("MCP server「{}」id 重复，已忽略后一个", server.id);
                continue;
            }
            if let Err(error) = validate_server(&server) {
                log::warn!("MCP server「{}」配置无效：{error}", server.id);
                continue;
            }
            if server.tools.allow.is_empty() {
                log::warn!(
                    "MCP server「{}」没有配置 tools.allow，已跳过（放行全部请写 \"*\"）",
                    server.id
                );
                continue;
            }
            match connect_server(&server).await {
                Ok((client, discovered)) => {
                    let timeout = Duration::from_millis(server.timeout_ms.clamp(1_000, 300_000));
                    let connection_id = server.id.clone();
                    for tool in discovered {
                        if !tool_allowed(&server.tools, &tool.name) {
                            continue;
                        }
                        if tools
                            .iter()
                            .filter(|item: &&McpToolBinding| item.server == connection_id)
                            .count()
                            >= MAX_TOOLS_PER_SERVER
                        {
                            break;
                        }
                        if let Some(binding) = bind_tool(&server.id, &tool) {
                            if !seen_tools.insert(binding.model_name.clone()) {
                                log::warn!("MCP 工具名冲突，已忽略：{}", binding.model_name);
                                continue;
                            }
                            tools.push(binding);
                        }
                    }
                    connections.push(ServerConnection {
                        id: server.id,
                        timeout,
                        client: Arc::new(client),
                    });
                }
                Err(error) => log::warn!("MCP server「{}」连接失败：{error}", server.id),
            }
        }
        if connections.is_empty() {
            return None;
        }
        Some(Arc::new(Self { connections, tools }))
    }

    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .map(|tool| ToolSpec {
                name: tool.model_name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            })
            .collect()
    }

    /// Call a namespaced MCP tool and flatten its content into the same
    /// observation shape as the local tools. Never returns a protocol error as
    /// a panic: a broken server should only fail this one tool call.
    pub async fn call(&self, model_name: &str, args: Value) -> String {
        let Some(tool) = self.tools.iter().find(|tool| tool.model_name == model_name) else {
            return format!("未知 MCP 工具：{model_name}");
        };
        let Some(connection) = self.connections.iter().find(|item| item.id == tool.server) else {
            return format!("MCP server「{}」已断开", tool.server);
        };
        let arguments = match args {
            Value::Object(map) => map,
            Value::Null => serde_json::Map::new(),
            _ => return "MCP 工具参数必须是 JSON 对象".into(),
        };
        let params = CallToolRequestParams::new(tool.tool.clone()).with_arguments(arguments);
        match tokio::time::timeout(connection.timeout, connection.client.call_tool(params)).await {
            Ok(Ok(result)) => render_result(result),
            Ok(Err(error)) => format!("MCP 工具失败：{error}"),
            Err(_) => format!(
                "MCP 工具超时（{}s）：{}",
                connection.timeout.as_secs(),
                tool.tool
            ),
        }
    }
}

fn app_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
    Ok(dir.join(CONFIG_FILE))
}

/// Read the file without creating it. A missing file shows a disabled example
/// instead of writing anything behind the user's back.
pub fn config_view(app: &tauri::AppHandle) -> Result<McpConfigView, String> {
    let path = app_config_path(app)?;
    let exists = path.is_file();
    let text = if exists {
        std::fs::read_to_string(&path).map_err(|error| format!("读取配置失败：{error}"))?
    } else {
        example_config()
    };
    let (server_count, enabled_count, allowed_tool_count, error) =
        match serde_json::from_str::<McpConfig>(&text) {
            Ok(config) => (
                config.servers.len(),
                config
                    .servers
                    .iter()
                    .filter(|server| server.enabled)
                    .count(),
                config
                    .servers
                    .iter()
                    .filter(|server| server.enabled)
                    .map(|server| server.tools.allow.len())
                    .sum(),
                validate_config(&config).err().unwrap_or_default(),
            ),
            Err(parse_error) => (0, 0, 0, format!("JSON 解析失败：{parse_error}")),
        };
    Ok(McpConfigView {
        path: path.display().to_string(),
        exists,
        text,
        server_count,
        enabled_count,
        allowed_tool_count,
        error,
    })
}

pub fn save_config_text(app: &tauri::AppHandle, text: &str) -> Result<McpConfigView, String> {
    if text.len() > MAX_CONFIG_BYTES {
        return Err(format!("配置文件超过 {} KB 上限", MAX_CONFIG_BYTES / 1024));
    }
    let config: McpConfig =
        serde_json::from_str(text).map_err(|error| format!("JSON 解析失败：{error}"))?;
    validate_config(&config)?;
    let path = app_config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建目录失败：{error}"))?;
    }
    std::fs::write(&path, text).map_err(|error| format!("写入配置失败：{error}"))?;
    config_view(app)
}

/// Connect each enabled server once and report discovery results. The clients
/// are dropped immediately, which closes stdio children, so this is safe to
/// run before a task.
pub async fn test_config_text(text: &str) -> Vec<McpTestResult> {
    if text.len() > MAX_CONFIG_BYTES {
        return vec![McpTestResult {
            id: "config".into(),
            transport: String::new(),
            ok: false,
            tools: Vec::new(),
            error: format!("配置文件超过 {} KB 上限", MAX_CONFIG_BYTES / 1024),
            elapsed_ms: 0,
        }];
    }
    let config: McpConfig = match serde_json::from_str(text) {
        Ok(config) => config,
        Err(error) => {
            return vec![McpTestResult {
                id: "config".into(),
                transport: String::new(),
                ok: false,
                tools: Vec::new(),
                error: format!("JSON 解析失败：{error}"),
                elapsed_ms: 0,
            }];
        }
    };
    if let Err(error) = validate_config(&config) {
        return vec![McpTestResult {
            id: "config".into(),
            transport: String::new(),
            ok: false,
            tools: Vec::new(),
            error,
            elapsed_ms: 0,
        }];
    }
    let mut results = Vec::new();
    for server in config.servers.into_iter().filter(|server| server.enabled) {
        let started = Instant::now();
        let timeout = Duration::from_millis(server.timeout_ms.clamp(1_000, 300_000));
        let result = tokio::time::timeout(timeout, connect_server(&server)).await;
        let (ok, tools, error) = match result {
            Ok(Ok((_client, discovered))) => (
                true,
                discovered
                    .into_iter()
                    .map(|tool| tool.name.to_string())
                    .take(100)
                    .collect(),
                String::new(),
            ),
            Ok(Err(error)) => (false, Vec::new(), error),
            Err(_) => (
                false,
                Vec::new(),
                format!("连接超时（{} ms）", timeout.as_millis()),
            ),
        };
        results.push(McpTestResult {
            id: server.id,
            transport: server.transport,
            ok,
            tools,
            error,
            elapsed_ms: started.elapsed().as_millis(),
        });
    }
    results
}

fn validate_config(config: &McpConfig) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for (index, server) in config.servers.iter().enumerate() {
        let label = if server.id.is_empty() {
            format!("第 {} 个 server", index + 1)
        } else {
            format!("server「{}」", server.id)
        };
        validate_server(server).map_err(|error| format!("{label}：{error}"))?;
        if !ids.insert(server.id.as_str()) {
            return Err(format!("{label}：id 重复"));
        }
        if server.tools.allow.iter().any(|name| name.trim().is_empty()) {
            return Err(format!("{label}：tools.allow 里不能有空字符串"));
        }
    }
    Ok(())
}

async fn connect_server(
    server: &McpServerConfig,
) -> Result<(RunningService<RoleClient, ()>, Vec<Tool>), String> {
    let client = match server.transport.as_str() {
        "stdio" => {
            let transport = TokioChildProcess::new(
                tokio::process::Command::new(&server.command).configure(|command| {
                    command.args(&server.args);
                    for (key, value) in &server.env {
                        command.env(key, value);
                    }
                }),
            )
            .map_err(|error| format!("启动失败：{error}"))?;
            ().serve(transport)
                .await
                .map_err(|error| format!("初始化失败：{error}"))?
        }
        "http" => {
            let mut config = StreamableHttpClientTransportConfig::with_uri(server.url.clone());
            if !server.auth_token.is_empty() {
                config = config.auth_header(server.auth_token.clone());
            }
            let transport = StreamableHttpClientTransport::from_config(config);
            ().serve(transport)
                .await
                .map_err(|error| format!("初始化失败：{error}"))?
        }
        other => return Err(format!("不支持的 transport：{other}")),
    };
    let tools = client
        .list_all_tools()
        .await
        .map_err(|error| format!("读取工具失败：{error}"))?;
    Ok((client, tools))
}

fn validate_server(server: &McpServerConfig) -> Result<(), String> {
    if server.id.is_empty()
        || server.id.len() > 64
        || !server
            .id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err("id 不合法".into());
    }
    match server.transport.as_str() {
        "stdio" if server.command.trim().is_empty() => Err("stdio 缺少 command".into()),
        "http" if !server.url.starts_with("http://") && !server.url.starts_with("https://") => {
            Err("http server 的 url 必须以 http:// 或 https:// 开头".into())
        }
        "http" => Ok(()),
        "stdio" => Ok(()),
        other => Err(format!("不支持的 transport：{other}")),
    }
}

/// An empty allowlist exposes nothing. `"*"` is the explicit opt-in for all
/// tools; without it a typo in `allow` cannot silently open a server.
fn tool_allowed(filter: &McpToolFilter, name: &str) -> bool {
    filter.allow.iter().any(|item| item == "*" || item == name)
}

fn bind_tool(server: &str, tool: &Tool) -> Option<McpToolBinding> {
    let tool_name = tool.name.to_string();
    if tool_name.is_empty() {
        return None;
    }
    let model_name = namespaced_name(server, &tool_name);
    let description = tool
        .description
        .as_deref()
        .map(|text| text.chars().take(500).collect::<String>())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| format!("MCP tool {tool_name}"));
    let parameters = Value::Object((*tool.input_schema).clone());
    Some(McpToolBinding {
        server: server.into(),
        tool: tool_name,
        model_name,
        description: format!("[MCP: {server}] {description}"),
        parameters,
    })
}

fn namespaced_name(server: &str, tool: &str) -> String {
    let sanitize = |value: &str| {
        value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>()
    };
    let server = sanitize(server);
    let tool = sanitize(tool);
    let mut name = format!("mcp__{server}__{tool}");
    if name.len() <= MAX_TOOL_NAME {
        return name;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    name.truncate(MAX_TOOL_NAME - 9);
    format!("{name}_{:08x}", hasher.finish() as u32)
}

fn render_result(result: CallToolResult) -> String {
    let mut parts = Vec::new();
    for block in result.content {
        match block {
            ContentBlock::Text(text) => parts.push(text.text),
            ContentBlock::Image(image) => parts.push(format!(
                "[MCP 图片：{}, {} bytes]",
                image.mime_type,
                image.data.len()
            )),
            ContentBlock::Audio(audio) => parts.push(format!(
                "[MCP 音频：{}, {} bytes]",
                audio.mime_type,
                audio.data.len()
            )),
            ContentBlock::Resource(resource) => {
                let uri = match &resource.resource {
                    rmcp::model::ResourceContents::TextResourceContents { uri, .. }
                    | rmcp::model::ResourceContents::BlobResourceContents { uri, .. } => {
                        uri.clone()
                    }
                    _ => String::new(),
                };
                parts.push(format!("[MCP 资源：{uri}]"));
            }
            ContentBlock::ResourceLink(link) => parts.push(format!("[MCP 资源链接：{}]", link.uri)),
            _ => parts.push("[MCP 未支持的内容块]".into()),
        }
    }
    if parts.is_empty() {
        if let Some(structured) = result.structured_content {
            parts.push(serde_json::to_string_pretty(&structured).unwrap_or_default());
        }
    }
    let mut text = parts.join("\n\n");
    if result.is_error == Some(true) {
        text = format!("MCP 工具返回错误：{text}");
    }
    let clipped: String = text.chars().take(MAX_RESULT_CHARS).collect();
    if clipped.chars().count() < text.chars().count() {
        format!("{clipped}\n…（已截断，原 {} 字符）", text.chars().count())
    } else {
        clipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn namespaced_tool_names_are_stable_and_short() {
        let name = namespaced_name("playwright", "browser_navigate");
        assert_eq!(name, "mcp__playwright__browser_navigate");
        let long = namespaced_name("a".repeat(60).as_str(), &"tool".repeat(40));
        assert!(long.len() <= MAX_TOOL_NAME, "{long}");
        assert!(long.starts_with("mcp__"));
    }

    #[test]
    fn empty_allowlist_exposes_nothing_and_star_opens_all() {
        assert!(!tool_allowed(&McpToolFilter::default(), "anything"));
        let filter = McpToolFilter {
            allow: vec!["read_file".into()],
        };
        assert!(tool_allowed(&filter, "read_file"));
        assert!(!tool_allowed(&filter, "write_file"));
        let all = McpToolFilter {
            allow: vec!["*".into()],
        };
        assert!(tool_allowed(&all, "anything"));
    }

    #[test]
    fn config_validation_rejects_duplicate_ids_and_empty_allow_entries() {
        let server = McpServerConfig {
            id: "same".into(),
            transport: "stdio".into(),
            command: "node".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            url: String::new(),
            auth_token: String::new(),
            enabled: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            tools: McpToolFilter::default(),
        };
        let duplicate = McpConfig {
            servers: vec![server.clone(), server.clone()],
        };
        assert!(validate_config(&duplicate).is_err());
        let empty_allow = McpConfig {
            servers: vec![McpServerConfig {
                tools: McpToolFilter {
                    allow: vec!["".into()],
                },
                ..server
            }],
        };
        assert!(validate_config(&empty_allow).is_err());
    }

    #[test]
    fn config_rejects_unsafe_ids_and_missing_endpoints() {
        let mut server = McpServerConfig {
            id: "../bad".into(),
            transport: "stdio".into(),
            command: "node".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            url: String::new(),
            auth_token: String::new(),
            enabled: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            tools: McpToolFilter::default(),
        };
        assert!(validate_server(&server).is_err());
        server.id = "ok".into();
        server.command.clear();
        assert!(validate_server(&server).is_err());
        server.transport = "http".into();
        server.url = "http://127.0.0.1:1/mcp".into();
        assert!(validate_server(&server).is_ok());
    }

    #[test]
    fn json_schema_is_preserved_for_the_model() {
        let tool: Tool = serde_json::from_value(json!({
            "name": "echo",
            "description": "Echo text",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}
        }))
        .expect("tool");
        let binding = bind_tool("stub", &tool).expect("binding");
        assert_eq!(binding.model_name, "mcp__stub__echo");
        assert_eq!(binding.parameters["properties"]["text"]["type"], "string");
    }

    #[tokio::test]
    async fn connects_to_stdio_server_and_calls_tool() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let stub = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("mcp_stub.cjs");
        let config = McpConfig {
            servers: vec![McpServerConfig {
                id: "stub".into(),
                transport: "stdio".into(),
                command: "node".into(),
                args: vec![stub.to_string_lossy().to_string()],
                env: BTreeMap::new(),
                url: String::new(),
                auth_token: String::new(),
                enabled: true,
                timeout_ms: 5_000,
                tools: McpToolFilter {
                    allow: vec!["echo".into()],
                },
            }],
        };
        let session = McpSession::connect_config(config).await.expect("session");
        let tools = session.tool_specs();
        assert!(tools.iter().any(|tool| tool.name == "mcp__stub__echo"));
        let result = session.call("mcp__stub__echo", json!({"text": "hi"})).await;
        assert_eq!(result, "stub: hi");
    }

    #[test]
    fn result_flattens_text_and_truncates() {
        let result = CallToolResult::success(vec![ContentBlock::text("hello")]);
        assert_eq!(render_result(result), "hello");
        let long =
            CallToolResult::success(vec![ContentBlock::text("x".repeat(MAX_RESULT_CHARS + 10))]);
        let rendered = render_result(long);
        assert!(rendered.contains("已截断"));
        assert!(rendered.chars().count() < MAX_RESULT_CHARS + 40);
    }
}
