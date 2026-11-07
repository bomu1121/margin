//! Unit-level checkpoints for resumable production runs.
//!
//! A checkpoint is only written after an external call has returned. The
//! pipeline can then be re-entered from the top: a matching unit returns the
//! saved value instead of calling the model again. See docs/RESUME_DESIGN.md.

use std::future::Future;
use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::clients::{self, AiConfig, ChatResult};
use super::store::ProductionStore;
use super::{PriceQuote, Usage};

/// Bump when prompt assembly, chunking or parsing changes in a way that is not
/// represented by the prompt text itself. It is part of every input hash.
pub const PIPELINE_VERSION: u32 = 1;

#[derive(Clone)]
pub struct CostModel {
    pub price: Option<PriceQuote>,
    pub asr_price_per_minute: f64,
}

impl CostModel {
    pub fn apply(&self, usage: &mut Usage) {
        let mut cost = 0.0;
        if let Some(price) = &self.price {
            let cache_read = if price.cache_read_priced { price.cache_read_cny } else { price.input_cny };
            let cache_write = if price.cache_write_priced { price.cache_write_cny } else { price.input_cny };
            cost += usage.uncached_input_tokens as f64 / 1_000_000.0 * price.input_cny;
            cost += usage.cached_tokens as f64 / 1_000_000.0 * cache_read;
            cost += usage.cache_write_tokens as f64 / 1_000_000.0 * cache_write;
            cost += usage.completion_tokens as f64 / 1_000_000.0 * price.output_cny;
        }
        cost += usage.audio_seconds / 60.0 * self.asr_price_per_minute;
        usage.estimated_cost = cost;
    }
}

pub fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(sha256_bytes(&bytes))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn file_sha256(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("读取音频失败（{}）：{error}", path.display()))?;
    Ok(sha256_bytes(&bytes))
}

pub fn usage_from_chat(result: &ChatResult) -> Usage {
    Usage {
        prompt_tokens: result.prompt_tokens,
        uncached_input_tokens: result.uncached_input_tokens,
        completion_tokens: result.completion_tokens,
        cached_tokens: result.cached_tokens,
        cache_write_tokens: result.cache_write_tokens,
        reasoning_tokens: result.reasoning_tokens,
        provider_cost: result.provider_cost.unwrap_or(0.0),
        provider_currency: result.provider_currency.clone().unwrap_or_default(),
        ..Usage::default()
    }
}

pub fn add_usage(total: &mut Usage, delta: &Usage) {
    total.prompt_tokens += delta.prompt_tokens;
    total.uncached_input_tokens += delta.uncached_input_tokens;
    total.completion_tokens += delta.completion_tokens;
    total.cached_tokens += delta.cached_tokens;
    total.cache_write_tokens += delta.cache_write_tokens;
    total.reasoning_tokens += delta.reasoning_tokens;
    total.audio_seconds += delta.audio_seconds;
    total.provider_cost += delta.provider_cost;
    if !delta.provider_currency.is_empty() {
        total.provider_currency = delta.provider_currency.clone();
    }
}

/// Return a cached value when its input hash matches; otherwise run the
/// closure, persist the result and usage, and refresh the job's known usage.
pub async fn memo<T, F, Fut>(
    store: &ProductionStore,
    job_id: &str,
    step_key: &str,
    unit_key: &str,
    kind: &str,
    input_hash: &str,
    usage: &mut Usage,
    cost: &CostModel,
    compute: F,
) -> Result<T, String>
where
    T: Serialize + DeserializeOwned + Send,
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = Result<(T, Usage), String>> + Send,
{
    if let Some(row) = store.checkpoint(job_id, unit_key)? {
        if row.input_hash == input_hash {
            let value = serde_json::from_str::<T>(&row.payload_json)
                .map_err(|error| format!("检查点 {unit_key} 无法解析：{error}"))?;
            let delta = serde_json::from_str::<Usage>(&row.usage_json).unwrap_or_default();
            add_usage(usage, &delta);
            cost.apply(usage);
            store.set_usage(job_id, usage)?;
            return Ok(value);
        }
    }

    let (value, delta) = compute().await?;
    add_usage(usage, &delta);
    cost.apply(usage);
    let payload = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    let usage_json = serde_json::to_string(&delta).map_err(|error| error.to_string())?;
    store.save_checkpoint(
        job_id,
        unit_key,
        step_key,
        kind,
        input_hash,
        &payload,
        &usage_json,
        payload.len() as i64,
    )?;
    store.set_usage(job_id, usage)?;
    Ok(value)
}

pub async fn cached_chat(
    store: &ProductionStore,
    job_id: &str,
    step_key: &str,
    unit_key: &str,
    system: &str,
    user: &str,
    ai: &AiConfig,
    usage: &mut Usage,
    cost: &CostModel,
    token: &CancellationToken,
) -> Result<ChatResult, String> {
    let input_hash = hash_json(&json!({
        "pipelineVersion": PIPELINE_VERSION,
        "system": system,
        "user": user,
        "provider": ai.provider,
        "protocol": ai.protocol,
        "baseUrl": ai.base_url,
        "model": ai.model,
    }))?;
    memo(store, job_id, step_key, unit_key, "chat", &input_hash, usage, cost, || async {
        let result = clients::chat(ai, system, user, token).await?;
        let delta = usage_from_chat(&result);
        Ok((result, delta))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn memo_reuses_results_and_accumulates_usage() {
        let dir = std::env::temp_dir().join(format!("yebian-checkpoint-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ProductionStore::open(&dir.join("yebian.sqlite")).unwrap();
        let cost = CostModel { price: None, asr_price_per_minute: 0.0 };
        let calls = Arc::new(AtomicUsize::new(0));
        let mut usage = Usage::default();

        let first = memo(
            &store,
            "run-test",
            "refine",
            "refine:item:0:part:0",
            "text",
            "hash-a",
            &mut usage,
            &cost,
            || {
                let calls = calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(("first".to_string(), Usage { prompt_tokens: 10, completion_tokens: 5, ..Usage::default() }))
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(first, "first");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);

        let second = memo(
            &store,
            "run-test",
            "refine",
            "refine:item:0:part:0",
            "text",
            "hash-a",
            &mut usage,
            &cost,
            || {
                let calls = calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(("should-not-run".to_string(), Usage::default()))
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(second, "first");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(usage.prompt_tokens, 20);

        let third = memo(
            &store,
            "run-test",
            "refine",
            "refine:item:0:part:0",
            "text",
            "hash-b",
            &mut usage,
            &cost,
            || {
                let calls = calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(("changed".to_string(), Usage::default()))
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(third, "changed");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(store.list_checkpoints("run-test").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn manual_cost_model_matches_known_value() {
        let quote = PriceQuote {
            input_cny: 10.0,
            output_cny: 30.0,
            ..PriceQuote::default()
        };
        let cost = CostModel { price: Some(quote), asr_price_per_minute: 0.0 };
        let mut usage = Usage {
            uncached_input_tokens: 1_000_000,
            completion_tokens: 1_000_000,
            ..Usage::default()
        };
        cost.apply(&mut usage);
        assert!((usage.estimated_cost - 40.0).abs() < 1e-9);
    }
}
