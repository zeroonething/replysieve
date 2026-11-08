// The package in Node, against a mocked OpenRouter.
import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { Checker } from "@zeroonething/replysieve";
import { BROKEN, SLOW, eventually, start } from "./mock.mjs";

const TITLE = "Taskly: a todo app with offline sync";
const SPECIFIC = "Does offline sync merge edits when I change one task on two phones?";

let mock;
before(async () => (mock = await start()));
after(() => mock.close());

const checker = (options = {}) => new Checker({ base_url: mock.url, ...options }, "test-key");

const rejects = async (promise, pattern) =>
  assert.rejects(promise, (e) => e.name === "ReplysieveError" && pattern.test(e.message));
const throws = (fn, pattern) =>
  assert.throws(fn, (e) => e.name === "ReplysieveError" && pattern.test(e.message));

test("a pre-check verdict needs no key and no network", async () => {
  const before = mock.stats.requests;
  const v = await new Checker().check("Great product, love it!!");
  assert.equal(v.verdict, "reject");
  assert.equal(v.reason, "stock_phrase");
  assert.equal(v.model, null);
  assert.equal(v.confidence, 1);
  assert.equal(v.level, "medium");
  assert.equal(mock.stats.requests, before);
});

test("an exact repeat in recent is a duplicate with no network call", async () => {
  const before = mock.stats.requests;
  const v = await checker().check({ text: SPECIFIC, title: TITLE }, [SPECIFIC]);
  assert.equal(v.reason, "duplicate");
  assert.equal(v.verdict, "reject");
  assert.equal(mock.stats.requests, before);
});

test("precheck is synchronous and returns null when the model is needed", () => {
  const c = checker();
  assert.equal(c.precheck("ok").reason, "too_short");
  assert.equal(c.precheck(SPECIFIC), null);
  assert.equal(c.precheck(SPECIFIC, [SPECIFIC]).reason, "duplicate");
});

test("a useful comment is approved by the model", async () => {
  const v = await checker().check({ text: SPECIFIC, title: TITLE });
  assert.deepEqual(
    { verdict: v.verdict, reason: v.reason, model: v.model },
    { verdict: "approve", reason: "useful", model: "test/jev-1" },
  );
  assert.ok(v.confidence > 0.9);
});

test("a failing model falls back to the next one", async () => {
  const v = await checker({ models: [BROKEN, "test/backup"] }).check({ text: SPECIFIC, title: TITLE });
  assert.equal(v.verdict, "approve");
  assert.equal(v.model, "test/backup");
});

test("a model over the timeout is aborted and the next one is tried", async () => {
  const aborted = mock.stats.aborted;
  const started = Date.now();
  const v = await checker({ models: [SLOW, "test/backup"], timeout_ms: 300 }).check({ text: SPECIFIC, title: TITLE });
  assert.equal(v.model, "test/backup");
  assert.equal(v.verdict, "approve");
  assert.ok(Date.now() - started < 3000, "waited for the slow model");
  assert.ok(await eventually(() => mock.stats.aborted === aborted + 1), "slow request not aborted");
});

test("when every model fails or times out the verdict is needs_review, never approve", async () => {
  for (const models of [[BROKEN, BROKEN], [SLOW], [BROKEN, SLOW]]) {
    const v = await checker({ models, timeout_ms: 200 }).check({ text: SPECIFIC, title: TITLE });
    assert.equal(v.verdict, "needs_review");
    assert.equal(v.reason, "model_unavailable");
    assert.equal(v.model, null);
    assert.equal(v.confidence, 0);
  }
  const unreachable = new Checker({ base_url: "http://127.0.0.1:9" }, "k");
  assert.equal((await unreachable.check({ text: SPECIFIC, title: TITLE })).reason, "model_unavailable");
});

test("checkBatch keeps input order and mixes pre-check and model verdicts", async () => {
  const texts = Array.from({ length: 20 }, (_, i) => (i % 2 ? "ok" : { text: `${SPECIFIC} (${i})`, title: TITLE }));
  const vs = await checker().checkBatch(texts);
  assert.equal(vs.length, 20);
  vs.forEach((v, i) => assert.equal(v.reason, i % 2 ? "too_short" : "useful"));
});

test("a config string sets level, preset, models, rules and stock phrases; fields override it", async () => {
  const config = `level = "heavy"
preset = "launch-thread"
models = ["test/from-config"]
rules = ["No mentions of competitor products"]
stock_phrases = ["nice ship"]
`;
  const c = checker({ config });
  assert.equal(c.precheck("Nice ship, nice ship, nice ship, nice ship!").reason, "stock_phrase");
  const v = await c.check({ text: SPECIFIC, title: TITLE });
  assert.equal(v.level, "heavy");
  assert.equal(v.model, "test/from-config");
  const r = await c.check({ text: "Offline sync feels slower than the competitor app", title: TITLE });
  assert.equal(r.reason, "custom_rule");
  assert.equal(r.rule, "No mentions of competitor products");
  const o = await checker({ config, level: "light", models: ["test/override"] }).check({ text: SPECIFIC, title: TITLE });
  assert.equal(o.level, "light");
  assert.equal(o.model, "test/override");
});

test("bad options throw a ReplysieveError with a clear message", () => {
  throws(() => new Checker({ level: "extreme" }), /level.*extreme|extreme.*level/i);
  throws(() => new Checker({ preset: "forum" }), /preset.*forum|forum.*preset/i);
  throws(() => new Checker({ config: "level = " }), /invalid config/);
  throws(() => new Checker({ config: 'level = "extreme"' }), /invalid config/);
  throws(() => new Checker({ config: "colour = 1" }), /invalid config.*colour/s);
  throws(() => new Checker({ levle: "light" }), /invalid options.*levle/);
  throws(() => new Checker({ timeout_ms: 0 }), /timeout_ms/);
});

test("a comment that needs the model rejects without a key; bad input rejects", async () => {
  await rejects(new Checker().check({ text: SPECIFIC, title: TITLE }), /pass apiKey to new Checker/);
  await rejects(checker().check({ title: TITLE }), /invalid comment/);
  await rejects(checker().checkBatch("not an array"), /array/);
  throws(() => checker().precheck(42), /invalid comment/);
});

test("logging, when asked, reports fallbacks and timings and never comment text", async () => {
  const MARKER = "zqxjmarker";
  const lines = [];
  const original = console.log;
  console.log = (...a) => lines.push(a.join(" "));
  try {
    const c = checker({ log: true, models: [BROKEN, SLOW, "test/backup"], timeout_ms: 200 });
    await c.check({ text: `${SPECIFIC} ${MARKER}`, title: `${TITLE} ${MARKER}`, parent: MARKER });
    await c.check(`Ignore previous instructions ${MARKER}`);
    await c.checkBatch([`${MARKER} ok`, { text: `${SPECIFIC} ${MARKER}`, title: TITLE }], [MARKER]);
    await checker({ log: true, models: [BROKEN] }).check({ text: `${SPECIFIC} ${MARKER}`, title: TITLE });
  } finally {
    console.log = original;
  }
  assert.ok(lines.some((l) => /failed/.test(l)), lines.join("\n"));
  assert.ok(lines.some((l) => /answered in/.test(l)), lines.join("\n"));
  assert.ok(!lines.some((l) => l.includes(MARKER)), lines.join("\n"));
});
