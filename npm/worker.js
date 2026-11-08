// Cloudflare Workers entry: wrangler bundles the .wasm as a compiled module.
import wasm from "./pkg/replysieve_bg.wasm";
import { initSync } from "./pkg/replysieve.js";

initSync({ module: wasm });

export { Checker } from "./pkg/replysieve.js";
