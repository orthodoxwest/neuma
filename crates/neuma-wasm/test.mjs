// Smoke test for dist/neuma.mjs under Node: `node crates/neuma-wasm/test.mjs`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { init, Chant, DEFAULT_WEIGHTS } from "./dist/neuma.mjs";

await init();
const gabc = readFileSync(new URL("../neuma/tests/corpus/psalm-134.gabc", import.meta.url), "utf8");
const chant = new Chant(gabc);
assert.ok(Array.isArray(chant.diagnostics));

const narrow = chant.layout(360);
const wide = chant.layout(900, { weights: { mediant: 3, full: 2.5 } });
assert.ok(narrow.svg.startsWith("<svg") && narrow.svg.includes('data-note="0"'));
assert.ok(narrow.timeline.lines.length > wide.timeline.lines.length);

// The same notes, by id, at both widths.
const ids = (p) => p.timeline.notes.map((n) => n.id).join(",");
assert.equal(ids(narrow), ids(wide));

// Times run forward, and the caller's weights apply.
const notes = wide.timeline.notes;
for (let i = 1; i < notes.length; i++) assert.ok(notes[i].start >= notes[i - 1].start + notes[i - 1].duration - 1e-3);
for (const p of wide.timeline.pauses) {
  if (p.kind === "mediant") assert.equal(p.weight, 3);
  if (p.kind === "full") assert.equal(p.weight, 2.5);
}
assert.ok(wide.timeline.pauses.some((p) => p.kind === "half"));

// Psalm marks pause by kind, and the half-verse counter turns at the mediant.
const verse = new Chant("(c4) Di(h)xit(h) Dó(h)mi(h)nus(h) *(:) Dó(h)mi(g)no(h) me(h)o(g) †(,) se(g)de(h) (::)", { initial: 0 });
const t = verse.layout(600).timeline;
assert.deepEqual(t.pauses.map((p) => p.kind), ["mediant", "full", "flex", "quarter", "double"]);
assert.deepEqual(t.notes.map((n) => n.half).join(""), "000001111111");
assert.ok(t.notes[0].recitation && t.notes[2].accent);
verse.free();

// Hit testing finds the note whose box holds the point.
const n = notes[5];
assert.equal(chant.noteAt(n.x, n.y), n.id);
assert.equal(chant.noteAt(-50, -50), null);

chant.free();
assert.throws(() => chant.layout(400));
assert.equal(DEFAULT_WEIGHTS.note, 1);
console.log(`ok: ${notes.length} notes, ${wide.timeline.lines.length} lines at 900, ${narrow.timeline.lines.length} at 360`);
