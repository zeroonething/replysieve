// Node entry: reads the .wasm next to this file once, at import.
import { readFileSync } from "node:fs";
import { initSync } from "./pkg/replysieve.js";

initSync({ module: readFileSync(new URL("./pkg/replysieve_bg.wasm", import.meta.url)) });

export { Checker } from "./pkg/replysieve.js";
