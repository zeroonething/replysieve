# @zeroonething/replysieve

Keep the real comments, drop the junk. replysieve judges a user comment and tells
you to approve it, reject it, or send it for review, with a reason. It runs free
checks first and then asks a small decision model on OpenRouter
([get a key](https://openrouter.ai/keys)).

This package is replysieve's Rust core compiled to WebAssembly, for Cloudflare
Workers and Node 20 and later. The CLI, HTTP server, Docker image and MCP server
live in the [main repository](https://github.com/zeroonething/replysieve). It
returns the same verdict JSON as the CLI, with no replysieve server to run.

```sh
npm install @zeroonething/replysieve
```

In a Cloudflare Worker, keep the OpenRouter key in a secret
(`npx wrangler secret put OPENROUTER_API_KEY`, and in `.dev.vars` for
`wrangler dev`). wrangler bundles the WebAssembly for you:

```js
import { Checker } from "@zeroonething/replysieve";

export default {
  async fetch(request, env) {
    const { text, title } = await request.json();
    const checker = new Checker({ level: "medium" }, env.OPENROUTER_API_KEY);
    const verdict = await checker.check({ text, title });
    // { verdict: "approve", reason: "useful", confidence: 0.97, level: "medium",
    //   model: "typesafe/jev-1.14-20261014", latency_ms: 640 }
    return Response.json(verdict);
  },
};
```

In Node:

```js
import { Checker } from "@zeroonething/replysieve";

const checker = new Checker(
  { level: "heavy", preset: "product-review", rules: ["No links to other products"] },
  process.env.OPENROUTER_API_KEY,
);

const verdict = await checker.check(
  { text: "Does offline sync merge edits made on two phones?", title: "Taskly" },
  ["texts this author posted recently"], // optional: an identical comment is a duplicate
);
const verdicts = await checker.checkBatch(["first comment", { text: "second", title: "Taskly" }]);
const free = checker.precheck("Great product, love it!!"); // synchronous, no network; null if the model is needed
```

`new Checker(options, apiKey)` takes `level`, `preset`, `models`, `rules`,
`stock_phrases`, `config` (the text of a replysieve TOML config; the other fields
override it), `timeout_ms` per model (default 10000), `base_url` and `log`
(model fallbacks and timings to `console.log`, never comment text). A comment is
a string or `{ text, title?, description?, parent? }`. Bad options throw an
`Error` named `ReplysieveError`; when no model answers, the verdict is
`needs_review` with reason `model_unavailable`, never an approve. TypeScript
types are included.

Do not use it from browser JavaScript: the OpenRouter key would be visible to
every visitor. Call it from your server or Worker. Deno and Bun may work but are
not tested.

## License

MIT OR Apache-2.0.
