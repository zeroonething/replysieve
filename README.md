# replysieve

Keep the real comments, drop the junk. replysieve judges a user comment in about
a second and tells you to approve it, reject it, or send it for review, with a
reason. It runs free checks first and then asks a small decision model on
OpenRouter, so it is much cheaper and faster than asking a chat LLM.

It works as a CLI, an HTTP server (also as a Docker image), an MCP server for
coding agents, and a Rust library.

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

## Run it as an HTTP server

`replysieve serve` puts the same checks behind HTTP, so your backend can call
them in any language. With Docker:

```sh
export OPENROUTER_API_KEY=sk-or-...
export REPLYSIEVE_TOKEN=$(openssl rand -hex 32)   # callers send this as a bearer token

docker run -d -p 8080:8080 -e OPENROUTER_API_KEY -e REPLYSIEVE_TOKEN \
  ghcr.io/zeroonething/replysieve:0.2.0
```

The image is about 44 MB, runs on linux/amd64 and linux/arm64 as a non-root
user, and listens on port 8080. It is tagged `0.2.0`, `0.2` and `latest`. To
change flags, put the whole command after the image name. A config file can be
mounted:

```sh
docker run -d -p 8080:8080 -e OPENROUTER_API_KEY -e REPLYSIEVE_TOKEN \
  -v "$PWD/replysieve.toml:/replysieve.toml:ro" \
  ghcr.io/zeroonething/replysieve:0.2.0 \
  serve --bind 0.0.0.0:8080 --level heavy --config /replysieve.toml
```

Or without Docker:

```sh
cargo install replysieve
replysieve serve   # listens on http://127.0.0.1:8080
```

`serve` takes `--bind` plus the same `--level`, `--preset`, `--config`,
`--model` and `-v` flags as `check`. It needs `OPENROUTER_API_KEY` and will not
start without it.

Judge one comment:

```sh
curl -s http://127.0.0.1:8080/v1/check \
  -H "Authorization: Bearer $REPLYSIEVE_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"text": "Does offline sync merge edits if I change the same task on two phones?",
       "title": "Taskly, a todo app with offline sync"}'
```

```json
{"verdict":"approve","reason":"useful","confidence":1.0,"level":"medium","model":"typesafe/jev-1.14-20261014","latency_ms":712}
```

The body takes `text` and, optionally, `title`, `description`, `parent`,
`level` and `recent` (a list of recent texts; an identical comment is a
duplicate). `level` overrides the server's level for that request.

Judge up to 100 comments at once. Verdicts come back in the same order:

```sh
curl -s http://127.0.0.1:8080/v1/check/batch \
  -H "Authorization: Bearer $REPLYSIEVE_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"comments": [{"text": "Great product, great job, well done!"}, {"text": "first"}]}'
```

```json
{"verdicts":[{"verdict":"reject","reason":"stock_phrase","confidence":1.0,"level":"medium","model":null,"latency_ms":0},{"verdict":"reject","reason":"too_short","confidence":1.0,"level":"medium","model":null,"latency_ms":0}]}
```

Each entry in `comments` takes `text` and, optionally, `title`, `description`
and `parent`. `level` and `recent` go next to `comments` and apply to all of
them.

### Call it from your backend

JavaScript (Node 18 or later):

```js
async function checkComment(text, title) {
  const res = await fetch("http://127.0.0.1:8080/v1/check", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${process.env.REPLYSIEVE_TOKEN}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ text, title }),
  });
  const body = await res.json();
  if (!res.ok) throw new Error(`replysieve: ${body.error}`);
  return body; // { verdict, reason, confidence, level, model, latency_ms }
}
```

Python, with `requests`:

```python
import os
import requests

def check_comment(text, title=None):
    resp = requests.post(
        "http://127.0.0.1:8080/v1/check",
        headers={"Authorization": f"Bearer {os.environ['REPLYSIEVE_TOKEN']}"},
        json={"text": text, "title": title},
        timeout=30,
    )
    resp.raise_for_status()
    return resp.json()  # {"verdict": ..., "reason": ..., ...}
```

Publish `approve`, drop `reject`, and hold `needs_review` for a person.

### Endpoints and errors

| Request | Token needed | Response |
| --- | --- | --- |
| `POST /v1/check` | yes, when set | one verdict |
| `POST /v1/check/batch` | yes, when set | `{"verdicts": [...]}` in input order |
| `GET /healthz` | no | `{"status":"ok","version":"0.2.0"}` |

Requests must send `Content-Type: application/json`. Every error has a JSON
body, `{"error": "..."}`.

| Status | When |
| --- | --- |
| 400 | the body is not valid JSON |
| 401 | the bearer token is missing or wrong |
| 413 | the body is larger than 1 MiB |
| 415 | `Content-Type` is not `application/json` |
| 422 | a field is missing or has the wrong type, or a batch has more than 100 comments |

### Security

- With `REPLYSIEVE_TOKEN` set, every `/v1` request needs
  `Authorization: Bearer <token>`. Without a token, anyone who can reach the
  port can spend your OpenRouter credits.
- `replysieve serve` listens on 127.0.0.1 by default. It refuses any other
  address unless `REPLYSIEVE_TOKEN` is set, or you pass `--insecure-no-auth`
  because something in front of it already authenticates callers. The Docker
  image listens on 0.0.0.0, so it needs a token too.
- There is no CORS. Call it from your server, never from browser JavaScript,
  where anyone could read the token.
- It speaks plain HTTP. Put TLS and rate limiting in a reverse proxy such as
  nginx or Caddy.
- It shuts down cleanly on Ctrl-C or SIGTERM, so `docker stop` lets requests in
  flight finish.

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

`checker.check_batch(&comments, &recent)` judges many comments at once and
returns the verdicts in input order. Docs are on
[docs.rs/replysieve](https://docs.rs/replysieve).

## Accuracy

`replysieve eval` runs the 302 labeled example comments that ship with the crate
(product reviews, Reddit-style threads, X replies and launch comments) and
reports, per level, how much junk was rejected and how many good comments were
wrongly rejected. With `~typesafe/jev-latest` on 2026-11-08:

| Level | Junk caught | Good wrongly rejected | Needs review |
| --- | --- | --- | --- |
| light | 100% (90/90) | 0% (0/212) | 0 |
| medium | 100% (150/150) | 0% (0/152) | 0 |
| heavy | 92.8% (194/209) | 0% (0/93) | 15 |

At heavy, the misses are on-topic comments with no specific point that the model
was unsure about, so they go to review rather than being rejected. The set was
drafted with an AI model and reviewed by hand, so treat these numbers as a check
that the pipeline works, not as a promise for your own comments. Run
`replysieve eval --file your-set.jsonl --misses` on your own labeled comments.

## Privacy

replysieve is stateless. It never writes comment text to disk or logs, at any
verbosity. `-v` prints only timing and model fallback events, and for `serve` one
line per request with the method, path, status and latency. Comment text goes
only to OpenRouter, using your own key.

## Not in 0.2.0

English only. No images or video. No hosted service: you run replysieve yourself.
replysieve gives verdicts and leaves deleting, hiding or banning to you. Recipes
for specific platforms and a way to test your own rules against examples come
later.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Contributions are welcome, see
[CONTRIBUTING.md](CONTRIBUTING.md).
