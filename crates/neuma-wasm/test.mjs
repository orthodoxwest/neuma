// Smoke test for dist/neuma.mjs under Node: `node crates/neuma-wasm/test.mjs`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { init, Chant, DEFAULT_WEIGHTS, noteAtTime, point, psalm, summarize, toneNames } from "./dist/neuma.mjs";

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
// A playhead finds the note sounding at a time.
assert.equal(noteAtTime(wide.timeline, -1), null);
assert.equal(noteAtTime(wide.timeline, notes[3].start), notes[3]);
assert.equal(noteAtTime(wide.timeline, notes[3].start + notes[3].duration / 2), notes[3]);
assert.equal(noteAtTime(wide.timeline, 1e9), null);
const rest = wide.timeline.pauses.find((p) => p.weight > 0);
assert.equal(noteAtTime(wide.timeline, rest.start + rest.weight / 2), null, "silent in a pause");

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
assert.equal(chant.noteAt(n.cx, n.cy), n.id);
assert.equal(chant.noteAt(-50, -50), null);

chant.free();
assert.throws(() => chant.layout(400));
// Library entries, from a Chant or straight from the source.
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

// Psalm tones: pointed text to GABC, with each note's role.
const text = "1 The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength.\n" +
  "2 He hath made the round world so · súre, * that it can·not be móv-ed.";
const ps = psalm(text, "8.G");
assert.deepEqual(ps.diagnostics, []);
assert.ok(toneNames().includes("8.G"));
const psChant = new Chant(ps.gabc);
const psNotes = psChant.layout(500).timeline.notes;
assert.equal(psNotes.length, ps.notes.length);
assert.equal(ps.notes[0].role, "intonation");
const accent = ps.notes.findIndex((n) => n.role === "accent");
assert.equal(psNotes[accent].syllableText, "pá");
assert.equal(new TextDecoder().decode(new TextEncoder().encode(text).slice(ps.notes[accent].start, ps.notes[accent].end)), "pá");
assert.equal(text.slice(ps.notes[accent].utf16Start, ps.notes[accent].utf16End), "pá");
assert.ok(ps.notes.some((n) => n.verse === 1 && n.number === 2 && n.part === "termination"));
assert.throws(() => psalm(text, "9.z"), /no built-in tone/);
const ownTone = psalm(text, "name: mine\nclef: c4\nmediant: f g hr 'g hr h\ntermination: hr g f 'g hr h");
assert.ok(ownTone.gabc.includes("(c4)"));
assert.equal(psalm(text, "8.G\n").gabc, psalm(text, "8.G").gabc);
psChant.free();
// Plain text is pointed automatically, and `point` shows where.
const plain = "O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.";
const pt = point(plain, "8.G");
assert.equal(pt.halves.length, 2);
assert.ok(pt.halves.every((h) => !h.kept && h.confidence > 0 && h.confidence <= 1));
assert.ok(pt.text.includes("·") && /[áéíóú]/.test(pt.text));
assert.equal(psalm(plain, "8.G").gabc, psalm(pt.text, "8.G").gabc);
assert.ok(psalm(plain, "8.G", { autoPoint: false }).diagnostics.some((d) => d.code === "apply::no-accent"));

// Editors: offsets in UTF-16 alongside bytes, fixes, updates, and both ways between source
// and score.
const src = "(c4) Ký-(g)ri(hi) (,) é(h) (::)";
const ed = new Chant(src, { initial: 0 });
const hyphen = ed.diagnostics.find((d) => d.code === "gabc::hyphen-in-syllable");
assert.equal(src.slice(hyphen.utf16Start, hyphen.utf16End), "-");
assert.ok(hyphen.end - hyphen.start === 1 && hyphen.start === hyphen.utf16Start + 1, "bytes count the é");
assert.equal(hyphen.fix.replacement, "");
const fixed = src.slice(0, hyphen.fix.utf16Start) + hyphen.fix.replacement + src.slice(hyphen.fix.utf16End);
ed.update(fixed);
assert.deepEqual(ed.diagnostics, []);
assert.equal(new Chant("Ky(g)", { initial: 0 }).diagnostics[0].fix.replacement, "(c4) ");

const quick = ed.layout(500, { timeline: false });
assert.equal(quick.timeline, undefined);
assert.ok(quick.svg.startsWith("<svg"));
const page = ed.layout(500);
for (const n of page.timeline.notes) {
  const hit = ed.sourceAt(n.cx, n.cy);
  assert.equal(hit.kind, "note");
  assert.equal(hit.index, n.id);
}
const hi = fixed.indexOf("hi");
const at = ed.elementsAt(hi + 1);
assert.deepEqual(at.map((e) => e.kind), ["note", "syllable"]);
assert.equal(fixed.slice(at[0].utf16Start, at[0].utf16End), "i");
assert.equal(fixed.slice(at[1].utf16Start, at[1].utf16End), "ri(hi)");
const bar = ed.elementsAt(fixed.indexOf(","));
assert.equal(bar[0].kind, "bar");
assert.equal(ed.sourceAt(bar[0].x + bar[0].w / 2, bar[0].y + 1).kind, "bar");
// The same caret as a byte offset.
const bytes = new TextEncoder().encode(fixed.slice(0, hi + 1)).length;
assert.deepEqual(ed.elementsAt(bytes, { unit: "utf8" }), at);
assert.equal(ed.sourceAt(-100, -100), null);
// Carets past either end are at it, however far: none wraps around to the start.
const atEnd = ed.elementsAt(fixed.length);
for (const far of [fixed.length + 1, 2 ** 32, 2 ** 32 + 5, 2 ** 53, Infinity]) {
  assert.deepEqual(ed.elementsAt(far), atEnd, String(far));
  if (far > 2 ** 31) assert.deepEqual(ed.elementsAt(far, { unit: "utf8" }), atEnd, String(far));
}
for (const before of [-1, -(2 ** 32), -Infinity, NaN]) {
  assert.deepEqual(ed.elementsAt(before), ed.elementsAt(0), String(before));
}

// The SVG a line at a time, without ids, draws what the whole SVG draws.
const parts = ed.layout(500, { svg: "lines", ids: false, timeline: false }).svgParts;
assert.ok(parts.head.startsWith("<svg") && parts.defs.startsWith("<path"));
assert.equal(parts.lines.length, page.timeline.lines.length);
const count = (text, tag) => text.split(tag).length - 1;
const lineSvg = parts.lines.map((l) => l.svg).join("");
assert.equal(count(lineSvg, "<use ") + count(parts.rest, "<use "), count(page.svg, "<use "));
assert.ok(!lineSvg.includes("data-note"));
assert.ok(parts.lines.every((l, i) => i === 0 || l.top > parts.lines[i - 1].top));
// Without the timeline, notes are still found under a point.
const first = page.timeline.notes[0];
assert.equal(ed.noteAt(first.cx, first.cy), first.id);
// Edits one after another: the lines the engine kept, and those it made again, are what a
// fresh Chant draws, with ids or without.
let edited = fixed;
for (let i = 0; i < 12; i++) {
  const at = edited.indexOf("(", (i * 37) % edited.length);
  edited = i % 3 === 2 ? edited.slice(0, at) + edited.slice(at + 3) : edited.slice(0, at) + "a" + edited.slice(at);
  ed.update(edited);
  const opts = { svg: "lines", ids: i % 4 === 3, timeline: false };
  const fresh = new Chant(edited, { initial: 0 });
  assert.deepEqual(ed.layout(200, opts), fresh.layout(200, opts), `edit ${i}`);
  fresh.free();
}
ed.free();
assert.throws(() => ed.update(src), /freed/);
assert.throws(() => ed.sourceAt(0, 0), /freed/);

// A RangeError from the caller's own arguments, before the engine runs, is rethrown and
// leaves the engine and its Chants alive.
{
  const live = new Chant("(c4) a(g)", { initial: 0 });
  const bad = { toString: () => (1).toFixed(500) };
  assert.throws(() => summarize(bad), RangeError);
  assert.throws(() => psalm("a * b", bad), RangeError);
  assert.throws(() => live.layout(400, { prefix: bad }), RangeError);
  assert.equal(summarize("(c4) a(g)").notes, 1);
  assert.ok(live.layout(400).svg.startsWith("<svg"));
  live.free();
}

// A trap or a stack overflow (a RangeError, not a trap) drops the engine until init() runs
// again. A fresh copy of the glue runs a stand-in module whose `neuma_summarize` recurses
// forever and whose `neuma_tone_names` traps.
{
  const glue = await import("./dist/neuma.mjs?crash");
  const section = (id, body) => [id, body.length, ...body];
  const fn = (code) => [code.length + 2, 0, ...code, 0x0b]; // size, no locals, code, end
  const exp = (name, kind, index) => [name.length, ...new TextEncoder().encode(name), kind, index];
  const exports = [
    exp("memory", 2, 0), exp("neuma_input", 0, 1), exp("neuma_output_ptr", 0, 2),
    exp("neuma_output_len", 0, 2), exp("neuma_summarize", 0, 0), exp("neuma_tone_names", 0, 3),
  ];
  const bytes = new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    // Types: () -> (), (i32) -> i32, () -> i32.
    ...section(1, [3, 0x60, 0, 0, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f]),
    ...section(3, [4, 0, 1, 2, 0]),
    ...section(5, [1, 0, 1]),
    ...section(7, [exports.length, ...exports.flat()]),
    ...section(10, [4, ...fn([0x10, 0]), ...fn([0x41, 0]), ...fn([0x41, 0]), ...fn([0x00])]),
  ]);
  glue.initSync(bytes);
  assert.throws(() => glue.summarize("x"), RangeError);
  assert.throws(() => glue.summarize("x"), /stopped on an internal error; call init\(\) again/);
  await glue.init();
  assert.throws(() => glue.toneNames(), WebAssembly.RuntimeError);
  assert.throws(() => glue.toneNames(), /call init\(\) again/);
  await glue.init();
  assert.throws(() => glue.summarize("x"), RangeError);
}

assert.equal(DEFAULT_WEIGHTS.note, 1);
console.log(`ok: ${notes.length} notes, ${wide.timeline.lines.length} lines at 900, ${narrow.timeline.lines.length} at 360`);
