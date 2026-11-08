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

/// Time one model gets to answer before the next one is tried.
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

const USER_AGENT: &str = concat!("replysieve/", env!("CARGO_PKG_VERSION"));

fn decisions_url(base_url: &str) -> String {
    format!("{}/api/alpha/decisions", base_url.trim_end_matches('/'))
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
    url: String,
    key: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl Client {
    pub fn new(base_url: &str, key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("HTTP client builds with static settings");
        Client {
            http,
            url: decisions_url(base_url),
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

/// The same client on WebAssembly, through the runtime's own `fetch`
/// (Cloudflare Workers, Node 20 and later). A model that has not answered
/// within the timeout is aborted with `AbortSignal.timeout`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub(crate) struct Client {
    url: String,
    key: String,
    pub timeout: Duration,
}

#[cfg(target_arch = "wasm32")]
impl Client {
    pub fn new(base_url: &str, key: String) -> Self {
        Client {
            url: decisions_url(base_url),
            key,
            timeout: DEFAULT_TIMEOUT,
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
        let (status, text) = fetch::post(&self.url, &self.key, &body.to_string(), self.timeout)
            .await
            .map_err(|timed_out| {
                if timed_out {
                    "timed out".to_string()
                } else {
                    "request failed".to_string()
                }
            })?;
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}"));
        }
        serde_json::from_str(&text).map_err(|_| "unreadable response".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod fetch {
    use js_sys::{Function, Object, Promise, Reflect};
    use std::time::Duration;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    fn get(target: &JsValue, key: &str) -> Result<JsValue, bool> {
        Reflect::get(target, &JsValue::from_str(key)).map_err(|_| false)
    }

    fn set(target: &Object, key: &str, value: &JsValue) -> Result<(), bool> {
        Reflect::set(target, &JsValue::from_str(key), value)
            .map(|_| ())
            .map_err(|_| false)
    }

    fn function(target: &JsValue, key: &str) -> Result<Function, bool> {
        get(target, key)?.dyn_into::<Function>().map_err(|_| false)
    }

    async fn awaited(value: JsValue) -> Result<JsValue, bool> {
        let promise = value.dyn_into::<Promise>().map_err(|_| false)?;
        JsFuture::from(promise).await.map_err(|e| {
            let name = get(&e, "name").ok().and_then(|n| n.as_string());
            matches!(name.as_deref(), Some("TimeoutError" | "AbortError"))
        })
    }

    /// POST `body` as JSON. Returns the status and body text, or `Err(true)`
    /// when the request timed out and `Err(false)` for any other failure.
    pub async fn post(
        url: &str,
        key: &str,
        body: &str,
        timeout: Duration,
    ) -> Result<(u16, String), bool> {
        let global: JsValue = js_sys::global().into();
        let fetch = function(&global, "fetch")?;
        let abort_signal = get(&global, "AbortSignal")?;
        let signal = function(&abort_signal, "timeout")?
            .call1(
                &abort_signal,
                &JsValue::from_f64(timeout.as_millis() as f64),
            )
            .map_err(|_| false)?;

        let headers = Object::new();
        set(&headers, "content-type", &"application/json".into())?;
        set(&headers, "authorization", &format!("Bearer {key}").into())?;
        set(&headers, "user-agent", &super::USER_AGENT.into())?;
        let init = Object::new();
        set(&init, "method", &"POST".into())?;
        set(&init, "headers", &headers)?;
        set(&init, "body", &body.into())?;
        set(&init, "signal", &signal)?;

        let resp = awaited(
            fetch
                .call2(&global, &url.into(), &init)
                .map_err(|_| false)?,
        )
        .await?;
        let status = get(&resp, "status")?.as_f64().ok_or(false)? as u16;
        let text = awaited(function(&resp, "text")?.call0(&resp).map_err(|_| false)?)
            .await?
            .as_string()
            .ok_or(false)?;
        Ok((status, text))
    }
}
