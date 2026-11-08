// Builds the WebAssembly package into npm/pkg: the library for wasm32 without
// the CLI, HTTP and MCP features, then wasm-bindgen's ES module glue.
// Needs the wasm32-unknown-unknown target and wasm-bindgen-cli at the same
// version as the wasm-bindgen crate in Cargo.lock.
import { execFileSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: "inherit" });

run("cargo", [
  "rustc", "--lib", "--release", "--locked", "--no-default-features",
  "--target", "wasm32-unknown-unknown", "--crate-type", "cdylib",
]);
run("wasm-bindgen", [
  "--target", "web", "--out-dir", "npm/pkg", "--out-name", "replysieve",
  "target/wasm32-unknown-unknown/release/replysieve.wasm",
]);

// npm packs only files under npm/, so the licences are copied in.
for (const f of ["LICENSE-APACHE", "LICENSE-MIT", "NOTICE"]) {
  copyFileSync(new URL(`../${f}`, import.meta.url), new URL(f, import.meta.url));
}
