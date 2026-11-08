//! OpenRouter decisions endpoint client.

use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

/// Default OpenRouter base URL.
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai";

/// One typed answer from a decision model.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Answer {
    Choice {
        #[allow(dead_code)]
        choice: String,
        probabilities: HashMap<String, f64>,
    },
    Noul {
        noul: f64,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Response {
    pub model: String,
    pub answers: HashMap<String, Answer>,
}

#[derive(Debug, Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
    url: String,
    key: String,
}

impl Client {
    pub fn new(base_url: &str, key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("replysieve/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client builds with static settings");
        Client {
            http,
            url: format!("{}/api/alpha/decisions", base_url.trim_end_matches('/')),
            key,
        }
    }

    /// Ask one model. The error string never contains comment text.
    pub async fn decide(
        &self,
        model: &str,
        state: &str,
        questions: &serde_json::Value,
    ) -> Result<Response, String> {
        let body = serde_json::json!({ "model": model, "state": state, "questions": questions });
        let resp = self
            .http
            .post(&self.url)
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    "timed out".to_string()
                } else {
                    "request failed".to_string()
                }
            })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {}", status.as_u16()));
        }
        resp.json::<Response>()
            .await
            .map_err(|_| "unreadable response".to_string())
    }
}
