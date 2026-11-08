<!--
Generated from canonical UltraShip state. Do not edit directly.
Run `ultraship views` to regenerate.
-->
# Release history

## replysieve

| Version | Released | Mode | Delivered |
| --- | --- | --- | --- |
| 0.1.0 | 2026-11-08T02:54:36Z | published | replysieve 0.1.0 is public at github.com/zeroonething/replysieve and on crates.io. A developer installs it with cargo install replysieve, judges one comment or a JSONL batch from the CLI or from Claude Code and Codex over MCP, at light, medium or heavy strictness with a preset or their own rules, and gets approve, reject or needs_review with a reason and confidence from ~typesafe/jev-latest on OpenRouter. |

### 0.1.0 known limitations

- Accuracy is measured on a 302-comment set drafted with an AI model and reviewed by one person, so it may share blind spots and is not a promise for other communities
- Accuracy depends on ~typesafe/jev-latest, which moves; thresholds were not re-checked against a pinned model version
- Heavy strictness sends most on-topic-but-nonspecific comments to needs_review rather than rejecting them (15 of 302 in the eval)
- English only; model path takes roughly 0.5 to 1.5 s per comment
- crates.io publish uses a stored environment token; trusted publishing is not set up yet
- cargo install verified on macOS and Linux only, not Windows


_Canonical sources: products/<id>/releases/<version>.yaml_
