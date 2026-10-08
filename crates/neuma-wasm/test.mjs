// Smoke test for dist/neuma.mjs under Node: `node crates/neuma-wasm/test.mjs`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { init, Chant, DEFAULT_WEIGHTS, Page, engineStats, noteAtTime, point, psalm, psalmDisplay, setLayoutBudget, summarize, toneLabel, toneNames } from "./dist/neuma.mjs";

await init();
const gabc = readFileSync(new URL("../neuma/tests/corpus/psalm-134.gabc", import.meta.url), "utf8");
const chant = new Chant(gabc);
assert.ok(Array.isArray(chant.diagnostics));

const narrow = chant.layout(360);
const wide = chant.layout(900, { weights: { mediant: 3, full: 2.5 } });
assert.ok(narrow.svg.startsWith("<svg") && narrow.svg.includes('data-note="0"'));
assert.ok(narrow.timeline().lines.length > wide.timeline().lines.length);

// The same notes, by id, at both widths.
const ids = (p) => p.timeline().notes.map((n) => n.id).join(",");
assert.equal(ids(narrow), ids(wide));

// Times run forward, and the caller's weights apply.
const notes = wide.timeline().notes;
for (let i = 1; i < notes.length; i++) assert.ok(notes[i].start >= notes[i - 1].start + notes[i - 1].duration - 1e-3);
for (const p of wide.timeline().pauses) {
  if (p.kind === "mediant") assert.equal(p.duration, 3);
  if (p.kind === "full") assert.equal(p.duration, 2.5);
}
assert.ok(wide.timeline().pauses.some((p) => p.kind === "half"));
// A playhead finds the note sounding at a time.
assert.equal(noteAtTime(wide.timeline(), -1), null);
assert.equal(noteAtTime(wide.timeline(), notes[3].start), notes[3]);
assert.equal(noteAtTime(wide.timeline(), notes[3].start + notes[3].duration / 2), notes[3]);
assert.equal(noteAtTime(wide.timeline(), 1e9), null);
const rest = wide.timeline().pauses.find((p) => p.duration > 0);
assert.equal(noteAtTime(wide.timeline(), rest.start + rest.duration / 2), null, "silent in a pause");

// Psalm marks pause by kind, and the half-verse counter turns at the mediant.
const verse = new Chant("(c4) Di(h)xit(h) Dó(h)mi(h)nus(h) *(:) Dó(h)mi(g)no(h) me(h)o(g) †(,) se(g)de(h) (::)", { initial: 0 });
const t = verse.layout(600).timeline();
assert.deepEqual(t.pauses.map((p) => p.kind), ["mediant", "full", "flex", "quarter", "double"]);
assert.deepEqual(t.notes.map((n) => n.half).join(""), "000001111111");
assert.ok(t.notes[0].recitation && t.notes[2].accent);
// The mediant is the whole pause at its bar.
assert.deepEqual(t.pauses.slice(0, 2).map((p) => p.duration), [DEFAULT_WEIGHTS.mediant, 0]);
// Recitation doesn't run across a bar.
const runs = new Chant("(c4) a(h) (::) b(h) (::) c(h)", { initial: 0 }).layout(400).timeline().notes;
assert.ok(runs.every((n) => !n.recitation));

// A porrectus swash carries both of the notes it draws.
const porrectus = new Chant("(c4) a(hgh)", { initial: 0 });
const porrectusPage = porrectus.layout(400);
const svg = porrectusPage.svg;
for (const id of [0, 1, 2]) assert.match(svg, new RegExp(`data-note="[0-9 ]*\\b${id}\\b`));
assert.match(svg, /data-note="0 1"/);

// A freed Chant's pages keep answering; the chant itself engraves again when used, as the
// same state, so its pages stay current.
const swash = porrectusPage.timeline().notes[1];
const porrectusVersion = porrectus.version;
porrectus.free();
assert.equal(porrectusPage.noteAt(swash.cx, swash.cy), 1);
assert.equal(porrectus.layout(400).svg, svg);
assert.ok(porrectus.version === porrectusVersion && !porrectusPage.stale);
assert.equal(porrectus.update("(c4) a(hgh)"), false);
porrectus.free();
porrectus.free();
// A freed page lays itself out again from its source, even with its chant gone; freeing
// twice does nothing.
const porrectusHits = [porrectusPage.noteAt(swash.cx, swash.cy), porrectusPage.sourceAt(swash.cx, swash.cy), porrectusPage.elementsAt(8)];
porrectusPage.free();
porrectusPage.free();
assert.deepEqual([porrectusPage.noteAt(swash.cx, swash.cy), porrectusPage.sourceAt(swash.cx, swash.cy), porrectusPage.elementsAt(8)], porrectusHits);
assert.equal(porrectusPage.timeline().notes[1], swash);
assert.ok(porrectusPage.svg.startsWith("<svg"));
assert.equal(verse.layout(600, { weights: { note: null } }).timeline().notes[1].duration, 1);
verse.free();

// Hit testing finds the note whose box holds the point, on each page whatever was laid out
// since: here a one-line thumbnail that leaves the note out.
const n = notes.at(-1);
const thumb = chant.layout(900, { maxLines: 1 });
assert.ok(!thumb.timeline().notes.some((m) => m.id === n.id));
assert.equal(wide.noteAt(n.cx, n.cy), n.id);
assert.equal(wide.noteAt(-50, -50), null);
assert.ok(wide instanceof Page);
// `chant.layout` gives a new page each call, so freeing one leaves the other; a page's
// timeline is made once.
const one = chant.layout(900), two = chant.layout(900);
assert.notEqual(one, two);
two.free();
assert.equal(one.noteAt(n.cx, n.cy), n.id);
one.free();
assert.equal(wide.timeline(), wide.timeline());

chant.free();
assert.equal(chant.layout(900).noteAt(n.cx, n.cy), n.id);
chant.free();
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
assert.ok(full.timeline().lines.length > 1 && preview.timeline().lines.length === 1);
assert.ok(preview.height < full.height);
assert.deepEqual(preview.timeline().notes.map((n) => n.id), full.timeline().notes.filter((n) => n.line === 0).map((n) => n.id));
// The preview's timeline ends with its line: no pauses from the lines left out.
const lastNote = preview.timeline().notes.at(-1);
assert.ok(preview.timeline().pauses.every((p) => p.beforeNote <= lastNote.id + 1));
assert.ok(preview.timeline().duration < full.timeline().duration);

// Psalm tones: pointed text to GABC, with each note's role.
const text = "1 The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength.\n" +
  "2 He hath made the round world so · súre, * that it can·not be móv-ed.";
const ps = psalm(text, "8.G");
assert.deepEqual(ps.diagnostics, []);
assert.ok(toneNames().includes("8.G"));
const psChant = new Chant(ps.gabc);
const psNotes = psChant.layout(500).timeline().notes;
assert.equal(psNotes.length, ps.notes.length);
assert.equal(ps.notes[0].role, "intonation");
const accent = ps.notes.findIndex((n) => n.role === "accent");
assert.equal(psNotes[accent].syllableText, "pá");
const sung = ps.notes[accent];
assert.equal(new TextDecoder().decode(new TextEncoder().encode(text).slice(sung.sourceStart, sung.sourceEnd)), "pá");
assert.equal(text.slice(sung.sourceUtf16Start, sung.sourceUtf16End), "pá");
// Engraved from the psalm, the chant's sources are in the text.
const fromPsalm = Chant.fromPsalm(text, "8.G", { initial: 0 });
assert.deepEqual(fromPsalm.psalm, ps);
const psPage = fromPsalm.layout(500);
const psNote = psPage.timeline().notes[accent];
assert.equal(text.slice(psNote.sourceUtf16Start, psNote.sourceUtf16End), "pá");
assert.equal(text.slice(...(({ utf16Start, utf16End }) => [utf16Start, utf16End])(psPage.sourceAt(psNote.cx, psNote.cy))), "pá");
// After edits the page on screen is stale, but answers for the text it shows.
assert.ok(!psPage.stale);
assert.equal(fromPsalm.update(text.replace("glorious", "great")), true);
assert.ok(fromPsalm.psalm.gabc.includes("great"));
assert.ok(psPage.stale);
assert.equal(fromPsalm.update(text.replace("glorious", "great")), false);
for (const word of ["grand", "glad", "good"]) fromPsalm.update(text.replace("glorious", word));
assert.ok(psPage.stale);
assert.equal(psPage.noteAt(psNote.cx, psNote.cy), accent);
assert.equal(text.slice(psPage.sourceAt(psNote.cx, psNote.cy).utf16Start, psNote.sourceUtf16End), "pá");
assert.equal(text.slice(...(({ utf16Start, utf16End }) => [utf16Start, utf16End])(psPage.elementsAt(psNote.sourceUtf16Start)[0])), "pá");
assert.equal(psPage.timeline().notes[accent].id, accent);
assert.ok(!fromPsalm.layout(500).stale);
assert.ok(Chant.fromPsalm("Lord ! * God ?", "1.D").diagnostics.some((d) => d.code === "point::unsure"));
assert.throws(() => Chant.fromPsalm(text, "9.z"), /no built-in tone/);
fromPsalm.free();
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

// A pointed psalter's display: the tone once, as a line of notes, and the verses as styled
// runs of text, with each syllable's place in the text (both units) and in the tone.
const tone = Chant.fromTone("8.G");
assert.equal(tone.source, "(c4) (g) (h) (j) (k) (j) (j) *(:) (j) (i) (j) (h) (g) (g) (::)");
assert.equal(tone.layout(400).timeline().notes.length, 12);
assert.throws(() => Chant.fromTone("9.z"), /no built-in tone/);
assert.ok(Chant.fromTone("name: mine\nclef: c4\nmediant: f g hr 'g hr h\ntermination: hr g f 'g hr h").source.startsWith("(c4) (f) (g) (h)"));
const verses = "1 Wash me thoróughly · from my wíckedness, † and cleanse me from my sín. * [Sit.] For I ac·knowledge my fáults.\n2 " + plain;
const shown = psalmDisplay(verses, "8.G");
assert.deepEqual(shown.verses.map((v) => v.number), [1, 2]);
assert.equal(shown.toneLabel, "Tone 8 G");
assert.equal(psalmDisplay(plain, "per").toneLabel, "Tonus peregrinus");
assert.equal(toneLabel("1.D2"), "Tone 1 D2");
assert.equal(toneLabel("name: mine\nclef: c4\nmediant: f g hr 'g hr h\ntermination: hr g f 'g hr h"), "mine");
assert.throws(() => toneLabel("9.z"), /no built-in tone/);
// A line never breaks between a mark and its syllable: those spaces are U+00A0.
assert.equal(shown.verses[0].runs.map((r) => r.text).join(""), "Wash me thoróughly ·\u00a0from my wíckedness,\u00a0† and cleanse me from my sín.\u00a0* Sit. For I ac·knowledge my fáults.");
assert.deepEqual(shown.verses[0].runs.filter((r) => r.kind !== "text" && r.kind !== "syllable").map((r) => r.kind), ["point", "flex", "mediant", "rubric", "point"]);
const syls = shown.verses[0].runs.filter((r) => r.kind === "syllable");
for (const r of syls) assert.equal(verses.slice(r.sourceUtf16Start, r.sourceUtf16End), r.text);
assert.deepEqual(syls.filter((r) => r.flexDrop).map((r) => r.text), ["ed", "ness,"]);
assert.ok(syls.find((r) => r.text === "fáults.").accent);
assert.equal(syls[0].role, "intonation");
assert.deepEqual(psalmDisplay(plain, "8.G").diagnostics, psalm(plain, "8.G").diagnostics);
assert.ok(psalmDisplay(plain, "8.G").verses[0].runs.some((r) => r.kind === "point"));
assert.ok(!psalmDisplay(plain, "8.G", { autoPoint: false }).verses[0].runs.some((r) => r.kind === "point"));
assert.throws(() => psalmDisplay(plain, "9.z"), /no built-in tone/);
// Acutes printed: a pointed psalter prints none in a flex.
const outside = psalmDisplay(verses, "8.G", { accents: "outsideFlex" }).verses[0].runs.map((r) => r.text).join("");
assert.ok(outside.startsWith("Wash me thoroughly ·\u00a0from my wickedness,\u00a0† and cleanse me from my sín."), outside);
assert.ok(!/[áéíóú]/.test(psalmDisplay(verses, "8.G", { accents: "none", intone: "every" }).verses[0].runs.map((r) => r.text).join("")));
assert.ok(!/[áéíóú]/.test(psalm(verses, "8.G", { accents: "none" }).gabc));
assert.ok(!/[áéíóú]/.test(Chant.fromPsalm(verses, "8.G", { accents: "none" }).psalm.gabc));
assert.equal(psalm(verses, "8.G", { accents: "none" }).notes.length, psalm(verses, "8.G").notes.length);
// A spelling hyphen is printed, and each piece has its own source.
const spelled = psalmDisplay("Deliver me from blood\\-guiltiness, O God * thou that art the God of my health.", "8.G");
// A spelling hyphen is "-" and a word joiner, so a line never breaks at it, as is each held "–".
assert.ok(spelled.verses[0].runs.map((r) => r.text).join("").startsWith("Deliver me from blood-\u2060guiltiness,"));
assert.ok(psalmDisplay("Lord, remember · Dávid, – – * and all his trouble.", "8.G").verses[0].runs.some((r) => r.kind === "held" && r.text === "–\u2060"));
// Misspelled options throw rather than fall back.
assert.throws(() => psalmDisplay(plain, "8.G", { accents: "outside-flex" }), /accents must be/);
assert.throws(() => psalm(plain, "8.G", { intone: "always" }), /intone must be/);
assert.throws(() => Chant.fromPsalm(plain, "8.G", { accents: "None" }), /accents must be/);
// A null option is its default, as an undefined one is.
assert.deepEqual(psalm(plain, "8.G", { intone: null, accents: null, autoPoint: null }), psalm(plain, "8.G"));
assert.deepEqual(psalmDisplay(plain, "8.G", null), psalmDisplay(plain, "8.G"));
assert.equal(Chant.fromPsalm(plain, "8.G", { intone: null, initial: null }).psalm.gabc, psalm(plain, "8.G").gabc);
// Errors say which they are by a code; options a function doesn't take, and values not among
// those it takes, throw a TypeError.
const invalid = { name: "TypeError", code: "invalid-option" };
assert.throws(() => psalm(plain, "8.G", { intone: "always" }), invalid);
assert.throws(() => psalm(plain, "8.G", { pointing: "auto" }), { ...invalid, message: /psalm has no option "pointing"; its options are intone, autoPoint, accents/ });
assert.throws(() => psalm(plain, "8.G", "first"), invalid);
assert.throws(() => psalm(plain, "9.z"), { name: "Error", code: "tone" });
assert.throws(() => Chant.fromTone("9.z"), { code: "tone" });
assert.throws(() => new Chant("(c4) a(g)", { intial: 1 }), { ...invalid, message: /Chant has no option "intial"/ });
assert.throws(() => new Chant("(c4) a(g)", { font: "EB Garamond" }), { ...invalid, message: /font must be "google", "eb-garamond-12"/ });
assert.throws(() => Chant.fromPsalm(plain, "8.G", { lyricsize: 3 }), invalid);
assert.throws(() => Chant.fromTone("8.G", { annotaton: false }), invalid);
{
  const c = new Chant("(c4) a(g) b(h)", null);
  assert.equal(c.layout(300, null).svg, new Chant("(c4) a(g) b(h)").layout(300).svg);
  assert.equal(new Chant("(c4) a(g) b(h)", { initial: null, font: null }).layout(300).svg, c.layout(300).svg);
  assert.throws(() => c.setOptions({ size: 3 }), invalid);
  // setOptions replaces the options, as in Rust and on mobile: one left out takes its default.
  const r = new Chant("(c4) a(g) b(h)", { initial: 0, lyricSize: 4 });
  assert.equal(r.setOptions({ lyricSize: 4 }), true, "initial back to 1");
  assert.equal(r.layout(300).svg, new Chant("(c4) a(g) b(h)", { lyricSize: 4 }).layout(300).svg);
  assert.equal(r.setOptions(), true);
  assert.equal(r.layout(300).svg, c.layout(300).svg);
  assert.equal(r.setOptions(null), false);
  assert.throws(() => c.layout(300, { svg: "parts" }), { ...invalid, message: /svg must be "whole", "lines"/ });
  assert.throws(() => c.layout(300, { lastLine: "justify" }), invalid);
  assert.throws(() => c.layout(300, { timeline: false }), invalid);
  assert.throws(() => c.layout(300, { weights: { notes: 2 } }), { ...invalid, message: /weights has no option "notes"/ });
  assert.throws(() => c.view({ svg: "line" }), invalid);
  assert.throws(() => c.view({ weights: {} }), invalid, "a view takes weights per layout");
  const v = c.view();
  assert.throws(() => v.layout(300, { scale: 2 }), invalid);
  assert.equal(v.layout(300, { weights: { note: null } }).svg, c.layout(300).svg);
  const p = v.layout(300);
  assert.throws(() => p.elementsAt(5, { unit: "bytes" }), invalid);
  assert.throws(() => p.elementsAt(5, { units: "utf8" }), invalid);
  assert.deepEqual(p.elementsAt(5, { unit: null }), p.elementsAt(5));
  assert.throws(() => setLayoutBudget({ layouts: 3 }), invalid);
  assert.throws(() => new Page(), { name: "TypeError", code: "no-constructor" });
  // Chants and views are disposable too.
  if (typeof Symbol.dispose === "symbol") {
    v[Symbol.dispose]();
    assert.equal(v.layout(300), p, "a disposed view still knows its page");
    c[Symbol.dispose]();
    assert.ok(c.update("(c4) a(g) c(i)") && c.layout(300).svg.startsWith("<svg"), "a disposed chant still works");
  }
}
// A line copied from the display sets as the text it came from.
const copied = shown.verses.map((v) => `${v.number} ` + v.runs.map((r) => r.text).join("").replace("Sit.", "[Sit.]")).join("\n");
assert.equal(psalm(copied, "8.G").gabc, psalm(verses, "8.G").gabc);
assert.equal(point(copied, "8.G").text, point(verses, "8.G").text);
assert.deepEqual(spelled.verses[0].runs.filter((r) => r.kind === "syllable").slice(5, 9).map((r) => r.sourceEnd - r.sourceStart), [5, 4, 2, 5]);

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

const page = ed.layout(500);
assert.ok(page.svg.startsWith("<svg"));
for (const n of page.timeline().notes) {
  const hit = page.sourceAt(n.cx, n.cy);
  assert.equal(hit.kind, "note");
  assert.equal(hit.index, n.id);
}
const hi = fixed.indexOf("hi");
const at = page.elementsAt(hi + 1);
assert.deepEqual(at.map((e) => e.kind), ["note", "syllable"]);
assert.equal(fixed.slice(at[0].utf16Start, at[0].utf16End), "i");
assert.equal(fixed.slice(at[1].utf16Start, at[1].utf16End), "ri(hi)");
const bar = page.elementsAt(fixed.indexOf(","));
assert.equal(bar[0].kind, "bar");
assert.equal(page.sourceAt(bar[0].x + bar[0].w / 2, bar[0].y + 1).kind, "bar");
// The same caret as a byte offset.
const bytes = new TextEncoder().encode(fixed.slice(0, hi + 1)).length;
assert.deepEqual(page.elementsAt(bytes, { unit: "utf8" }), at);
assert.equal(page.sourceAt(-100, -100), null);
// Carets past either end are at it, however far: none wraps around to the start.
const atEnd = page.elementsAt(fixed.length);
for (const far of [fixed.length + 1, 2 ** 32, 2 ** 32 + 5, 2 ** 53, Infinity]) {
  assert.deepEqual(page.elementsAt(far), atEnd, String(far));
  if (far > 2 ** 31) assert.deepEqual(page.elementsAt(far, { unit: "utf8" }), atEnd, String(far));
}
for (const before of [-1, -(2 ** 32), -Infinity, NaN]) {
  assert.deepEqual(page.elementsAt(before), page.elementsAt(0), String(before));
}

// The SVG a line at a time, without ids, draws what the whole SVG draws.
const parts = ed.layout(500, { svg: "lines", ids: false }).svgParts;
assert.ok(parts.head.startsWith("<svg") && parts.defs.startsWith("<path"));
assert.equal(parts.lines.length, page.timeline().lines.length);
const count = (text, tag) => text.split(tag).length - 1;
const lineSvg = parts.lines.map((l) => l.svg).join("");
assert.equal(count(lineSvg, "<use ") + count(parts.rest, "<use "), count(page.svg, "<use "));
assert.ok(!lineSvg.includes("data-note"));
assert.ok(parts.lines.every((l, i) => i === 0 || l.top > parts.lines[i - 1].top));
// Notes are found under a point after other layouts.
const first = page.timeline().notes[0];
assert.equal(page.noteAt(first.cx, first.cy), first.id);
// Edits one after another: the lines the engine kept, and those it made again, are what a
// fresh Chant draws, with ids or without.
let edited = fixed;
for (let i = 0; i < 12; i++) {
  const at = edited.indexOf("(", (i * 37) % edited.length);
  edited = i % 3 === 2 ? edited.slice(0, at) + edited.slice(at + 3) : edited.slice(0, at) + "a" + edited.slice(at);
  ed.update(edited);
  const opts = { svg: "lines", ids: i % 4 === 3 };
  const fresh = new Chant(edited, { initial: 0 });
  assert.deepEqual(ed.layout(200, opts), fresh.layout(200, opts), `edit ${i}`);
  fresh.free();
}
// The same options change nothing: a view gives the page it laid out.
const edView = ed.view();
const before = edView.layout(500);
const tap = before.timeline().notes[0];
const version = ed.version;
assert.equal(ed.setOptions({ initial: 0 }), false);
assert.equal(ed.version, version);
assert.equal(edView.layout(500), before);
// A larger lyric engraves again; the page from before still answers for what it shows.
assert.equal(ed.setOptions({ initial: 0, lyricSize: 4 }), true);
assert.ok(ed.version > version);
// Versions name states across chants: no other chant has had this one.
assert.ok(![version, ed.version].includes(new Chant(fixed, { initial: 0 }).version));
assert.ok(before.stale && before.version === version);
const larger = edView.layout(500);
assert.ok(larger.height > before.height && !larger.stale && larger.version === ed.version);
assert.equal(before.noteAt(tap.cx, tap.cy), tap.id);
// A page answers for as long as it is held: through many layouts at other widths and edits
// between, after its layout is dropped from the engine's cache, and after its chant is freed.
const held = [500, 300, 400, 600, 700].map((w) => ed.layout(w));
const kept = held[0].timeline().notes.at(-1);
const answers = (p) => JSON.stringify([p.noteAt(kept.cx, kept.cy), p.sourceAt(kept.cx, kept.cy), p.elementsAt(5), p.elementsAt(40)]);
const firstAnswers = held.map(answers);
for (let i = 0; i < 40; i++) {
  ed.update(edited + " a(g)".repeat(i + 1));
  for (const w of [500, 300, 250]) ed.layout(w, { svg: i % 2 ? "lines" : "whole" });
}
{
  const { layouts, staleLayouts, budget } = engineStats();
  assert.ok(staleLayouts <= budget.stale && layouts - staleLayouts <= budget.current, "the engine keeps at most its budget");
  assert.ok(staleLayouts > 0, "the held pages are stale");
}
assert.ok(held.every((p) => p.stale));
assert.deepEqual(held.map(answers), firstAnswers);
assert.equal(held[0].noteAt(kept.cx, kept.cy), kept.id);
// A view's pages are its own: freeing one only drops the engine's copy.
const third = edView.layout(400);
before.free();
assert.equal(before.noteAt(tap.cx, tap.cy), tap.id);
assert.equal(before.timeline().notes[0], tap, "a timeline already made stays");
assert.equal(before.timeline, Page.prototype.timeline, "a method, not a getter");
// Alternating widths in one view swap its two pages without laying out again.
const at200 = edView.layout(200), at250 = edView.layout(250);
assert.equal(edView.layout(200), at200);
assert.equal(edView.layout(250), at250);
const after = edView.layout(500);
assert.ok(!after.stale && !third.stale && after !== third);
ed.free();
assert.equal(after.noteAt(kept.cx, kept.cy), kept.id);
assert.ok(after.elementsAt(0).length > 0);
assert.equal(edView.layout(500), after, "a freed chant's view still knows its page");
edView.free();
// Changed after it was freed, the chant changes as one never freed would.
assert.equal(ed.update(src), true);
assert.ok(after.stale);
assert.equal(edView.layout(600).svg, new Chant(src, { initial: 0, lyricSize: 4 }).layout(600).svg);
assert.deepEqual(held.map(answers), firstAnswers);
// A budget of one layout: every other page lays itself out again when asked.
const budget = engineStats().budget;
setLayoutBudget({ current: 1, stale: 1 });
assert.ok(engineStats().layouts <= 2);
assert.deepEqual(held.map(answers), firstAnswers);
setLayoutBudget(budget);
assert.deepEqual(engineStats().budget, { current: 64, stale: 2 });
setLayoutBudget(3);
assert.deepEqual(engineStats().budget, { current: 3, stale: 2 }, "a number sets the current pool's");
// A budget replaces the last one: a pool left out, undefined or null takes its default.
setLayoutBudget({ current: 5, stale: 7 });
setLayoutBudget({ stale: 4 });
assert.deepEqual(engineStats().budget, { current: 64, stale: 4 });
setLayoutBudget({ current: 9, stale: 7 });
setLayoutBudget(null);
assert.deepEqual(engineStats().budget, { current: 64, stale: 2 });
setLayoutBudget({ current: 2.9, stale: Infinity });
assert.deepEqual(engineStats().budget, { current: 2, stale: 0xffffffff }, "rounded down, and capped as the engine takes it");
setLayoutBudget(2 ** 40);
assert.deepEqual(engineStats().budget, { current: 0xffffffff, stale: 2 });
for (const bad of [0, -3, 0.5, NaN, -Infinity, "4", { current: 0 }, { stale: -1 }, { current: "8" }]) {
  assert.throws(() => setLayoutBudget(bad), { name: "TypeError", code: "invalid-option" }, String(bad));
}
assert.deepEqual(engineStats().budget, { current: 0xffffffff, stale: 2 }, "a budget that throws changes nothing");
setLayoutBudget();
assert.deepEqual(engineStats().budget, { current: 64, stale: 2 });

// Each view in parts, here a page and a thumbnail, reuses the lines of its own last page,
// even when the caller has changed that page's parts.
{
  let body = "(c4) " + Array.from({ length: 30 }, (_, i) => `s${i}(${"fgh"[i % 3]})`).join(" ") + " (::)";
  const doc = new Chant(body, { initial: 0 });
  const mainView = doc.view({ svg: "lines" });
  const thumbView = doc.view({ svg: "lines", scale: 3 });
  let main = mainView.layout(300);
  let thumb = thumbView.layout(300);
  assert.ok(main.svgParts.lines.length > 2 && thumb.svgParts.lines.length > 1);
  for (let i = 0; i < 4; i++) {
    body = body.replace("(::)", "x(g) (::)");
    assert.equal(doc.update(body), true);
    assert.equal(doc.update(body), false);
    const lines = main.svgParts.lines.map((l) => l.svg);
    if (i === 2) main.svgParts.lines.length = 0;
    const nextMain = mainView.layout(300);
    const nextThumb = thumbView.layout(300);
    const fresh = new Chant(body, { initial: 0 });
    assert.deepEqual(nextMain.svgParts, fresh.layout(300, { svg: "lines" }).svgParts, `main ${i}`);
    assert.deepEqual(nextThumb.svgParts, fresh.layout(300, { svg: "lines", scale: 3 }).svgParts, `thumb ${i}`);
    fresh.free();
    // The first line didn't change, and comes back as the same string.
    assert.equal(nextMain.svgParts.lines[0].svg, lines[0]);
    [main, thumb] = [nextMain, nextThumb];
  }
  // The same arguments give the same page, each view its own; two views of the same
  // options (two panels, a component mounted twice) have pages of their own, and freeing
  // one leaves the other.
  assert.equal(mainView.layout(300), main);
  assert.equal(thumbView.layout(300), thumb);
  const twin = doc.view({ svg: "lines" });
  const twinPage = twin.layout(300);
  assert.notEqual(twinPage, main);
  twin.free();
  const first = main.timeline().notes[0];
  assert.equal(main.noteAt(first.cx, first.cy), first.id);
  // A page its owner freed is still the view's page, and still answers.
  main.free();
  assert.equal(mainView.layout(300), main);
  assert.equal(main.noteAt(first.cx, first.cy), first.id);
  if (typeof Symbol.dispose === "symbol") {
    const p = doc.layout(200);
    const hit = p.noteAt(first.cx, first.cy);
    p[Symbol.dispose]();
    assert.equal(p.noteAt(first.cx, first.cy), hit);
  }
  doc.free();
}

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
  assert.throws(() => glue.summarize("x"), { code: "not-initialized" });
  glue.initSync(bytes);
  // A build that leaves psalm tones out says so.
  assert.throws(() => glue.psalm("a * b", "8.G"), { code: "unsupported", message: /no psalm tones/ });
  assert.throws(() => glue.point("a * b", "8.G"), { code: "unsupported", message: /no automatic pointing/ });
  assert.throws(() => glue.summarize("x"), RangeError);
  assert.throws(() => glue.summarize("x"), { code: "engine-stopped", message: /stopped on an internal error; call init\(\) again/ });
  await glue.init();
  assert.throws(() => glue.toneNames(), WebAssembly.RuntimeError);
  assert.throws(() => glue.toneNames(), /call init\(\) again/);
  await glue.init();
  assert.throws(() => glue.summarize("x"), RangeError);
}

// Versions keep growing across a restart of the engine, whose own count starts again: a copy
// of the glue whose engine traps once, on demand.
{
  const Real = WebAssembly.Instance;
  let trap = false;
  WebAssembly.Instance = function (module, imports) {
    const real = new Real(module, imports);
    const summarize = real.exports.neuma_summarize;
    return {
      exports: {
        ...real.exports,
        neuma_summarize: () => {
          if (!trap) return summarize();
          trap = false;
          throw new WebAssembly.RuntimeError("unreachable");
        },
      },
    };
  };
  try {
    const glue = await import("./dist/neuma.mjs?restart");
    await glue.init();
    const a = new glue.Chant("(c4) a(g)");
    a.update("(c4) a(h)");
    const seen = [a.version];
    const page = a.layout(300);
    trap = true;
    assert.throws(() => glue.summarize("x"), WebAssembly.RuntimeError);
    await glue.init();
    const b = new glue.Chant("(c4) b(g)");
    assert.ok(b.version > seen[0], "a chant made after the restart");
    assert.equal(a.version, seen[0], "a chant made before keeps its state");
    assert.ok(!page.stale);
    a.update("(c4) a(i)");
    assert.ok(a.version > b.version && page.stale);
  } finally {
    WebAssembly.Instance = Real;
  }
}

// Memory past 2 GiB, where the engine's pointers are negative as signed numbers: a copy of
// the glue whose engine has grown its memory by 2 GiB before it allocates anything.
{
  const Real = WebAssembly.Instance;
  WebAssembly.Instance = function (module, imports) {
    const real = new Real(module, imports);
    real.exports.memory.grow(32768);
    return real;
  };
  try {
    const glue = await import("./dist/neuma.mjs?high");
    await glue.init();
    const high = new glue.Chant(gabc);
    assert.ok(glue.engineStats().memory > 2 ** 31);
    assert.equal(high.layout(500).svg, chant.layout(500).svg);
    assert.equal(high.update(gabc.replace("(", "(f) a(")), true);
    assert.deepEqual(glue.summarize(gabc), summarize(gabc));
  } finally {
    WebAssembly.Instance = Real;
  }
}

// The module without the wasm inlined, given its bytes, works as the inlined one. Without
// them, it fetches neuma.wasm: Node's fetch takes no file: URL, and the error says to pass
// the bytes.
{
  const external = await import("./dist/neuma-external.mjs");
  await assert.rejects(external.init(), { code: "fetch", message: /pass the bytes/ });
  const realFetch = globalThis.fetch;
  globalThis.fetch = async () => ({ ok: false, status: 404 });
  try {
    await assert.rejects(external.init(), { code: "fetch", message: /failed: 404/ });
  } finally {
    globalThis.fetch = realFetch;
  }
  await external.init(readFileSync(new URL("./dist/neuma.wasm", import.meta.url)));
  assert.equal(new external.Chant(gabc).layout(500).svg, chant.layout(500).svg);
  assert.ok(readFileSync(new URL("./dist/neuma-external.mjs", import.meta.url)).length < 100_000);
}

// The types declare what the module exports, no more and no less.
{
  const types = readFileSync(new URL("./dist/neuma.d.mts", import.meta.url), "utf8");
  const declared = [...types.matchAll(/^export (?:declare )?(?:const|function|class) (\w+)/gm)].map((m) => m[1]);
  assert.deepEqual([...new Set(declared)].sort(), Object.keys(await import("./dist/neuma.mjs")).sort());
}

assert.equal(DEFAULT_WEIGHTS.note, 1);
console.log(`ok: ${notes.length} notes, ${wide.timeline().lines.length} lines at 900, ${narrow.timeline().lines.length} at 360`);
