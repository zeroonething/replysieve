//! HTTP server: POST /v1/check, POST /v1/check/batch and GET /healthz.
//!
//! Request bodies are the MCP tool arguments, and responses are the same
//! verdict JSON the CLI prints. Logs carry method, path, status and timing,
//! never comment text.

use crate::mcp::{CheckBatch, CheckComment, Verdicts, at};
use anyhow::{Context, bail};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use replysieve::{API_KEY_ENV, Checker};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

/// Environment variable holding the bearer token callers must send.
pub const TOKEN_ENV: &str = "REPLYSIEVE_TOKEN";

/// Largest request body accepted, in bytes.
const MAX_BODY: usize = 1024 * 1024;

/// Most comments accepted in one batch.
const MAX_BATCH: usize = 100;

struct App {
    checker: Checker,
    token: Option<String>,
}

type Shared = State<Arc<App>>;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

/// A body that could not be read or parsed: 400, 413, 415 or 422.
fn rejected(e: JsonRejection) -> Response {
    error(e.status(), e.body_text())
}

async fn healthz() -> Response {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })).into_response()
}

async fn check(State(app): Shared, body: Result<Json<CheckComment>, JsonRejection>) -> Response {
    let p = match body {
        Ok(Json(p)) => p,
        Err(e) => return rejected(e),
    };
    match at(&app.checker, p.level)
        .check(&p.comment(), &p.recent)
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn check_batch(
    State(app): Shared,
    body: Result<Json<CheckBatch>, JsonRejection>,
) -> Response {
    let p = match body {
        Ok(Json(p)) => p,
        Err(e) => return rejected(e),
    };
    if p.comments.len() > MAX_BATCH {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "a batch holds at most {MAX_BATCH} comments, got {}",
                p.comments.len()
            ),
        );
    }
    match at(&app.checker, p.level)
        .check_batch(&p.comments, &p.recent)
        .await
    {
        Ok(verdicts) => Json(Verdicts { verdicts }).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// Byte comparison whose timing does not depend on where the first mismatch is.
/// The length still shows; a token's length is not the secret.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn auth(State(app): Shared, req: Request, next: Next) -> Response {
    if let Some(token) = &app.token {
        let sent = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if !sent.is_some_and(|s| same(s.as_bytes(), token.as_bytes())) {
            let mut resp = error(StatusCode::UNAUTHORIZED, "missing or wrong bearer token");
            resp.headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            return resp;
        }
    }
    next.run(req).await
}

/// One line per request for --verbose: method, path, status, time.
async fn log_request(req: Request, next: Next) -> Response {
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    let started = Instant::now();
    let resp = next.run(req).await;
    log::info!(
        "{method} {path} {} in {} ms",
        resp.status().as_u16(),
        started.elapsed().as_millis()
    );
    resp
}

fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/v1/check", post(check))
        .route("/v1/check/batch", post(check_batch))
        .route_layer(middleware::from_fn_with_state(app.clone(), auth));
    Router::new()
        .route("/healthz", get(healthz))
        .merge(api)
        .fallback(|| async { error(StatusCode::NOT_FOUND, "no such endpoint") })
        .method_not_allowed_fallback(|| async {
            error(StatusCode::METHOD_NOT_ALLOWED, "method not allowed")
        })
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn(log_request))
        .with_state(app)
}

/// Resolves on Ctrl-C or SIGTERM, so `docker stop` drains in-flight requests.
/// The handlers are installed when this is called, not when the future is
/// first polled: called before the server listens, a signal that arrives right
/// after startup still shuts it down cleanly instead of killing the process.
#[cfg(unix)]
fn shutdown() -> anyhow::Result<impl Future<Output = ()>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).context("cannot handle SIGTERM")?;
    let mut int = signal(SignalKind::interrupt()).context("cannot handle SIGINT")?;
    Ok(async move {
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
        log::info!("shutting down");
    })
}

#[cfg(not(unix))]
fn shutdown() -> anyhow::Result<impl Future<Output = ()>> {
    Ok(async {
        tokio::signal::ctrl_c().await.ok();
        log::info!("shutting down");
    })
}

/// Serve HTTP on `bind` until Ctrl-C or SIGTERM.
pub async fn serve(
    checker: Checker,
    bind: SocketAddr,
    insecure_no_auth: bool,
) -> anyhow::Result<()> {
    let has_key = std::env::var(API_KEY_ENV).is_ok_and(|k| !k.trim().is_empty());
    if !has_key {
        bail!("{}", replysieve::Error::MissingApiKey);
    }
    let token = std::env::var(TOKEN_ENV)
        .ok()
        .filter(|t| !t.trim().is_empty());
    if token.is_none() && !bind.ip().is_loopback() && !insecure_no_auth {
        bail!(
            "refusing to listen on {bind} without {TOKEN_ENV}: anyone who can reach it could spend your \
             OpenRouter credits. Set {TOKEN_ENV} to a long random string, or pass --insecure-no-auth \
             if something in front of replysieve already authenticates callers"
        );
    }
    let shutdown = shutdown()?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("cannot listen on {bind}"))?;
    eprintln!("replysieve: listening on http://{}", listener.local_addr()?);
    if token.is_none() {
        eprintln!("replysieve: {TOKEN_ENV} is not set, so requests are not authenticated");
    }
    let app = Arc::new(App { checker, token });
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
