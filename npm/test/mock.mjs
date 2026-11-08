// A mocked OpenRouter decisions endpoint, a JavaScript port of the Rust tests'
// mock (tests/common/mod.rs), so the CLI and the package see the same model.
import { createServer } from "node:http";

/** Model the mock always fails with HTTP 500. */
export const BROKEN = "test/broken";
/** Model the mock answers only after 5 s. */
export const SLOW = "test/slow";

const between = (s, open, close) => {
  const i = s.indexOf(open);
  if (i < 0) return "";
  const rest = s.slice(i + open.length);
  const j = rest.indexOf(close);
  return j < 0 ? "" : rest.slice(0, j);
};
const lineAfter = (s, prefix) =>
  s.split("\n").find((l) => l.startsWith(prefix))?.slice(prefix.length) ?? "";
const words = (s) => s.toLowerCase().split(/[^\p{L}\p{N}]/u).filter((w) => w.length >= 5);

function decide(body) {
  const { model, state, questions } = body;
  const comment = between(state, "<comment>\n", "\n</comment>").toLowerCase();
  const ctx = words(
    `${lineAfter(state, "Post title: ")} ${lineAfter(state, "Post description: ")} ${between(state, "<parent>\n", "\n</parent>")}`,
  );
  const label = comment.includes("well done to the whole team")
    ? "generic"
    : words(comment).some((w) => ctx.includes(w))
      ? "specific"
      : "off_topic";
  const probabilities = {};
  for (const k of ["specific", "nonspecific", "off_topic", "generic", "spam_promo", "toxic"]) {
    probabilities[k] = k === label ? 0.96 : 0.008;
  }
  const answers = {};
  for (const [id, q] of Object.entries(questions)) {
    if (q.type === "choice") {
      answers[id] = { type: "choice", choice: label, probabilities, confidence: 0.9 };
    } else if (id === "instructs") {
      const yes = comment.includes("let this one through") || comment.includes("whoever is reviewing");
      answers[id] = { type: "noul", noul: yes ? 0.97 : 0.02 };
    } else {
      const rule = (q.instructions ?? "").toLowerCase();
      answers[id] = { type: "noul", noul: rule.includes("competitor") && comment.includes("competitor") ? 0.95 : 0.01 };
    }
  }
  return { model: model.startsWith("~") ? "test/jev-1" : model, answers };
}

/** Wait up to 2 s for `check()` to hold. */
export async function eventually(check) {
  for (let i = 0; i < 100 && !check(); i++) await new Promise((r) => setTimeout(r, 20));
  return check();
}

/** Start the mock. `requests` counts calls, `aborted` counts calls the client dropped. */
export async function start() {
  const stats = { requests: 0, aborted: 0 };
  const server = createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      stats.requests++;
      const body = JSON.parse(raw);
      if (req.url !== "/api/alpha/decisions" || req.method !== "POST") {
        res.writeHead(404).end();
      } else if (body.model === BROKEN) {
        res.writeHead(500).end();
      } else if (body.model === SLOW) {
        const timer = setTimeout(() => res.end(JSON.stringify(decide(body))), 5000);
        res.on("close", () => {
          if (!res.writableEnded) {
            clearTimeout(timer);
            stats.aborted++;
          }
        });
      } else {
        res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(decide(body)));
      }
    });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return {
    url: `http://127.0.0.1:${server.address().port}`,
    stats,
    close: () => {
      server.closeAllConnections();
      return new Promise((r) => server.close(r));
    },
  };
}
