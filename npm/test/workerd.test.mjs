// The packed package inside workerd: `npm pack`, installed into a scratch
// project, bundled by wrangler and run by wrangler's local runtime, against a
// mocked OpenRouter. This is what `wrangler dev` does with a user's Worker.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { unstable_startWorker } from "wrangler";
import { BROKEN, SLOW, start } from "./mock.mjs";

const TITLE = "Taskly: a todo app with offline sync";
const SPECIFIC = "Does offline sync merge edits when I change one task on two phones?";
const PKG = fileURLToPath(new URL("..", import.meta.url));

const WORKER = `
import { Checker } from "@zeroonething/replysieve";

export default {
  async fetch(request, env) {
    const { op, options, comment, recent, n } = await request.json();
    try {
      const checker = new Checker(options, env.OPENROUTER_API_KEY);
      if (op === "precheck") return Response.json(checker.precheck(comment, recent));
      if (op === "noop") return Response.json(null);
      if (op === "bench") {
        // Each precheck timed on its own, in the isolate.
        const times = [];
        for (let i = 0; i < n; i++) {
          const t = performance.now();
          const v = checker.precheck(comment + " ".repeat(i % 3), recent);
          times.push(performance.now() - t);
          if (v?.reason !== "stock_phrase") throw new Error("unexpected verdict");
        }
        return Response.json({ times, clock: performance.now() });
      }
      if (op === "batch") return Response.json(await checker.checkBatch(comment, recent));
      return Response.json(await checker.check(comment, recent));
    } catch (e) {
      return Response.json({ error: e.name, message: e.message }, { status: 400 });
    }
  },
};
`;

let mock;
let worker;
before(async () => {
  mock = await start();
  const dir = mkdtempSync(join(tmpdir(), "replysieve-workerd-"));
  const tgz = execFileSync("npm", ["pack", "--pack-destination", dir, "--silent"], { cwd: PKG, encoding: "utf8" }).trim();
  writeFileSync(join(dir, "package.json"), JSON.stringify({ name: "worker", private: true, type: "module" }));
  execFileSync("npm", ["install", "--no-audit", "--no-fund", "--silent", join(dir, tgz)], { cwd: dir });
  writeFileSync(join(dir, "index.js"), WORKER);
  worker = await unstable_startWorker({
    name: "replysieve-test",
    entrypoint: join(dir, "index.js"),
    compatibilityDate: "2026-01-01",
    bindings: { OPENROUTER_API_KEY: { type: "plain_text", value: "test-key" } },
    dev: { server: { port: 0 }, inspector: false, logLevel: "none" },
  });
  await worker.ready;
});
after(async () => {
  await worker?.dispose();
  await mock?.close();
});

async function call(body) {
  const res = await worker.fetch("http://worker/", { method: "POST", body: JSON.stringify(body) });
  return res.json();
}

test("a pre-check verdict inside workerd", async () => {
  const v = await call({ op: "check", comment: "Great product, love it!!" });
  assert.equal(v.reason, "stock_phrase");
  assert.equal(v.model, null);
});

test("a model verdict, fallback and model_unavailable inside workerd", async () => {
  const base = { base_url: mock.url };
  const ok = await call({ op: "check", options: base, comment: { text: SPECIFIC, title: TITLE } });
  assert.equal(ok.verdict, "approve");
  assert.equal(ok.model, "test/jev-1");
  const fb = await call({ op: "check", options: { ...base, models: [BROKEN, "test/backup"] }, comment: { text: SPECIFIC, title: TITLE } });
  assert.equal(fb.model, "test/backup");
  const none = await call({ op: "check", options: { ...base, models: [BROKEN] }, comment: { text: SPECIFIC, title: TITLE } });
  assert.equal(none.verdict, "needs_review");
  assert.equal(none.reason, "model_unavailable");
  const slow = await call({ op: "check", options: { ...base, models: [SLOW, "test/backup"], timeout_ms: 300 }, comment: { text: SPECIFIC, title: TITLE } });
  assert.equal(slow.model, "test/backup");
});

test("a duplicate in recent needs no network call inside workerd", async () => {
  const before = mock.stats.requests;
  const v = await call({ op: "check", options: { base_url: mock.url }, comment: SPECIFIC, recent: [SPECIFIC] });
  assert.equal(v.reason, "duplicate");
  assert.equal(mock.stats.requests, before);
});

test("checkBatch and a typed error inside workerd", async () => {
  const vs = await call({ op: "batch", options: { base_url: mock.url }, comment: ["ok", { text: SPECIFIC, title: TITLE }] });
  assert.deepEqual(vs.map((v) => v.reason), ["too_short", "useful"]);
  const e = await call({ op: "check", options: { level: "extreme" }, comment: "hi" });
  assert.equal(e.error, "ReplysieveError");
  assert.match(e.message, /unknown level 'extreme'/);
});

test("precheck answers in under 5 ms inside workerd", async () => {
  const { times } = await call({ op: "bench", comment: "Great product, love it!!", n: 300 });
  times.sort((a, b) => a - b);
  const p50 = times[150];
  const p99 = times[Math.floor(times.length * 0.99)];
  console.log(`precheck in workerd: p50 ${p50.toFixed(3)} ms, p99 ${p99.toFixed(3)} ms over ${times.length}`);
  assert.ok(p99 < 5, `p99 ${p99} ms`);
});
