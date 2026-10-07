// Draws docs/images/pointed-psalm.png: the pointed psalm example
// (crates/neuma-wasm/examples/psalm.html) in headless Chromium. Build the browser package
// first, and have Playwright at hand:
//
//   cargo build -p neuma-wasm --target wasm32-unknown-unknown --profile wasm
//   node crates/neuma-wasm/build.mjs
//   node tools/readme/record-psalm.mjs [OUT.png]
//
// PLAYWRIGHT and FONTS_DIR are as for record-editor.mjs.

import { createServer } from "node:http";
import { readFileSync, existsSync } from "node:fs";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const out = process.argv[2] ?? join(root, "docs/images/pointed-psalm.png");
const { chromium } = await import(process.env.PLAYWRIGHT ?? "playwright");

const types = { ".html": "text/html", ".mjs": "text/javascript", ".js": "text/javascript", ".css": "text/css" };
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^(\.\.[/\\])+/, "");
  const file = join(root, path);
  if (!file.startsWith(root) || !existsSync(file)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(readFileSync(file));
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1100, height: 900 }, deviceScaleFactor: 2, colorScheme: "light" });
const fonts = process.env.FONTS_DIR;
if (fonts) {
  await page.route("https://fonts.googleapis.com/**", (r) =>
    r.fulfill({ contentType: "text/css", body: readFileSync(join(fonts, "css.css")) }));
  await page.route("https://fonts.gstatic.com/**", (r) => {
    const name = new URL(r.request().url()).pathname.slice(1).replaceAll("/", "_");
    r.fulfill({ contentType: "font/woff2", body: readFileSync(join(fonts, name)) });
  });
}
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
await page.goto(`http://127.0.0.1:${server.address().port}/crates/neuma-wasm/examples/psalm.html`);
await page.waitForFunction(() => window.neumaPsalm);
// The image shows the psalter's style alone, without the underlines for checking.
await page.addStyleTag({ content: ".unsure { text-decoration: none; }" });
await page.evaluate(() => document.fonts.ready.then(() => window.neumaPsalm.draw()));
await page.locator("#psalm").screenshot({ path: out });
await browser.close();
server.close();
if (errors.length) throw new Error(errors.join("\n"));
console.log(out);
