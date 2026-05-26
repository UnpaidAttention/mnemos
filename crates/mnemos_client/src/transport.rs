use reqwest::{Client as Http, Method};
use serde::{de::DeserializeOwned, Serialize};
use url::Url;

use crate::error::{ClientError, Result};

pub struct Transport {
    http: Http,
    base: Url,
    token: String,
}

impl Transport {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let base = Url::parse(base_url)?;
        let http = Http::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            base,
            token: token.into(),
        })
    }

    pub async fn request<B: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        with_auth: bool,
    ) -> Result<R> {
        let url = self.base.join(path)?;
        let mut req = self.http.request(method, url);
        if with_auth {
            req = req.bearer_auth(&self.token);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(ClientError::Server {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).to_string(),
            });
        }
        if bytes.is_empty() {
            // Allow `()` deserialization when there's no body.
            return Ok(serde_json::from_str::<R>("null")?);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}
