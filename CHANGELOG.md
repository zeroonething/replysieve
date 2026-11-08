# Changelog

All notable changes to replysieve are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.2.0] - 2026-11-08

replysieve now runs as an HTTP server and ships as a Docker image.

### Added

- `replysieve serve` runs an HTTP server with `POST /v1/check`, `POST /v1/check/batch` and `GET /healthz`. Request bodies match the MCP tool arguments, and responses are the same verdict JSON the CLI prints, with batch verdicts in input order.
- Bearer token auth with `REPLYSIEVE_TOKEN`. The server listens on 127.0.0.1:8080 by default and refuses a non-loopback address without a token unless `--insecure-no-auth` is passed. It will not start without `OPENROUTER_API_KEY`.
- Request limits: bodies up to 1 MiB and batches up to 100 comments. Every error returns a JSON body `{"error": "..."}` with status 400, 401, 413, 415 or 422.
- Graceful shutdown on Ctrl-C and SIGTERM. With `-v` the server logs method, path, status and latency per request, never comment text.
- Docker image `ghcr.io/zeroonething/replysieve` (tags `0.2.0`, `0.2`, `latest`) for linux/amd64 and linux/arm64. It is distroless, about 44 MB, runs as a non-root user and listens on 0.0.0.0:8080.
- `Checker::check_batch` in the Rust library judges many comments and returns verdicts in input order.
## [0.1.0] - 2026-11-08

First release.

### Added

- `replysieve check` judges one comment, or a JSONL batch with `--file`, and prints verdict JSON with verdict, reason, confidence, level, model and latency_ms.
- Three strictness levels. Light rejects spam, toxic text and injection attempts, medium also rejects generic comments, and heavy also rejects off-topic comments and anything without a specific point.
- Built-in presets `comments`, `product-review` and `launch-thread`, plus plain-English custom rules in a TOML config file that apply at every level.
- Free pre-checks with no model call for minimum length, stock phrases and duplicates of recent texts passed with `--recent`.
- Injection guard. Comment text is sent strictly as fenced data, and text that tries to instruct the moderator is rejected.
- OpenRouter decisions connector, with the models in your list tried in order. When no model answers the verdict is needs_review with reason model_unavailable.
- `replysieve mcp` serves `check_comment` and `check_batch` over stdio for Claude Code, Codex and other MCP clients.
- `replysieve eval` reports junk caught and good comments wrongly rejected per level over a labeled set of 302 example comments that ships with the crate.
- Rust library API (`Checker`, `Comment`, `Verdict`) documented on docs.rs.

[0.2.0]: https://github.com/zeroonething/replysieve/releases/tag/v0.2.0
[0.1.0]: https://github.com/zeroonething/replysieve/releases/tag/v0.1.0
