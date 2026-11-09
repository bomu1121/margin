//! Wire types and the transport used by the sync engine.
//!
//! The server treats `payload` as opaque bytes: today it is JSON, later it can
//! be ciphertext without touching the server.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const OP_UPSERT: &str = "upsert";
pub const OP_DELETE: &str = "delete";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Op {
    pub op_id: String,
    pub entity: String,
    pub pk: String,
    pub kind: String,
    pub hlc: String,
    pub device_id: String,
    #[serde(default = "json_codec")]
    pub codec: String,
    #[serde(default)]
    pub payload: Value,
}

fn json_codec() -> String {
    "json".to_string()
}

impl Op {
    pub fn is_delete(&self) -> bool {
        self.kind == OP_DELETE
    }
}

#[derive(Clone, Debug)]
pub struct RemoteOp {
    #[allow(dead_code)]
    pub seq: i64,
    pub op: Op,
}

#[derive(Clone, Debug)]
pub struct PullBatch {
    pub ops: Vec<RemoteOp>,
    pub cursor: i64,
    pub has_more: bool,
}

pub trait Transport {
    fn push_ops(&self, vault_id: &str, ops: &[Op]) -> Result<(), String>;
    fn pull_ops(&self, vault_id: &str, cursor: i64, limit: usize) -> Result<PullBatch, String>;
    fn put_blob(&self, vault_id: &str, hash: &str, bytes: &[u8]) -> Result<(), String>;
    fn get_blob(&self, vault_id: &str, hash: &str) -> Result<Vec<u8>, String>;
}

/// HTTP client for `sync-server`. Blocking on purpose: the caller runs it on a
/// background thread, never on the UI thread.
pub struct HttpTransport {
    client: reqwest::blocking::Client,
    base: String,
    token: String,
}

impl HttpTransport {
    pub fn new(base: &str, token: &str) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { client, base: base.trim_end_matches('/').to_string(), token: token.to_string() })
    }

    fn url(&self, vault: &str, tail: &str) -> String {
        format!("{}/v1/vaults/{}/{}", self.base, urlencode(vault), tail)
    }

    fn auth(&self, request: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        request.bearer_auth(&self.token)
    }
}

fn urlencode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

impl Transport for HttpTransport {
    fn push_ops(&self, vault_id: &str, ops: &[Op]) -> Result<(), String> {
        if ops.is_empty() {
            return Ok(());
        }
        let response = self
            .auth(self.client.post(self.url(vault_id, "ops")))
            .json(&serde_json::json!({ "ops": ops }))
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("push failed: HTTP {}", response.status()));
        }
        Ok(())
    }

    fn pull_ops(&self, vault_id: &str, cursor: i64, limit: usize) -> Result<PullBatch, String> {
        let response = self
            .auth(self.client.get(self.url(vault_id, "ops")))
            .query(&[("cursor", cursor.to_string()), ("limit", limit.to_string())])
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("pull failed: HTTP {}", response.status()));
        }
        let body: serde_json::Value = response.json().map_err(|error| error.to_string())?;
        let ops = body
            .get("ops")
            .and_then(Value::as_array)
            .ok_or_else(|| "pull response has no ops".to_string())?
            .iter()
            .map(|entry| {
                let seq = entry.get("seq").and_then(Value::as_i64).unwrap_or(0);
                let op: Op = serde_json::from_value(entry.get("op").cloned().unwrap_or(Value::Null))
                    .map_err(|error| format!("bad remote op: {error}"))?;
                Ok(RemoteOp { seq, op })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let next = body.get("cursor").and_then(Value::as_i64).unwrap_or(cursor);
        let has_more = body.get("has_more").and_then(Value::as_bool).unwrap_or(false);
        Ok(PullBatch { ops, cursor: next, has_more })
    }

    fn put_blob(&self, vault_id: &str, hash: &str, bytes: &[u8]) -> Result<(), String> {
        let response = self
            .auth(self.client.put(self.url(vault_id, &format!("blobs/{hash}"))))
            .header("content-type", "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("blob upload failed: HTTP {}", response.status()));
        }
        Ok(())
    }

    fn get_blob(&self, vault_id: &str, hash: &str) -> Result<Vec<u8>, String> {
        let response = self
            .auth(self.client.get(self.url(vault_id, &format!("blobs/{hash}"))))
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("blob download failed: HTTP {}", response.status()));
        }
        response.bytes().map(|bytes| bytes.to_vec()).map_err(|error| error.to_string())
    }
}

/// In-process implementation used by the convergence tests. It mirrors the
/// server contract: monotonic sequence, op-id dedupe, content-addressed blobs.
#[cfg(test)]
pub struct MemoryTransport {
    state: std::sync::Mutex<MemoryState>,
}

#[cfg(test)]
#[derive(Default)]
struct MemoryState {
    next_seq: i64,
    ops: Vec<RemoteOp>,
    seen: std::collections::HashSet<String>,
    blobs: std::collections::HashMap<String, Vec<u8>>,
}

#[cfg(test)]
impl MemoryTransport {
    pub fn new() -> Self {
        Self { state: std::sync::Mutex::new(MemoryState { next_seq: 1, ..Default::default() }) }
    }
}

#[cfg(test)]
impl Transport for MemoryTransport {
    fn push_ops(&self, _vault_id: &str, ops: &[Op]) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        for op in ops {
            if state.seen.insert(op.op_id.clone()) {
                let seq = state.next_seq;
                state.ops.push(RemoteOp { seq, op: op.clone() });
                state.next_seq += 1;
            }
        }
        Ok(())
    }

    fn pull_ops(&self, _vault_id: &str, cursor: i64, limit: usize) -> Result<PullBatch, String> {
        let state = self.state.lock().map_err(|error| error.to_string())?;
        let mut selected: Vec<RemoteOp> =
            state.ops.iter().filter(|entry| entry.seq > cursor).cloned().collect();
        selected.sort_by_key(|entry| entry.seq);
        let has_more = selected.len() > limit;
        selected.truncate(limit);
        let next = selected.last().map(|entry| entry.seq).unwrap_or(cursor);
        Ok(PullBatch { ops: selected, cursor: next, has_more })
    }

    fn put_blob(&self, _vault_id: &str, hash: &str, bytes: &[u8]) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        state.blobs.insert(hash.to_string(), bytes.to_vec());
        Ok(())
    }

    fn get_blob(&self, _vault_id: &str, hash: &str) -> Result<Vec<u8>, String> {
        let state = self.state.lock().map_err(|error| error.to_string())?;
        state.blobs.get(hash).cloned().ok_or_else(|| format!("blob {hash} not found"))
    }
}
