//! Mnemos HTTP client. Talks to the daemon's REST surface.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod error;
pub mod transport;

pub use error::{ClientError, Result};

use mnemos_core::retrieval::RecallHit;
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::Tier;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

#[derive(Clone)]
pub struct Client {
    tx: std::sync::Arc<transport::Transport>,
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        Ok(Self {
            tx: std::sync::Arc::new(transport::Transport::new(base_url, token)?),
        })
    }

    /// `GET /health` — returns true on 200.
    pub async fn health(&self) -> Result<bool> {
        let v: Value = self
            .tx
            .request(Method::GET, "/health", None::<&()>, false)
            .await?;
        Ok(v.get("status").and_then(|s| s.as_str()) == Some("ok"))
    }

    pub async fn remember(&self, body: &str, opts: RememberClientOpts) -> Result<String> {
        let req = RememberReq {
            body: body.to_string(),
            title: opts.title,
            tier: opts.tier.unwrap_or_else(|| "semantic".into()),
            kind: opts.kind.unwrap_or_else(|| "fact".into()),
            tags: opts.tags,
            importance: opts.importance,
            workspace: opts.workspace,
            source_tool: opts.source_tool,
        };
        let v: Value = self
            .tx
            .request(Method::POST, "/v1/memories", Some(&req), true)
            .await?;
        Ok(v["id"].as_str().unwrap_or_default().to_string())
    }

    pub async fn get_memory(&self, id: &str) -> Result<Memory> {
        // The daemon's GET /v1/memories/{id} returns a superset of Memory that
        // includes `body` (which Memory's own serde impl skips).  We deserialize
        // into our own wire type first, then convert.
        let r: GetMemoryResp = self
            .tx
            .request(
                Method::GET,
                &format!("/v1/memories/{id}"),
                None::<&()>,
                true,
            )
            .await?;
        Ok(r.into())
    }

    pub async fn forget(&self, id: &str, reason: Option<&str>) -> Result<()> {
        let q = reason
            .map(|r| format!("?reason={}", urlencoding::encode(r)))
            .unwrap_or_default();
        let _: Value = self
            .tx
            .request(
                Method::DELETE,
                &format!("/v1/memories/{id}{q}"),
                None::<&()>,
                true,
            )
            .await?;
        Ok(())
    }

    pub async fn list_memories(&self, opts: ListClientOpts) -> Result<Vec<Memory>> {
        let q = opts.to_query();
        let v: Value = self
            .tx
            .request(Method::GET, &format!("/v1/memories{q}"), None::<&()>, true)
            .await?;
        Ok(serde_json::from_value(v["memories"].clone())?)
    }

    pub async fn recall(&self, query: &str, opts: RecallClientOpts) -> Result<Vec<RecallHit>> {
        let req = RecallReq {
            query: query.to_string(),
            k: opts.k.unwrap_or(10),
            tier: opts.tier,
            workspace: opts.workspace,
            include_invalid: opts.include_invalid,
            explain: opts.explain,
            rerank: opts.rerank,
        };
        let v: Value = self
            .tx
            .request(Method::POST, "/v1/memories/search", Some(&req), true)
            .await?;
        Ok(serde_json::from_value(v["hits"].clone())?)
    }
}

#[derive(Default, Debug, Clone)]
pub struct RememberClientOpts {
    pub title: Option<String>,
    pub tier: Option<String>,
    pub kind: Option<String>,
    pub tags: Vec<String>,
    pub importance: Option<f64>,
    pub workspace: Option<String>,
    pub source_tool: Option<String>,
}

#[derive(Default, Debug, Clone)]
pub struct RecallClientOpts {
    pub k: Option<usize>,
    pub tier: Option<Vec<String>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
    pub explain: bool,
    pub rerank: bool,
}

#[derive(Default, Debug, Clone)]
pub struct ListClientOpts {
    pub tier: Option<Vec<String>>,
    pub workspace: Option<String>,
    pub include_invalid: bool,
    pub limit: Option<usize>,
}

impl ListClientOpts {
    fn to_query(&self) -> String {
        let mut parts: Vec<String> = vec![];
        if let Some(ts) = &self.tier {
            for t in ts {
                parts.push(format!("tier={}", urlencoding::encode(t)));
            }
        }
        if let Some(ws) = &self.workspace {
            parts.push(format!("workspace={}", urlencoding::encode(ws)));
        }
        if self.include_invalid {
            parts.push("include_invalid=true".into());
        }
        if let Some(l) = self.limit {
            parts.push(format!("limit={l}"));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("?{}", parts.join("&"))
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RememberReq {
    body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    tier: String,
    kind: String,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    importance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_tool: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct RecallReq {
    query: String,
    k: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    tier: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<String>,
    include_invalid: bool,
    explain: bool,
    rerank: bool,
}

/// Wire type for `GET /v1/memories/{id}` — mirrors `GetMemoryResp` in the
/// daemon, which explicitly includes `body` (unlike `Memory`'s serde impl).
#[derive(Deserialize)]
struct GetMemoryResp {
    id: String,
    #[serde(rename = "type")]
    kind: MemoryType,
    tier: String,
    title: String,
    body: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    entities: Vec<String>,
    #[serde(default)]
    links: Vec<String>,
    strength: f64,
    importance: f64,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    source_tool: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    ingested_at: chrono::DateTime<chrono::Utc>,
    valid_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    invalid_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    superseded_by: Option<String>,
    last_accessed: chrono::DateTime<chrono::Utc>,
    access_count: u64,
    #[serde(default = "default_mnemos_version")]
    mnemos_version: u32,
}

fn default_mnemos_version() -> u32 {
    1
}

impl From<GetMemoryResp> for Memory {
    fn from(r: GetMemoryResp) -> Self {
        let tier = Tier::from_str(&r.tier).unwrap_or_default();
        Memory {
            id: r.id,
            tier,
            kind: r.kind,
            title: r.title,
            body: r.body,
            tags: r.tags,
            entities: r.entities,
            links: r.links,
            provenance: vec![],
            created_at: r.created_at,
            ingested_at: r.ingested_at,
            valid_at: r.valid_at,
            invalid_at: r.invalid_at,
            superseded_by: r.superseded_by,
            strength: r.strength,
            importance: r.importance,
            last_accessed: r.last_accessed,
            access_count: r.access_count,
            workspace: r.workspace,
            source_tool: r.source_tool,
            mnemos_version: r.mnemos_version,
        }
    }
}
