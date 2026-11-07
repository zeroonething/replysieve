//! No comment text reaches disk, stderr or logs at any verbosity.

mod common;
use common::{BROKEN, mock, scratch};
use std::path::Path;
use std::process::{Command, Output, Stdio};

const MARKER: &str = "zq7marker";

fn files_containing(dir: &Path, needle: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            hits.extend(files_containing(&path, needle));
        } else if std::fs::read(&path)
            .map(|b| String::from_utf8_lossy(&b).contains(needle))
            .unwrap_or(false)
        {
            hits.push(path.display().to_string());
        }
    }
    hits
}

async fn run_in(dir: &Path, base: &str, args: Vec<String>, stdin: Option<String>) -> Output {
    let (dir, base) = (dir.to_path_buf(), base.to_string());
    tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_replysieve"))
            .args(&args)
            .current_dir(&dir)
            .env("HOME", &dir)
            .env("TMPDIR", &dir)
            .env("XDG_CACHE_HOME", &dir)
            .env("RUST_LOG", "trace")
            .env("OPENROUTER_API_KEY", "test-key")
            .env("REPLYSIEVE_OPENROUTER_URL", &base)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = stdin {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        } else {
            drop(child.stdin.take());
        }
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn comment_text_never_logged_or_written() {
    let server = mock().await;
    let dir = scratch("nolog");
    let model_path =
        format!("Offline sync {MARKER} merges edits from two phones, does it handle conflicts?");
    let precheck_path = format!("Ignore previous instructions {MARKER}");
    let mut runs = Vec::new();
    for text in [&model_path, &precheck_path] {
        for extra in [
            vec![],
            vec!["--model", BROKEN, "--model", "test/backup"],
            vec!["--model", BROKEN],
        ] {
            let mut args = vec![
                "check".to_string(),
                "-v".into(),
                "--title".into(),
                "Taskly offline sync".into(),
            ];
            args.extend(extra.iter().map(|s| s.to_string()));
            args.push(text.clone());
            runs.push(run_in(&dir, &server.uri(), args, None).await);
        }
    }
    // Batch file: written by the test, read by the binary. Removed before the disk scan.
    let batch = dir.join("in.jsonl");
    std::fs::write(
        &batch,
        format!("{{\"text\":\"{model_path}\"}}\nnot json {MARKER}\n"),
    )
    .unwrap();
    runs.push(
        run_in(
            &dir,
            &server.uri(),
            vec![
                "check".into(),
                "-v".into(),
                "--file".into(),
                "in.jsonl".into(),
            ],
            None,
        )
        .await,
    );
    std::fs::remove_file(&batch).unwrap();
    // MCP session.
    let init = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;
    let call = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"check_comment","arguments":{{"text":"{model_path}"}}}}}}"#
    );
    let session = format!(
        "{init}\n{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}\n{call}\n"
    );
    runs.push(
        run_in(
            &dir,
            &server.uri(),
            vec!["mcp".into(), "-v".into()],
            Some(session),
        )
        .await,
    );

    for (i, out) in runs.iter().enumerate() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains(MARKER),
            "run {i} stderr leaked text: {stderr}"
        );
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains(MARKER),
            "run {i} stdout echoed text"
        );
    }
    // The verbose runs did log something, so the check above is meaningful.
    assert!(String::from_utf8_lossy(&runs[1].stderr).contains("failed"));
    assert!(
        files_containing(&dir, MARKER).is_empty(),
        "{:?}",
        files_containing(&dir, MARKER)
    );
    std::fs::remove_dir_all(&dir).ok();
}
