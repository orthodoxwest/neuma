// Smoke test for dist/neuma.mjs under Node: `node crates/neuma-wasm/test.mjs`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { init, Chant, DEFAULT_WEIGHTS, summarize } from "./dist/neuma.mjs";

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
// The mediant is the whole pause at its bar.
assert.deepEqual(t.pauses.slice(0, 2).map((p) => p.weight), [DEFAULT_WEIGHTS.mediant, 0]);
// Recitation doesn't run across a bar.
const runs = new Chant("(c4) a(h) (::) b(h) (::) c(h)", { initial: 0 }).layout(400).timeline.notes;
assert.ok(runs.every((n) => !n.recitation));

// A porrectus swash carries both of the notes it draws.
const porrectus = new Chant("(c4) a(hgh)", { initial: 0 });
const svg = porrectus.layout(400).svg;
for (const id of [0, 1, 2]) assert.match(svg, new RegExp(`data-note="[0-9 ]*\\b${id}\\b`));
assert.match(svg, /data-note="0 1"/);

// A freed Chant throws rather than reaching another score's handle.
porrectus.free();
assert.throws(() => porrectus.layout(400), /freed/);
assert.throws(() => porrectus.noteAt(0, 0), /freed/);
assert.equal(verse.layout(600, { weights: { note: null } }).timeline.notes[1].duration, 1);
verse.free();

// Hit testing finds the note whose box holds the point.
const n = notes[5];
assert.equal(chant.noteAt(n.x, n.y), n.id);
assert.equal(chant.noteAt(-50, -50), null);

chant.free();
assert.throws(() => chant.layout(400));
// Catalogue entries, from a Chant or straight from the source.
const puer = "name: Puer natus est;\noffice-part: Introitus;\nmode: 7;\n%%\n(c3) Pu(g)er(gh) na(h)tus(hi) est(h.) (,) no(h)bis(g) (::)";
const entry = summarize(puer);
assert.equal(entry.kind, "introit");
assert.equal(entry.mode.number, 7);
assert.equal(entry.incipit, "Puer natus est");
assert.deepEqual(new Chant(puer).summary, entry);
assert.equal(summarize("").notes, 0);
const own = summarize("name: Versicle;\noffice-part: R. br.;\nsource: Vespers, p. 7;\n%%\n(c4) A(g) (::)");
assert.equal(own.kind, "short-responsory");
assert.deepEqual(own.otherHeaders, [{ name: "source", value: "Vespers, p. 7" }]);

// A one-line preview keeps the first line as broken for the whole score.
const long = new Chant(puer.replace("(::)", "(;) " + "a(g) ".repeat(40) + "(::)"), { initial: 0 });
const full = long.layout(300);
const preview = long.layout(300, { maxLines: 1 });
assert.ok(full.timeline.lines.length > 1 && preview.timeline.lines.length === 1);
assert.ok(preview.height < full.height);
assert.deepEqual(preview.timeline.notes.map((n) => n.id), full.timeline.notes.filter((n) => n.line === 0).map((n) => n.id));
// The preview's timeline ends with its line: no pauses from the lines left out.
const lastNote = preview.timeline.notes.at(-1);
assert.ok(preview.timeline.pauses.every((p) => p.beforeNote <= lastNote.id + 1));
assert.ok(preview.timeline.duration < full.timeline.duration);

assert.equal(DEFAULT_WEIGHTS.note, 1);
console.log(`ok: ${notes.length} notes, ${wide.timeline.lines.length} lines at 900, ${narrow.timeline.lines.length} at 360`);
