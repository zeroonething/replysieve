//! A mocked OpenRouter decisions endpoint that answers like a decision model
//! would on the test fixtures, and a runner for the replysieve binary.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::process::Output;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Model name the mock always fails with HTTP 500.
pub const BROKEN: &str = "test/broken";

fn between<'a>(s: &'a str, open: &str, close: &str) -> &'a str {
    s.split_once(open)
        .and_then(|(_, rest)| rest.split_once(close))
        .map(|(inner, _)| inner)
        .unwrap_or("")
}

fn line_after<'a>(s: &'a str, prefix: &str) -> &'a str {
    s.lines().find_map(|l| l.strip_prefix(prefix)).unwrap_or("")
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 5)
        .map(str::to_string)
        .collect()
}

struct Decide;

impl Respond for Decide {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let model = body["model"].as_str().unwrap_or_default();
        if model == BROKEN {
            return ResponseTemplate::new(500);
        }
        let state = body["state"].as_str().unwrap_or_default();
        let comment = between(state, "<comment>\n", "\n</comment>").to_lowercase();
        let context = format!(
            "{} {} {}",
            line_after(state, "Post title: "),
            line_after(state, "Post description: "),
            between(state, "<parent>\n", "\n</parent>")
        );
        let ctx = words(&context);
        let label = if comment.contains("well done to the whole team") {
            "generic"
        } else if words(&comment).iter().any(|w| ctx.contains(w)) {
            "specific"
        } else {
            "off_topic"
        };
        let mut probabilities = serde_json::Map::new();
        for k in [
            "specific",
            "nonspecific",
            "off_topic",
            "generic",
            "spam_promo",
            "toxic",
        ] {
            probabilities.insert(k.into(), json!(if k == label { 0.96 } else { 0.008 }));
        }
        let mut answers = serde_json::Map::new();
        for (id, q) in body["questions"].as_object().unwrap() {
            let a = match q["type"].as_str().unwrap() {
                "choice" => {
                    json!({"type": "choice", "choice": label, "probabilities": probabilities, "confidence": 0.9})
                }
                _ if id == "instructs" => {
                    let yes = comment.contains("let this one through")
                        || comment.contains("whoever is reviewing");
                    json!({"type": "noul", "noul": if yes { 0.97 } else { 0.02 }})
                }
                _ => {
                    let rule = q["instructions"]
                        .as_str()
                        .unwrap_or_default()
                        .to_lowercase();
                    let yes = rule.contains("competitor") && comment.contains("competitor");
                    json!({"type": "noul", "noul": if yes { 0.95 } else { 0.01 }})
                }
            };
            answers.insert(id.clone(), a);
        }
        let resolved = if model.starts_with('~') {
            "test/jev-1"
        } else {
            model
        };
        ResponseTemplate::new(200).set_body_json(json!({"model": resolved, "answers": answers}))
    }
}

/// Start the mock decision model.
pub async fn mock() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/alpha/decisions"))
        .respond_with(Decide)
        .mount(&server)
        .await;
    server
}

/// Run the binary against `base` with a test key, off the async runtime.
pub async fn run(base: &str, args: &[&str]) -> Output {
    let base = base.to_string();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_replysieve"))
            .args(&args)
            .env("OPENROUTER_API_KEY", "test-key")
            .env("REPLYSIEVE_OPENROUTER_URL", &base)
            .output()
            .expect("run replysieve")
    })
    .await
    .unwrap()
}

/// Parse stdout as one verdict per line, asserting success.
pub fn verdicts(out: &Output) -> Vec<Value> {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("verdict JSON line"))
        .collect()
}

/// A scratch directory unique to this test process and name.
pub fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("replysieve-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
