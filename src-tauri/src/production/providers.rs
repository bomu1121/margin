use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const CATALOG_URL: &str = "https://models.dev/api.json";
const CATALOG_TTL_MS: i64 = 24 * 60 * 60 * 1000;
const CATALOG_MIN_MODELS: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub default_model: String,
    pub key_required: bool,
    pub local: bool,
    pub supports_models: bool,
    pub supports_balance: bool,
    pub docs_url: String,
    pub catalog_id: String,
    pub note: String,
}

fn preset(
    id: &str,
    name: &str,
    protocol: &str,
    base_url: &str,
    default_model: &str,
    key_required: bool,
    local: bool,
    supports_balance: bool,
    docs_url: &str,
    catalog_id: &str,
    note: &str,
) -> ProviderPreset {
    ProviderPreset {
        id: id.into(),
        name: name.into(),
        protocol: protocol.into(),
        base_url: base_url.into(),
        default_model: default_model.into(),
        key_required,
        local,
        supports_models: true,
        supports_balance,
        docs_url: docs_url.into(),
        catalog_id: catalog_id.into(),
        note: note.into(),
    }
}

pub fn presets() -> Vec<ProviderPreset> {
    vec![
        preset("deepseek", "DeepSeek", "openai", "https://api.deepseek.com/v1", "deepseek-v4-pro", true, false, true, "https://api-docs.deepseek.com/quick_start/pricing", "deepseek", "官方 API；价格有峰谷两档，估算按基础档"),
        preset("openai", "OpenAI", "openai", "https://api.openai.com/v1", "gpt-5", true, false, false, "https://platform.openai.com/docs/models", "openai", "官方 API；余额/账单需要组织 Admin Key"),
        preset("anthropic", "Anthropic", "anthropic", "https://api.anthropic.com/v1", "claude-sonnet-5", true, false, false, "https://docs.anthropic.com/en/docs/about-claude/models", "anthropic", "原生 Messages API；余额/账单需要 Admin Key"),
        preset("gemini", "Google Gemini", "gemini", "https://generativelanguage.googleapis.com/v1beta", "gemini-pro-latest", true, false, false, "https://ai.google.dev/gemini-api/docs/models", "google", "原生 generateContent API"),
        preset("openrouter", "OpenRouter", "openai", "https://openrouter.ai/api/v1", "openai/gpt-5", true, false, true, "https://openrouter.ai/docs/guides/overview/models", "openrouter", "聚合网关；响应里的 cost 是实际扣费"),
        preset("moonshot", "Moonshot / Kimi", "openai", "https://api.moonshot.cn/v1", "kimi-k3", true, false, true, "https://platform.moonshot.cn/docs", "moonshotai-cn", "官方 API；可查余额"),
        preset("zhipu", "智谱 BigModel", "openai", "https://open.bigmodel.cn/api/paas/v4", "glm-4.6", true, false, false, "https://docs.bigmodel.cn/", "zhipuai", "官方没有公开余额接口"),
        preset("siliconflow", "SiliconFlow", "openai", "https://api.siliconflow.cn/v1", "deepseek-ai/DeepSeek-V3", true, false, false, "https://docs.siliconflow.cn/", "siliconflow", "OpenAI 兼容聚合"),
        preset("dashscope", "阿里百炼", "openai", "https://dashscope.aliyuncs.com/compatible-mode/v1", "qwen-plus", true, false, false, "https://help.aliyun.com/zh/model-studio/", "alibaba", "OpenAI 兼容模式"),
        preset("volcengine", "火山方舟", "openai", "https://ark.cn-beijing.volces.com/api/v3", "doubao-seed-1-8-251228", true, false, false, "https://www.volcengine.com/docs/82379", "volcengine", "OpenAI 兼容模式"),
        preset("ollama", "Ollama", "openai", "http://127.0.0.1:11434/v1", "", false, true, false, "https://ollama.com/", "", "本地模型，无 key；只记 token，不产生金额"),
        preset("lmstudio", "LM Studio", "openai", "http://127.0.0.1:1234/v1", "", false, true, false, "https://lmstudio.ai/", "lmstudio", "本地模型，无 key；只记 token，不产生金额"),
        preset("custom", "自定义 OpenAI 兼容", "openai", "", "", true, false, false, "", "", "完全自定义；没价目时只记 token"),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PriceQuote {
    pub input_usd: f64,
    pub output_usd: f64,
    pub cache_read_usd: f64,
    pub cache_write_usd: f64,
    pub input_cny: f64,
    pub output_cny: f64,
    pub cache_read_cny: f64,
    pub cache_write_cny: f64,
    #[serde(default)]
    pub cache_read_priced: bool,
    #[serde(default)]
    pub cache_write_priced: bool,
    pub exchange_rate: f64,
    pub source: String,
    pub updated_at: i64,
    pub matched_key: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub context_length: i64,
    pub max_output: i64,
    pub price: Option<PriceQuote>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelList {
    pub models: Vec<ModelOption>,
    pub source: String,
    pub warning: String,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceEntry {
    pub label: String,
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResult {
    pub provider: String,
    pub provider_label: String,
    pub entries: Vec<BalanceEntry>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CatalogModel {
    id: String,
    name: String,
    #[serde(default)]
    context: i64,
    #[serde(default)]
    max_output: i64,
    #[serde(default)]
    input_usd: f64,
    #[serde(default)]
    output_usd: f64,
    #[serde(default)]
    cache_read_usd: f64,
    #[serde(default)]
    cache_write_usd: f64,
    #[serde(default)]
    cache_read_priced: bool,
    #[serde(default)]
    cache_write_priced: bool,
    #[serde(default)]
    canonical: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CatalogProvider {
    name: String,
    models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CachedCatalog {
    source: String,
    fetched_at: i64,
    providers: HashMap<String, CatalogProvider>,
}

pub struct PriceCatalog {
    cache: CachedCatalog,
}

impl PriceCatalog {
    pub fn empty() -> Self {
        Self {
            cache: CachedCatalog {
                source: String::new(),
                fetched_at: 0,
                providers: HashMap::new(),
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cache.providers.is_empty()
    }

    pub async fn load(cache_dir: &Path, force: bool) -> Result<Self, String> {
        let cache_path = cache_dir.join("models-dev-catalog.json");
        let cached = read_cache(&cache_path);
        let now = now_ms();
        if !force {
            if let Some(cached) = &cached {
                if now - cached.fetched_at < CATALOG_TTL_MS {
                    return Ok(Self { cache: cached.clone() });
                }
            }
        }
        match fetch_catalog().await {
            Ok(cache) => {
                let _ = write_cache(&cache_path, &cache);
                Ok(Self { cache })
            }
            Err(error) => {
                if let Some(cached) = cached {
                    return Ok(Self { cache: cached });
                }
                Err(format!("模型价目获取失败：{error}"))
            }
        }
    }

    pub fn source(&self) -> &str {
        &self.cache.source
    }

    pub fn fetched_at(&self) -> i64 {
        self.cache.fetched_at
    }

    pub fn models(&self, catalog_id: &str, exchange_rate: f64) -> Vec<ModelOption> {
        let Some(provider) = self.cache.providers.get(catalog_id) else { return Vec::new() };
        provider
            .models
            .iter()
            .map(|model| ModelOption {
                id: model.id.clone(),
                name: model.name.clone(),
                context_length: model.context,
                max_output: model.max_output,
                source: "models.dev".into(),
                price: price_from_catalog(model, exchange_rate, self.cache.fetched_at),
            })
            .collect()
    }

    pub fn price(&self, catalog_id: &str, model_id: &str, exchange_rate: f64) -> Option<PriceQuote> {
        let provider = self.cache.providers.get(catalog_id)?;
        let normalized = normalize_model(model_id);
        let without_date = strip_date_suffix(&normalized);
        provider.models.iter().find_map(|model| {
            let id = normalize_model(&model.id);
            if id == normalized
                || id == without_date
                || strip_date_suffix(&id) == without_date
                || (!model.canonical.is_empty() && normalize_model(&model.canonical).ends_with(&normalized))
            {
                price_from_catalog(model, exchange_rate, self.cache.fetched_at)
            } else {
                None
            }
        })
    }
}

fn price_from_catalog(model: &CatalogModel, exchange_rate: f64, fetched_at: i64) -> Option<PriceQuote> {
    if model.input_usd <= 0.0 && model.output_usd <= 0.0 {
        return None;
    }
    let rate = if exchange_rate > 0.0 { exchange_rate } else { 7.2 };
    Some(PriceQuote {
        input_usd: round(model.input_usd),
        output_usd: round(model.output_usd),
        cache_read_usd: round(model.cache_read_usd),
        cache_write_usd: round(model.cache_write_usd),
        input_cny: round(model.input_usd * rate),
        output_cny: round(model.output_usd * rate),
        cache_read_cny: round(model.cache_read_usd * rate),
        cache_write_cny: round(model.cache_write_usd * rate),
        cache_read_priced: model.cache_read_priced,
        cache_write_priced: model.cache_write_priced,
        exchange_rate: rate,
        source: "models.dev".into(),
        updated_at: fetched_at,
        matched_key: model.id.clone(),
        note: if model.input_usd > 0.0 && model.output_usd <= 0.0 {
            "只找到输入价，输出价按 0 展示".into()
        } else {
            String::new()
        },
    })
}

fn round(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn normalize_model(value: &str) -> String {
    value.trim().to_lowercase().replace(['.', '@'], "-")
}

fn strip_date_suffix(value: &str) -> String {
    let bytes = value.as_bytes();
    let digits = |range: std::ops::Range<usize>| range.clone().all(|index| bytes[index].is_ascii_digit());
    let len = bytes.len();
    // -YYYY-MM-DD
    if len > 11 {
        let start = len - 11;
        if bytes[start] == b'-'
            && bytes[start + 5] == b'-'
            && bytes[start + 8] == b'-'
            && digits(start + 1..start + 5)
            && digits(start + 6..start + 8)
            && digits(start + 9..start + 11)
        {
            return value[..start].to_string();
        }
    }
    // -YYYYMMDD / YYYYMMDD
    for (marker, width) in [("-", 9), ("", 8)] {
        if len <= width {
            continue;
        }
        let start = len - width;
        if (marker.is_empty() || bytes[start] == b'-') && digits(start + marker.len()..len) {
            return value[..start].to_string();
        }
    }
    value.to_string()
}

fn read_cache(path: &Path) -> Option<CachedCatalog> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_cache(path: &Path, cache: &CachedCatalog) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string(cache).map_err(|error| error.to_string())?;
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, text).map_err(|error| error.to_string())?;
    std::fs::rename(temp, path).map_err(|error| error.to_string())
}

async fn fetch_catalog() -> Result<CachedCatalog, String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?
        .get(CATALOG_URL)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let raw: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let mut providers = HashMap::new();
    let Some(object) = raw.as_object() else { return Err("目录不是对象".into()) };
    for (provider_id, provider_value) in object {
        let Some(models) = provider_value.get("models").and_then(Value::as_object) else { continue };
        let name = provider_value.get("name").and_then(Value::as_str).unwrap_or(provider_id).to_string();
        let mut items = Vec::new();
        for (model_id, model_value) in models {
            let cost = model_value.get("cost");
            let input_usd = number(cost.and_then(|value| value.get("input")));
            let output_usd = number(cost.and_then(|value| value.get("output")));
            let cache_read = cost.and_then(|value| value.get("cache_read"));
            let cache_write = cost.and_then(|value| value.get("cache_write"));
            let cache_read_usd = number(cache_read);
            let cache_write_usd = number(cache_write);
            let limit = model_value.get("limit");
            items.push(CatalogModel {
                id: model_value.get("id").and_then(Value::as_str).unwrap_or(model_id).to_string(),
                name: model_value.get("name").and_then(Value::as_str).unwrap_or(model_id).to_string(),
                context: number(limit.and_then(|value| value.get("context"))) as i64,
                max_output: number(limit.and_then(|value| value.get("output"))) as i64,
                input_usd,
                output_usd,
                cache_read_usd,
                cache_write_usd,
                cache_read_priced: cache_read.is_some(),
                cache_write_priced: cache_write.is_some(),
                canonical: model_value.get("canonical_model_id").and_then(Value::as_str).unwrap_or_default().to_string(),
            });
        }
        if items.len() >= 1 {
            providers.insert(provider_id.clone(), CatalogProvider { name, models: items });
        }
    }
    let total: usize = providers.values().map(|provider| provider.models.len()).sum();
    if total < CATALOG_MIN_MODELS {
        return Err(format!("目录只返回 {total} 个模型，疑似不完整"));
    }
    Ok(CachedCatalog {
        source: CATALOG_URL.into(),
        fetched_at: now_ms(),
        providers,
    })
}

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => text.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}


fn endpoint(base_url: &str, suffix: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), suffix.trim_start_matches('/'))
}

pub async fn fetch_models(
    preset: &ProviderPreset,
    base_url: &str,
    key: &str,
    exchange_rate: f64,
    catalog: &PriceCatalog,
    _force: bool,
) -> Result<ModelList, String> {
    let base = if base_url.trim().is_empty() { &preset.base_url } else { base_url };
    let catalog_models = catalog.models(&preset.catalog_id, exchange_rate);
    let can_try_live = !base.trim().is_empty() && (!preset.key_required || !key.trim().is_empty());
    if !can_try_live {
        return Ok(ModelList {
            models: catalog_models,
            source: if preset.local { "catalog".into() } else { "catalog".into() },
            warning: if preset.key_required && key.trim().is_empty() {
                "还没有填 Key，先显示目录里的模型；填 Key 后可以拉取账号可用列表".into()
            } else {
                "本地服务未启动或没有模型，先显示目录".into()
            },
            fetched_at: catalog.fetched_at(),
        });
    }

    let live = match preset.protocol.as_str() {
        "anthropic" => fetch_anthropic_models(base, key).await,
        "gemini" => fetch_gemini_models(base, key).await,
        _ => fetch_openai_models(base, key, preset.id == "openrouter", exchange_rate).await,
    };
    match live {
        Ok(items) => {
            let mut models: Vec<ModelOption> = items
                .into_iter()
                .map(|(id, name, context, max_output, live_price)| {
                    let price = if preset.id == "openrouter" {
                        live_price.or_else(|| catalog.price(&preset.catalog_id, &id, exchange_rate))
                    } else {
                        catalog.price(&preset.catalog_id, &id, exchange_rate)
                    };
                    ModelOption {
                        id,
                        name,
                        context_length: context,
                        max_output,
                        price,
                        source: "provider".into(),
                    }
                })
                .collect();
            models.sort_by(|a, b| a.id.cmp(&b.id));
            if models.is_empty() {
                models = catalog_models;
            }
            Ok(ModelList {
                models,
                source: "provider".into(),
                warning: String::new(),
                fetched_at: now_ms(),
            })
        }
        Err(error) => {
            let has_catalog = !catalog_models.is_empty();
            Ok(ModelList {
                models: catalog_models,
                source: if has_catalog { "catalog".into() } else { String::new() },
                warning: if preset.local {
                    format!("本地服务未启动或没有可用模型：{error}")
                } else if has_catalog {
                    format!("拉取账号模型列表失败，改用目录：{error}")
                } else {
                    format!("拉取模型列表失败：{error}")
                },
                fetched_at: catalog.fetched_at(),
            })
        }
    }
}

async fn fetch_openai_models(
    base: &str,
    key: &str,
    openrouter: bool,
    exchange_rate: f64,
) -> Result<Vec<(String, String, i64, i64, Option<PriceQuote>)>, String> {
    let request = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?
        .get(endpoint(base, "models"))
        .header("User-Agent", "yebian-production/0.1");
    let request = if key.is_empty() { request } else { request.header("Authorization", format!("Bearer {key}")) };
    let response = request.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}：{}", text.chars().take(300).collect::<String>()));
    }
    let raw: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let data = raw.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut items = Vec::new();
    for item in data {
        let Some(id) = item.get("id").and_then(Value::as_str) else { continue };
        let name = item.get("name").and_then(Value::as_str).unwrap_or(id).to_string();
        let context = item.get("context_length").and_then(Value::as_i64).unwrap_or(0);
        let max_output = item
            .get("top_provider")
            .and_then(|value| value.get("max_completion_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let price = if openrouter {
            let pricing = item.get("pricing");
            let input = number(pricing.and_then(|value| value.get("prompt")));
            let output = number(pricing.and_then(|value| value.get("completion")));
            let cache_read = pricing.and_then(|value| value.get("input_cache_read"));
            let cache_write = pricing.and_then(|value| value.get("input_cache_write"));
            let cache_read_usd = number(cache_read);
            let cache_write_usd = number(cache_write);
            if input > 0.0 || output > 0.0 {
                let rate = if exchange_rate > 0.0 { exchange_rate } else { 7.2 };
                Some(PriceQuote {
                    input_usd: round(input * 1_000_000.0),
                    output_usd: round(output * 1_000_000.0),
                    cache_read_usd: round(cache_read_usd * 1_000_000.0),
                    cache_write_usd: round(cache_write_usd * 1_000_000.0),
                    input_cny: round(input * 1_000_000.0 * rate),
                    output_cny: round(output * 1_000_000.0 * rate),
                    cache_read_cny: round(cache_read_usd * 1_000_000.0 * rate),
                    cache_write_cny: round(cache_write_usd * 1_000_000.0 * rate),
                    cache_read_priced: cache_read.is_some(),
                    cache_write_priced: cache_write.is_some(),
                    exchange_rate: rate,
                    source: "OpenRouter".into(),
                    updated_at: now_ms(),
                    matched_key: id.to_string(),
                    note: "OpenRouter 转售报价".into(),
                })
            } else {
                None
            }
        } else {
            None
        };
        items.push((id.to_string(), name, context, max_output, price));
    }
    Ok(items)
}

async fn fetch_anthropic_models(base: &str, key: &str) -> Result<Vec<(String, String, i64, i64, Option<PriceQuote>)>, String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?
        .get(endpoint(base, "models?limit=1000"))
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .header("User-Agent", "yebian-production/0.1")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}：{}", text.chars().take(300).collect::<String>()));
    }
    let raw: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let data = raw.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(data
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_str)?;
            Some((
                id.to_string(),
                item.get("display_name").and_then(Value::as_str).unwrap_or(id).to_string(),
                item.get("max_input_tokens").and_then(Value::as_i64).unwrap_or(0),
                item.get("max_tokens").and_then(Value::as_i64).unwrap_or(0),
                None,
            ))
        })
        .collect())
}

async fn fetch_gemini_models(base: &str, key: &str) -> Result<Vec<(String, String, i64, i64, Option<PriceQuote>)>, String> {
    let url = format!("{}/models?key={}&pageSize=1000", base.trim_end_matches('/'), urlencoding(key));
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?
        .get(url)
        .header("User-Agent", "yebian-production/0.1")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}：{}", text.chars().take(300).collect::<String>()));
    }
    let raw: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let data = raw.get("models").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(data
        .iter()
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?.trim_start_matches("models/").to_string();
            Some((
                name.clone(),
                item.get("displayName").and_then(Value::as_str).unwrap_or(&name).to_string(),
                item.get("inputTokenLimit").and_then(Value::as_i64).unwrap_or(0),
                item.get("outputTokenLimit").and_then(Value::as_i64).unwrap_or(0),
                None,
            ))
        })
        .collect())
}

fn urlencoding(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(*byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

pub async fn fetch_balance(preset: &ProviderPreset, base_url: &str, key: &str) -> Result<BalanceResult, String> {
    if !preset.supports_balance {
        return Err(format!("{} 没有公开余额接口；余额只能到厂商控制台查看", preset.name));
    }
    if key.trim().is_empty() {
        return Err("还没有配置 API Key".into());
    }
    let base = if base_url.trim().is_empty() { &preset.base_url } else { base_url };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let get = |url: String| {
        client
            .get(url)
            .header("Authorization", format!("Bearer {key}"))
            .header("User-Agent", "yebian-production/0.1")
    };
    let (url, provider_label) = match preset.id.as_str() {
        "deepseek" => (format!("{}/user/balance", base.trim_end_matches('/').trim_end_matches("/v1")), "DeepSeek"),
        "moonshot" => (format!("{}/users/me/balance", base.trim_end_matches('/')), "Moonshot / Kimi"),
        "openrouter" => {
            let root = base.trim_end_matches('/').trim_end_matches("/api/v1");
            (format!("{root}/api/v1/credits"), "OpenRouter")
        }
        _ => return Err(format!("{} 没有接入余额查询", preset.name)),
    };
    let response = get(url).send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("余额接口 {status}：{}", text.chars().take(300).collect::<String>()));
    }
    let raw: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    if preset.id == "deepseek" {
        for info in raw.get("balance_infos").and_then(Value::as_array).cloned().unwrap_or_default() {
            entries.push(BalanceEntry {
                label: "账户余额".into(),
                amount: info.get("total_balance").map(value_string).unwrap_or_default(),
                currency: info.get("currency").map(value_string).unwrap_or_default(),
            });
        }
    } else if preset.id == "moonshot" {
        if let Some(data) = raw.get("data") {
            for (key, label) in [("available_balance", "可用余额"), ("voucher_balance", "代金券"), ("cash_balance", "现金余额")] {
                entries.push(BalanceEntry {
                    label: label.into(),
                    amount: data.get(key).map(value_string).unwrap_or_default(),
                    currency: "CNY".into(),
                });
            }
        }
    } else {
        if let Some(data) = raw.get("data") {
            let total = number(data.get("total_credits"));
            let used = number(data.get("total_usage"));
            entries.push(BalanceEntry { label: "已购买".into(), amount: format!("{total:.2}"), currency: "USD".into() });
            entries.push(BalanceEntry { label: "已使用".into(), amount: format!("{used:.2}"), currency: "USD".into() });
            entries.push(BalanceEntry { label: "剩余".into(), amount: format!("{:.2}", total - used), currency: "USD".into() });
        }
    }
    entries.retain(|entry| !entry.amount.is_empty());
    if entries.is_empty() {
        return Err("余额接口没有返回可用字段".into());
    }
    Ok(BalanceResult {
        provider: preset.id.clone(),
        provider_label: provider_label.into(),
        entries,
        fetched_at: now_ms(),
    })
}

fn value_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

pub fn preset_by_id(id: &str) -> Option<ProviderPreset> {
    presets().into_iter().find(|item| item.id == id)
}

pub fn catalog_id_for(protocol: &str, id: &str) -> String {
    preset_by_id(id).map(|item| item.catalog_id).unwrap_or_else(|| if protocol == "gemini" { "google".into() } else { id.into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_cover_all_supported_providers() {
        let ids = presets().into_iter().map(|item| item.id).collect::<Vec<_>>();
        for expected in ["deepseek", "openai", "anthropic", "gemini", "openrouter", "moonshot", "zhipu", "siliconflow", "dashscope", "volcengine", "ollama", "lmstudio", "custom"] {
            assert!(ids.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[tokio::test]
    #[ignore = "requires network access to models.dev"]
    async fn live_catalog_covers_presets_and_prices() {
        let dir = std::env::temp_dir().join(format!("yebian-catalog-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let catalog = PriceCatalog::load(&dir, true).await.unwrap();
        let mut total = 0usize;
        for preset in presets() {
            if preset.catalog_id.is_empty() {
                continue;
            }
            let models = catalog.models(&preset.catalog_id, 7.2);
            assert!(!models.is_empty(), "{} has no catalog models", preset.id);
            total += models.len();
        }
        let quote = catalog.price("deepseek", "deepseek-v4-pro", 7.2).unwrap();
        assert!(quote.input_cny > 0.0 && quote.output_cny > 0.0);
        eprintln!(
            "catalog presets={} models={} deepseek input=¥{:.3}/M output=¥{:.3}/M",
            presets().iter().filter(|preset| !preset.catalog_id.is_empty()).count(),
            total,
            quote.input_cny,
            quote.output_cny
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn strips_model_date_suffix() {
        assert_eq!(strip_date_suffix("gpt-4o-2024-08-06"), "gpt-4o");
        assert_eq!(strip_date_suffix("claude-20250929"), "claude");
        assert_eq!(strip_date_suffix("deepseek-v4-pro"), "deepseek-v4-pro");
    }

    #[test]
    fn matches_exact_dated_and_canonical_models_only() {
        let mut models = std::collections::HashMap::new();
        models.insert(
            "openai".to_string(),
            CatalogProvider {
                name: "OpenAI".into(),
                models: vec![CatalogModel {
                    id: "gpt-4o-2024-08-06".into(),
                    name: "GPT-4o".into(),
                    context: 128000,
                    max_output: 16384,
                    input_usd: 2.5,
                    output_usd: 10.0,
                    cache_read_usd: 1.25,
                    cache_write_usd: 0.0,
                    cache_read_priced: true,
                    cache_write_priced: true,
                    canonical: "openai/gpt-4o".into(),
                }],
            },
        );
        let catalog = PriceCatalog {
            cache: CachedCatalog { source: "test".into(), fetched_at: 1, providers: models },
        };
        assert_eq!(catalog.price("openai", "gpt-4o", 7.2).unwrap().matched_key, "gpt-4o-2024-08-06");
        assert_eq!(catalog.price("openai", "gpt-4o-2024-08-06", 7.2).unwrap().input_cny, 18.0);
        assert!(catalog.price("openai", "gpt-4o-mini", 7.2).is_none());
        assert!(catalog.price("unknown", "gpt-4o", 7.2).is_none());
    }

    #[test]
    fn zero_or_missing_cost_is_not_reported_as_free() {
        let model = CatalogModel {
            id: "mystery".into(),
            input_usd: 0.0,
            output_usd: 0.0,
            ..CatalogModel::default()
        };
        assert!(price_from_catalog(&model, 7.2, 1).is_none());
    }

    #[test]
    fn quote_converts_usd_to_cny() {
        let model = CatalogModel {
            input_usd: 0.15,
            output_usd: 0.6,
            cache_read_usd: 0.015,
            ..CatalogModel::default()
        };
        let quote = price_from_catalog(&model, 7.2, 1).unwrap();
        assert!((quote.input_cny - 1.08).abs() < 0.0001);
        assert!((quote.output_cny - 4.32).abs() < 0.0001);
        assert!((quote.cache_read_cny - 0.108).abs() < 0.0001);
    }
}
