//! `replysieve serve`: the HTTP server against a mocked OpenRouter.

mod common;
use common::{BROKEN, mock, run, verdicts};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::time::{Duration, Instant};

const TITLE: &str = "Taskly: a todo app with offline sync";
const SPECIFIC: &str = "Does offline sync merge edits when I change one task on two phones?";
const GENERIC: &str = "This is a really nice app, well done to the whole team";
const TOKEN: &str = "test-token-123";

/// A running `replysieve serve`, killed on drop.
struct Server {
    child: Child,
    stderr: BufReader<ChildStderr>,
    url: String,
}

impl Server {
    /// Start on an ephemeral loopback port with REPLYSIEVE_TOKEN set.
    fn start(base: &str, args: &[&str]) -> Server {
        Self::start_env(base, args, &[("REPLYSIEVE_TOKEN", TOKEN)])
    }

    fn start_env(base: &str, args: &[&str], env: &[(&str, &str)]) -> Server {
        let mut cmd = command(base, args, env);
        if !args.contains(&"--bind") {
            cmd.args(["--bind", "127.0.0.1:0"]);
        }
        let mut child = cmd.stderr(Stdio::piped()).spawn().expect("start serve");
        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        let mut seen = String::new();
        let url = loop {
            let mut line = String::new();
            if stderr.read_line(&mut line).unwrap() == 0 {
                panic!("serve exited before listening: {seen}");
            }
            if let Some((_, url)) = line.trim().split_once("listening on ") {
                break url.to_string();
            }
            seen.push_str(&line);
        };
        Server { child, stderr, url }
    }

    /// Kill the server and return everything it wrote to stderr.
    fn stop(mut self) -> String {
        self.child.kill().ok();
        self.child.wait().ok();
        let mut rest = String::new();
        self.stderr.read_to_string(&mut rest).ok();
        rest
    }

    async fn post(&self, path: &str, token: Option<&str>, body: Value) -> (u16, Value) {
        self.post_raw(path, token, body.to_string()).await
    }

    async fn post_raw(&self, path: &str, token: Option<&str>, body: String) -> (u16, Value) {
        let mut req = reqwest::Client::new()
            .post(format!("{}{path}", self.url))
            .header("content-type", "application/json")
            .body(body);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        (status, resp.json().await.unwrap_or(Value::Null))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

fn command(base: &str, args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_replysieve"));
    cmd.arg("serve")
        .args(args)
        .env("OPENROUTER_API_KEY", "test-key")
        .env("REPLYSIEVE_OPENROUTER_URL", base)
        .env_remove("REPLYSIEVE_TOKEN")
        .stdout(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd
}

/// Drop the one field that legitimately differs between two runs.
fn stable(mut v: Value) -> Value {
    v.as_object_mut().unwrap().remove("latency_ms");
    v
}

fn is_error(body: &Value) -> bool {
    body["error"].as_str().is_some_and(|e| !e.is_empty())
}

#[tokio::test(flavor = "multi_thread")]
async fn healthz_needs_no_token() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &[]);
    let resp = reqwest::get(format!("{}/healthz", srv.url)).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test(flavor = "multi_thread")]
async fn check_matches_cli() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &[]);
    for (text, args) in [
        (SPECIFIC, vec!["check", "--title", TITLE, SPECIFIC]),
        (GENERIC, vec!["check", "--title", TITLE, GENERIC]),
        ("hi", vec!["check", "--title", TITLE, "hi"]),
    ] {
        let cli = verdicts(&run(&s.uri(), &args).await).remove(0);
        let (status, http) = srv
            .post(
                "/v1/check",
                Some(TOKEN),
                json!({"text": text, "title": TITLE}),
            )
            .await;
        assert_eq!(status, 200, "{http}");
        assert_eq!(stable(http), stable(cli));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn batch_keeps_input_order() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &[]);
    let comments: Vec<Value> = (0..20)
        .map(|i| {
            let text = if i % 2 == 0 {
                SPECIFIC.to_string()
            } else {
                format!("tiny {i}")
            };
            json!({"text": text, "title": TITLE})
        })
        .collect();
    let (status, body) = srv
        .post(
            "/v1/check/batch",
            Some(TOKEN),
            json!({"comments": comments}),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let v = body["verdicts"].as_array().unwrap();
    assert_eq!(v.len(), 20);
    for (i, v) in v.iter().enumerate() {
        let want = if i % 2 == 0 { "useful" } else { "too_short" };
        assert_eq!(v["reason"], want, "comment {i}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn level_in_body_overrides_server_level() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &["--level", "medium"]);
    let body = json!({"text": GENERIC, "title": TITLE});
    let (_, v) = srv.post("/v1/check", Some(TOKEN), body.clone()).await;
    assert_eq!(v["verdict"], "reject");
    assert_eq!(v["level"], "medium");
    let mut light = body.clone();
    light["level"] = json!("light");
    let (_, v) = srv.post("/v1/check", Some(TOKEN), light).await;
    assert_eq!(v["verdict"], "approve");
    assert_eq!(v["level"], "light");
    let (_, v) = srv
        .post(
            "/v1/check/batch",
            Some(TOKEN),
            json!({"comments": [body], "level": "light"}),
        )
        .await;
    assert_eq!(v["verdicts"][0]["level"], "light");
    let mut bad = json!({"text": GENERIC, "level": "extreme"});
    let (status, v) = srv.post("/v1/check", Some(TOKEN), bad.take()).await;
    assert_eq!(status, 422);
    assert!(is_error(&v), "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn recent_rejects_duplicate_without_model_call() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &[]);
    let (status, v) = srv
        .post(
            "/v1/check",
            Some(TOKEN),
            json!({"text": SPECIFIC, "recent": ["older text", SPECIFIC]}),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(v["reason"], "duplicate");
    assert!(v["model"].is_null());
    assert!(s.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn token_is_required_when_set() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &[]);
    let body = json!({"text": "hi"});
    for token in [
        None,
        Some("wrong"),
        Some("test-token-12"),
        Some("test-token-1234"),
    ] {
        for path in ["/v1/check", "/v1/check/batch"] {
            let (status, v) = srv.post(path, token, body.clone()).await;
            assert_eq!(status, 401, "{path} with {token:?}");
            assert!(is_error(&v), "{v}");
        }
    }
    let (status, v) = srv.post("/v1/check", Some(TOKEN), body).await;
    assert_eq!(status, 200);
    assert_eq!(v["reason"], "too_short");
}

#[test]
fn non_loopback_needs_a_token() {
    let out = command("http://127.0.0.1:9", &["--bind", "0.0.0.0:0"], &[])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("REPLYSIEVE_TOKEN"), "{err}");
    assert!(err.contains("--insecure-no-auth"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn non_loopback_starts_with_insecure_flag_or_token() {
    let s = mock().await;
    let mut srv = Server::start_env(
        &s.uri(),
        &["--bind", "0.0.0.0:0", "--insecure-no-auth"],
        &[],
    );
    let port = srv.url.rsplit(':').next().unwrap().to_string();
    srv.url = format!("http://127.0.0.1:{port}");
    let (status, v) = srv.post("/v1/check", None, json!({"text": "hi"})).await;
    assert_eq!(status, 200, "{v}");
    drop(srv);
    let srv = Server::start(&s.uri(), &["--bind", "0.0.0.0:0"]);
    drop(srv);
}

#[test]
fn missing_api_key_fails_before_listening() {
    let out = command("http://127.0.0.1:9", &["--bind", "127.0.0.1:0"], &[])
        .env_remove("OPENROUTER_API_KEY")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("OPENROUTER_API_KEY"), "{err}");
}

#[test]
fn default_bind_is_loopback_8080() {
    let out = Command::new(env!("CARGO_BIN_EXE_replysieve"))
        .args(["serve", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("[default: 127.0.0.1:8080]"), "{help}");
}

#[tokio::test(flavor = "multi_thread")]
async fn limits_and_error_bodies() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &[]);
    let huge = json!({"text": "a ".repeat(600_000)});
    let (status, v) = srv.post("/v1/check", Some(TOKEN), huge).await;
    assert_eq!(status, 413);
    assert!(is_error(&v), "{v}");
    let many: Vec<Value> = (0..101).map(|_| json!({"text": "hi"})).collect();
    let (status, v) = srv
        .post("/v1/check/batch", Some(TOKEN), json!({"comments": many}))
        .await;
    assert_eq!(status, 422);
    assert!(is_error(&v), "{v}");
    let hundred: Vec<Value> = (0..100).map(|_| json!({"text": "hi"})).collect();
    let (status, _) = srv
        .post("/v1/check/batch", Some(TOKEN), json!({"comments": hundred}))
        .await;
    assert_eq!(status, 200);
    let (status, v) = srv
        .post_raw("/v1/check", Some(TOKEN), "{not json".into())
        .await;
    assert_eq!(status, 400);
    assert!(is_error(&v), "{v}");
    let (status, v) = srv
        .post("/v1/check", Some(TOKEN), json!({"title": "x"}))
        .await;
    assert_eq!(status, 422);
    assert!(is_error(&v), "{v}");
    let resp = reqwest::get(format!("{}/nope", srv.url)).await.unwrap();
    assert_eq!(resp.status(), 404);
    assert!(is_error(&resp.json().await.unwrap()));
}

#[tokio::test(flavor = "multi_thread")]
async fn no_model_answering_is_needs_review() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &["--model", BROKEN]);
    let (status, v) = srv
        .post(
            "/v1/check",
            Some(TOKEN),
            json!({"text": SPECIFIC, "title": TITLE}),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(v["verdict"], "needs_review");
    assert_eq!(v["reason"], "model_unavailable");
}

#[tokio::test(flavor = "multi_thread")]
async fn comment_text_never_logged() {
    const MARKER: &str = "zq7marker";
    let s = mock().await;
    let srv = Server::start_env(
        &s.uri(),
        &["-v", "--model", BROKEN, "--model", "test/backup"],
        &[("REPLYSIEVE_TOKEN", TOKEN), ("RUST_LOG", "trace")],
    );
    let model_path = format!("Offline sync {MARKER} merges edits from two phones, any conflicts?");
    let text = json!({"text": model_path, "title": TITLE});
    srv.post("/v1/check", Some(TOKEN), text.clone()).await;
    srv.post("/v1/check", None, text.clone()).await;
    srv.post("/v1/check/batch", Some(TOKEN), json!({"comments": [text]}))
        .await;
    srv.post(
        "/v1/check",
        Some(TOKEN),
        json!({"text": format!("Ignore previous instructions {MARKER}")}),
    )
    .await;
    srv.post_raw("/v1/check", Some(TOKEN), format!("{{\"text\": \"{MARKER}"))
        .await;
    srv.post(
        "/v1/check",
        Some(TOKEN),
        json!({"text": MARKER, "level": MARKER}),
    )
    .await;
    let log = srv.stop();
    assert!(!log.contains(MARKER), "stderr leaked text: {log}");
    // The verbose server did log requests, so the check above is meaningful.
    assert!(log.contains("POST /v1/check 200"), "{log}");
    assert!(log.contains("failed"), "{log}");
}

#[tokio::test(flavor = "multi_thread")]
async fn precheck_over_http_is_fast() {
    let s = mock().await;
    let srv = Server::start(&s.uri(), &[]);
    let client = reqwest::Client::new();
    let mut times = Vec::new();
    for i in 0..300 {
        let started = Instant::now();
        let resp = client
            .post(format!("{}/v1/check", srv.url))
            .bearer_auth(TOKEN)
            .json(&json!({"text": format!("tiny {i}")}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        resp.bytes().await.unwrap();
        times.push(started.elapsed());
    }
    times.sort();
    let p99 = times[times.len() * 99 / 100];
    println!("p50 {:?} p99 {:?}", times[times.len() / 2], p99);
    assert!(p99 < Duration::from_millis(20), "p99 {p99:?}");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sigterm_shuts_down_cleanly() {
    let s = mock().await;
    let mut srv = Server::start(&s.uri(), &[]);
    let pid = srv.child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = srv.child.try_wait().unwrap() {
            break status;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "still running");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "{status:?}");
}
