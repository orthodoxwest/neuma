// Records docs/images/editor.gif: the example editor (crates/neuma-wasm/examples/editor.html)
// in headless Chromium while GABC is typed, a diagnostic's fix is applied and a note is
// clicked. Build the browser package first, and have Playwright and ffmpeg at hand:
//
//   cargo build -p neuma-wasm --target wasm32-unknown-unknown --profile wasm
//   node crates/neuma-wasm/build.mjs
//   node tools/readme/record-editor.mjs [OUT.gif]
//
// PLAYWRIGHT names the playwright module to import (default "playwright"). The editor loads
// EB Garamond from Google Fonts; FONTS_DIR, a directory holding the stylesheet as css.css and
// the font files named by their path on fonts.gstatic.com with "/" as "_", serves them
// locally instead, for machines that can't reach Google. The timing readout is hidden, as
// it depends on the machine.

import { createServer } from "node:http";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const out = process.argv[2] ?? join(root, "docs/images/editor.gif");
const { chromium } = await import(process.env.PLAYWRIGHT ?? "playwright");

const types = { ".html": "text/html", ".mjs": "text/javascript", ".js": "text/javascript", ".css": "text/css" };
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^(\.\.[/\\])+/, "");
  const file = join(root, path);
  if (!file.startsWith(root) || !existsSync(file)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(readFileSync(file));
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 960, height: 340 } });
const fonts = process.env.FONTS_DIR;
if (fonts) {
  await page.route("https://fonts.googleapis.com/**", (r) =>
    r.fulfill({ contentType: "text/css", body: readFileSync(join(fonts, "css.css")) }));
  await page.route("https://fonts.gstatic.com/**", (r) => {
    const name = new URL(r.request().url()).pathname.slice(1).replaceAll("/", "_");
    r.fulfill({ contentType: "font/woff2", body: readFileSync(join(fonts, name)) });
  });
}
await page.goto(`${base}/crates/neuma-wasm/examples/editor.html`);
await page.waitForFunction(() => window.neumaEditor);
await page.evaluate(() => document.fonts.ready);

// A drawn pointer, since screenshots leave the real one out.
await page.addStyleTag({ content: `
  #stats { visibility: hidden; }
  #fake-pointer { position: fixed; z-index: 99; width: 18px; height: 18px; pointer-events: none;
    left: -40px; top: -40px; transition: none; }
` });
await page.evaluate(() => {
  const p = document.createElement("div");
  p.id = "fake-pointer";
  p.innerHTML = `<svg viewBox="0 0 18 18" width="18" height="18"><path d="M2 1 L2 15 L5.6 11.6 L8.2 17 L10.6 16 L8 10.6 L13 10.6 Z" fill="#111" stroke="#fff" stroke-width="1.2" stroke-linejoin="round"/></svg>`;
  document.body.append(p);
});

const frames = [];
const dir = mkdtempSync(join(tmpdir(), "neuma-editor-"));
const settle = () => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
async function frame(ms) {
  await settle();
  const file = join(dir, `f${String(frames.length).padStart(4, "0")}.png`);
  await page.screenshot({ path: file });
  frames.push({ file, ms });
}
async function setSource(src) {
  await page.evaluate((src) => {
    const t = window.neumaEditor.text;
    t.value = src;
    t.focus();
    t.setSelectionRange(src.length, src.length);
    t.scrollTop = t.scrollHeight;
    t.dispatchEvent(new Event("input"));
  }, src);
}
async function type(text, ms = 70) {
  for (const ch of text) {
    await page.keyboard.type(ch);
    await frame(ms);
  }
}
async function pointAt(x, y, steps = 8) {
  const from = await page.evaluate(() => {
    const p = document.getElementById("fake-pointer");
    return [parseFloat(p.style.left) || 700, parseFloat(p.style.top) || 380];
  });
  for (let i = 1; i <= steps; i++) {
    const k = i / steps, e = k * k * (3 - 2 * k);
    await page.evaluate(([x, y]) => {
      const p = document.getElementById("fake-pointer");
      p.style.left = `${x}px`; p.style.top = `${y}px`;
    }, [from[0] + (x - from[0]) * e, from[1] + (y - from[1]) * e]);
    await frame(40);
  }
}

// 1. Typing: the score follows each keystroke, underlining the source while a neume is open.
await setSource("name: Puer natus est;\nmode: 7;\n%%\n(c3) PU(ei)ER(i) *() na(iji)tus(h) est(hhh) no(ih/ji)bis,(i) (;)");
await frame(900);
await type(" et(ei~) fí(iji)li(hg)us(f) da(hhi)tus(h) est(h) no(hihh)bis:(efe) (:) cu(e)jus(f) im(h)pé(gi!jk", 60);
await frame(900);

// 2. A diagnostic with a fix: apply it from the problems list.
const fix = page.locator("#problems button").first();
const fb = await fix.boundingBox();
await pointAt(fb.x + fb.width / 2, fb.y + fb.height / 2 + 2);
await frame(400);
await fix.click();
await frame(1100);

// 3. Click a note in the score: its source is selected and the note highlighted.
const target = await page.evaluate(() => {
  const host = document.getElementById("host").getBoundingClientRect();
  // The note under the third syllable of the first line: "na".
  for (let x = 120; x < host.width; x += 2) {
    for (let y = 0; y < 80; y += 2) {
      const hit = window.neumaEditor.page.sourceAt(x, y);
      if (hit && hit.kind === "note" && hit.index === 3) return { x: host.left + hit.x + hit.w / 2, y: host.top + hit.y + hit.h / 2 };
    }
  }
  return null;
});
if (target) {
  await pointAt(target.x - 4, target.y - 2, 12);
  await page.mouse.click(target.x, target.y);
  await frame(1800);
}
await browser.close();
server.close();

// Assemble: one shared palette, frames held for their durations.
const list = frames.map((f) => `file '${f.file}'\nduration ${(f.ms / 1000).toFixed(3)}`).join("\n") +
  `\nfile '${frames.at(-1).file}'\n`;
writeFileSync(join(dir, "frames.txt"), list);
execFileSync("ffmpeg", ["-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", join(dir, "frames.txt"),
  "-vf", "split[a][b];[a]palettegen=max_colors=96:stats_mode=full[p];[b][p]paletteuse=dither=none:diff_mode=rectangle",
  "-loop", "0", out]);
rmSync(dir, { recursive: true });
console.log(`${out}: ${frames.length} frames`);
