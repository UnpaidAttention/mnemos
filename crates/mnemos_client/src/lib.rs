//! Mnemos HTTP client. Talks to the daemon's REST surface.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod error;
pub mod transport;

pub use error::{ClientError, Result};

use mnemos_core::retrieval::RecallHit;
use mnemos_core::types::Memory;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        v["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ClientError::Server {
                status: 200,
                body: "daemon response missing 'id' field".to_string(),
            })
    }

    pub async fn get_memory(&self, id: &str) -> Result<Memory> {
        self.tx
            .request(
                Method::GET,
                &format!("/v1/memories/{id}"),
                None::<&()>,
                true,
            )
            .await
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

#[cfg(test)]
mod tests {
    use super::*;

    /// P2-9: a daemon response with a missing "id" field must surface as
    /// ClientError::Server, not silently return an empty string.
    #[test]
    fn remember_missing_id_is_an_error() {
        // Simulate the JSON body that `remember()` parses from the daemon.
        let v: Value = serde_json::json!({ "other": "stuff" });
        let result: Result<String> =
            v["id"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| ClientError::Server {
                    status: 200,
                    body: "daemon response missing 'id' field".to_string(),
                });
        assert!(result.is_err(), "missing id must yield an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("missing 'id'"),
            "error message should mention missing id, got: {msg}"
        );
    }

    /// P2-9: when the daemon returns a proper id, remember() must return it.
    #[test]
    fn remember_present_id_is_returned() {
        let v: Value = serde_json::json!({ "id": "mem_abc123", "body": "x" });
        let result: Result<String> =
            v["id"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| ClientError::Server {
                    status: 200,
                    body: "daemon response missing 'id' field".to_string(),
                });
        assert_eq!(result.unwrap(), "mem_abc123");
    }

    /// P2-9: null id (JSON null) is also treated as an error.
    #[test]
    fn remember_null_id_is_an_error() {
        let v: Value = serde_json::json!({ "id": null });
        let result: Result<String> =
            v["id"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| ClientError::Server {
                    status: 200,
                    body: "daemon response missing 'id' field".to_string(),
                });
        assert!(result.is_err(), "null id must yield an error");
    }
}
