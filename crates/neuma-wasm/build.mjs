// Builds dist/neuma.mjs: the wasm module, optimized and gzipped, inlined into the glue; and
// dist/neuma-external.mjs, the glue alone, which fetches dist/neuma.wasm.
//
//   cargo build -p neuma-wasm --target wasm32-unknown-unknown --profile wasm
//   node crates/neuma-wasm/build.mjs [path/to/wasm-opt]
//
// Without wasm-opt the module is inlined as cargo built it (about 10% larger).

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const built = join(root, "target/wasm32-unknown-unknown/wasm/neuma_wasm.wasm");
const dist = join(here, "dist");
mkdirSync(dist, { recursive: true });

let wasmPath = built;
const wasmOpt = process.argv[2];
if (wasmOpt) {
  wasmPath = join(dist, "neuma.wasm");
  execFileSync(wasmOpt, [
    "-Oz", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext",
    "--enable-mutable-globals", built, "-o", wasmPath,
  ]);
} else if (existsSync(built)) {
  writeFileSync(join(dist, "neuma.wasm"), readFileSync(built));
}
const wasm = readFileSync(wasmPath);
const packed = gzipSync(wasm, { level: 9 }).toString("base64");
const glue = readFileSync(join(here, "js/neuma.mjs"), "utf8");
const marker = '/*__NEUMA_WASM__*/\nconst WASM_GZIP_BASE64 = "";\nconst wasmUrl = () => null;';
if (!glue.includes(marker)) throw new Error("marker missing from js/neuma.mjs");
// One self-contained module, the wasm inlined; and one that fetches neuma.wasm beside it.
writeFileSync(join(dist, "neuma.mjs"), glue.replace(marker, `const WASM_GZIP_BASE64 = "${packed}";\nconst wasmUrl = () => null;`));
writeFileSync(
  join(dist, "neuma-external.mjs"),
  glue.replace(marker, 'const WASM_GZIP_BASE64 = "";\nconst wasmUrl = () => new URL("./neuma.wasm", import.meta.url);'),
);
const types = readFileSync(join(here, "js/neuma.d.mts"), "utf8");
writeFileSync(join(dist, "neuma.d.mts"), types);
writeFileSync(join(dist, "neuma-external.d.mts"), types);
console.log(`dist/neuma.mjs: ${(packed.length / 1024).toFixed(0)} KB of inlined wasm (${(wasm.length / 1024).toFixed(0)} KB raw, ${(gzipSync(wasm, { level: 9 }).length / 1024).toFixed(0)} KB gzipped)`);
