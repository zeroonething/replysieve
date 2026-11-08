//! `replysieve mcp` over stdio returns the same verdicts as the CLI.

mod common;
use common::{mock, run, verdicts};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const TITLE: &str = "Taskly: a todo app with offline sync";
const SPECIFIC: &str = "Does offline sync merge edits when I change one task on two phones?";
const GENERIC: &str = "This is a really nice app, well done to the whole team";

/// Drop the one field that legitimately differs between two runs.
fn stable(mut v: Value) -> Value {
    v.as_object_mut().unwrap().remove("latency_ms");
    v
}

fn mcp_session(base: &str, calls: Vec<Value>) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_replysieve"))
        .arg("mcp")
        .env("OPENROUTER_API_KEY", "test-key")
        .env("REPLYSIEVE_OPENROUTER_URL", base)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start replysieve mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut send = |m: Value| writeln!(stdin, "{m}").unwrap();
    let mut recv = |id: u64| loop {
        let line = lines.next().expect("server closed stdout").unwrap();
        let m: Value = serde_json::from_str(&line).expect("JSON-RPC line");
        if m["id"] == id {
            return m;
        }
    };
    send(
        json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}}),
    );
    recv(0);
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let mut out = Vec::new();
    for (i, call) in calls.into_iter().enumerate() {
        let id = i as u64 + 1;
        let mut call = call;
        call["jsonrpc"] = json!("2.0");
        call["id"] = json!(id);
        send(call);
        out.push(recv(id));
    }
    // Closing stdin ends the session; the server must exit cleanly.
    drop(stdin);
    assert!(child.wait().unwrap().success());
    out
}

fn tool(name: &str, args: Value) -> Value {
    json!({"method": "tools/call", "params": {"name": name, "arguments": args}})
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_match_cli_verdicts() {
    let s = mock().await;
    let base = s.uri();
    let cli_specific =
        stable(verdicts(&run(&base, &["check", "--title", TITLE, SPECIFIC]).await).remove(0));
    let cli_generic_heavy = stable(
        verdicts(
            &run(
                &base,
                &["check", "--level", "heavy", "--title", TITLE, GENERIC],
            )
            .await,
        )
        .remove(0),
    );
    let cli_short = stable(verdicts(&run(&base, &["check", "ok"]).await).remove(0));

    let base2 = base.clone();
    let replies = tokio::task::spawn_blocking(move || {
        mcp_session(
            &base2,
            vec![
                json!({"method": "tools/list", "params": {}}),
                tool("check_comment", json!({"text": SPECIFIC, "title": TITLE})),
                tool(
                    "check_comment",
                    json!({"text": GENERIC, "title": TITLE, "level": "heavy"}),
                ),
                tool(
                    "check_batch",
                    json!({"comments": [{"text": SPECIFIC, "title": TITLE}, {"text": "ok"}]}),
                ),
            ],
        )
    })
    .await
    .unwrap();

    let names: Vec<&str> = replies[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"check_comment") && names.contains(&"check_batch"),
        "{names:?}"
    );

    let structured = |r: &Value| r["result"]["structuredContent"].clone();
    assert_eq!(stable(structured(&replies[1])), cli_specific);
    assert_eq!(stable(structured(&replies[2])), cli_generic_heavy);
    let batch = structured(&replies[3])["verdicts"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(batch.len(), 2);
    assert_eq!(stable(batch[0].clone()), cli_specific);
    assert_eq!(stable(batch[1].clone()), cli_short);
}
