# replysieve

Keep the real comments, drop the junk. replysieve judges a user comment in about
a second and tells you to approve it, reject it, or send it for review, with a
reason. It runs free checks first and then asks a small decision model on
OpenRouter, so it is much cheaper and faster than asking a chat LLM.

It works as a CLI, an MCP server for coding agents, and a Rust library.

```sh
cargo install replysieve
export OPENROUTER_API_KEY=sk-or-...   # https://openrouter.ai/keys

replysieve check --title "Taskly, a todo app with offline sync" \
  "Does offline sync merge edits if I change the same task on two phones?"
```

```json
{"verdict":"approve","reason":"useful","confidence":1.0,"level":"medium","model":"typesafe/jev-1.14-20261014","latency_ms":712}
```

### Use it from Claude Code or Codex

```sh
claude mcp add replysieve -e OPENROUTER_API_KEY=sk-or-... -- replysieve mcp
codex mcp add replysieve --env OPENROUTER_API_KEY=sk-or-... -- replysieve mcp
```

Then ask your agent something like "check these comments with replysieve at heavy
strictness". It gets two tools, `check_comment` and `check_batch`.

## Strictness levels

| Level | Rejects | Minimum words |
| --- | --- | --- |
| `light` | spam and self-promotion, toxic text, attempts to instruct the moderator | 1 |
| `medium` (default) | all of the above, plus generic low-effort comments like "Great product!" | 4 |
| `heavy` | all of the above, plus off-topic comments and anything without a specific point about the post | 8 |

Your own rules apply at every level. The level only changes the built-in checks.

## Verdict JSON

| Field | Meaning |
| --- | --- |
| `verdict` | `approve`, `reject` or `needs_review` |
| `reason` | `useful`, `too_short`, `stock_phrase`, `duplicate`, `injection_attempt`, `spam_promo`, `toxic`, `generic`, `off_topic`, `not_specific`, `custom_rule`, `low_confidence` or `model_unavailable` |
| `confidence` | the model's probability for the outcome, or 1.0 when a free pre-check decided |
| `level` | the level used |
| `model` | the model that answered, or `null` when no model was called |
| `latency_ms` | time spent on this comment |
| `rule` | the custom rule that fired, only when `reason` is `custom_rule` |

If no model answers, the verdict is `needs_review` with reason
`model_unavailable`. replysieve never approves silently on failure.

## Context, batches and duplicates

```sh
# Context helps the heavy level tell on-topic from off-topic
replysieve check --level heavy \
  --title "Brewly" --description "Coffee beans by mail, roasted weekly" \
  --parent "Has anyone tried the decaf?" \
  "The decaf is surprisingly good, a bit flat in milk though"

# A batch, one JSON object per line, verdicts come back in the same order
replysieve check --file comments.jsonl
#   {"text": "...", "title": "...", "description": "...", "parent": "..."}

# Reject exact repeats of recent texts (one per line)
replysieve check --recent recent.txt "Try my app, link in bio"
```

## Presets and custom rules

`--preset comments` (default), `product-review` or `launch-thread` tells the model
what kind of place the comment came from and adds stock phrases for that setting.

Put your own rules and defaults in a TOML file and pass `--config replysieve.toml`.

```toml
level = "medium"
preset = "launch-thread"
models = ["~typesafe/jev-latest"]        # tried in order
rules = [
  "No links to other products",
  "No questions about pricing, those go to support",
]
stock_phrases = ["nice ship"]
```

Command-line flags override the file. `--model` can be repeated to set the
fallback order.

## Rust library

```rust,no_run
use replysieve::{Checker, Comment, Level, Options};

# async fn run() -> Result<(), replysieve::Error> {
let checker = Checker::new(
    Options { level: Level::Heavy, ..Options::default() },
    std::env::var("OPENROUTER_API_KEY").ok(),
);
let comment = Comment {
    title: Some("Taskly, a todo app with offline sync".into()),
    ..Comment::new("Does sync merge edits made on two phones?")
};
let verdict = checker.check(&comment, &[]).await?;
println!("{:?} {:?}", verdict.verdict, verdict.reason);
# Ok(())
# }
```

Docs are on [docs.rs/replysieve](https://docs.rs/replysieve).

## Privacy

replysieve is stateless. It never writes comment text to disk or logs, at any
verbosity (`-v` prints only timing and model fallback events). Comment text goes
only to OpenRouter, using your own key.

## Not in 0.1.0

An HTTP server and Docker image are planned for 0.2.0. English only. No images or
video. replysieve gives verdicts and leaves deleting, hiding or banning to you.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Contributions are welcome, see
[CONTRIBUTING.md](CONTRIBUTING.md).
