//! CLI behavior that needs no API key and no network.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_replysieve"))
        .args(args)
        .env_remove("OPENROUTER_API_KEY")
        // Any attempted request would go to a closed port and fail.
        .env("REPLYSIEVE_OPENROUTER_URL", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .output()
        .expect("run replysieve")
}

fn verdict(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("one verdict JSON object")
}

#[test]
fn too_short_is_rejected_without_network() {
    let v = verdict(&run(&["check", "Nice tool"]));
    assert_eq!(v["verdict"], "reject");
    assert_eq!(v["reason"], "too_short");
    assert_eq!(v["level"], "medium");
    assert!(v["model"].is_null());
    assert!(v["latency_ms"].is_u64());
    assert_eq!(v["confidence"], 1.0);
}

#[test]
fn stock_phrase_is_rejected_without_network() {
    let v = verdict(&run(&[
        "check",
        "--preset",
        "launch-thread",
        "Congrats on the launch guys, this is awesome!",
    ]));
    assert_eq!(v["verdict"], "reject");
    assert_eq!(v["reason"], "stock_phrase");
}

#[test]
fn duplicate_via_recent_is_rejected_without_network() {
    let dir = std::env::temp_dir().join(format!("replysieve-recent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let recent = dir.join("recent.txt");
    std::fs::write(
        &recent,
        "Try my budgeting app, link in bio for a discount\n",
    )
    .unwrap();
    let v = verdict(&run(&[
        "check",
        "--level",
        "light",
        "--recent",
        recent.to_str().unwrap(),
        "try my budgeting app — link in bio for a discount!",
    ]));
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(v["verdict"], "reject");
    assert_eq!(v["reason"], "duplicate");
}

#[test]
fn every_preset_loads() {
    for preset in ["comments", "product-review", "launch-thread"] {
        let v = verdict(&run(&["check", "--preset", preset, "ok"]));
        assert_eq!(v["reason"], "too_short", "{preset}");
    }
}

#[test]
fn unknown_preset_fails_clearly() {
    let out = run(&[
        "check",
        "--preset",
        "forum",
        "Some comment here that is long",
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown preset 'forum'"), "{err}");
    assert!(out.stdout.is_empty());
}

#[test]
fn missing_api_key_fails_before_network() {
    let out = run(&[
        "check",
        "The offline mode lost two of my notes after the update",
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("OPENROUTER_API_KEY is not set"), "{err}");
    assert!(out.stdout.is_empty());
}

#[test]
fn unknown_level_fails_clearly() {
    let out = run(&["check", "--level", "extreme", "hello there"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown level 'extreme'"));
}
