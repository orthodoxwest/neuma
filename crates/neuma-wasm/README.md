# neuma for the browser

One ES module, `dist/neuma.mjs`, with the engine inlined as gzipped WebAssembly (about
190 KB). It fetches nothing, so it works inside sandboxed pages that block other origins.

```sh
cargo build -p neuma-wasm --target wasm32-unknown-unknown --profile wasm
node crates/neuma-wasm/build.mjs path/to/wasm-opt   # wasm-opt is optional
node crates/neuma-wasm/test.mjs
```

CI builds the module on every push and keeps it as the `neuma-browser` artifact.

## Use

```js
import { init, Chant } from "./neuma.mjs";

await init();                      // once; everything after is synchronous
const chant = new Chant(gabc, { initial: 1, annotation: true });
chant.diagnostics;                 // [{ severity, start, end, from, to, code, message, fix }]

const page = chant.layout(host.clientWidth, {
  scale: 6,                        // SVG units per staff space
  lastLine: "ragged",              // or "justified"
  maxLines: 0,                     // 1 for an incipit preview; 0 keeps every line
  weights: { mediant: 3, full: 2.5 },
});
host.innerHTML = page.svg;
```

The lyric font is EB Garamond. Load it on the page from Google Fonts
(`family=EB+Garamond:ital@0;1`), and the layout spaces lyrics with that font's metrics. A
page that serves the EB Garamond 12 files instead passes `{ font: "eb-garamond-12" }`. Leave
the SVG text's size, weight and letter-spacing alone: CSS that changes them changes the
widths the layout planned for.

`page` is `{ width, height, svg, timeline }`. All positions are in the SVG's user units.

- **`timeline.notes`**: one entry per note, in singing order, with these fields:
  - `id`: stable across layouts of one `Chant`. Each SVG element lists the notes it draws
    in `data-note`, so select a note's ink with `[data-note~="<id>"]`: a porrectus swash
    draws two notes and reads `data-note="4 5"`.
  - `x`, `y`, `w`, `h`: the notehead's center and size. For a porrectus, the swash's two
    ends.
  - `line`, `syllable`, `word`.
  - `start`, `duration`: in weight units. Choose your own tempo.
  - `staffPosition`, `degree`, `semitones`: `semitones` counts from the clef's do, with
    flats applied.
  - `syllableText`, `vowel`, `shape`, `liquescent`, `quilisma`.
  - `accent`: the syllable has an acute in the source.
  - `newSyllable`: the first note of its syllable.
  - `recitation`: inferred from three or more single-note syllables on one pitch.
  - `verse`, `half`: `verse` advances after each full or double bar. `half` turns to 1 at
    the mediant `*`.
  - `spanStart`, `spanEnd`: the note's bytes in the GABC source.
- **`timeline.pauses`**: `{ beforeNote, kind, weight, start }`. `kind` is one of
  `virgula`, `minimis`, `quarter`, `half`, `full`, `dotted-full`, `double`, `dominican`,
  `mediant` (`*`) or `flex` (`†`). A mediant or flex is the whole pause at its bar: the bar
  right after it stays in the list with weight 0.
- **`timeline.lines`**: each line's `top`, `bottom`, `staff` (the middle line) and
  `baseline` (the lyrics).
- **`timeline.duration`**: the total length.

Offsets named `start` and `end` (and the timeline's `spanStart` and `spanEnd`) count UTF-8
bytes of the source, as the engine does. Offsets named `from` and `to`, and the carets the
editor calls take, count UTF-16 code units: JavaScript string indices, as `textarea` and
CodeMirror use. They differ once the source has a character outside ASCII (`é`, `℣`).

Each diagnostic has a `code` that stays stable across versions (see
[docs/diagnostics.md](../../docs/diagnostics.md) for the list), a `message` for people, and a
`fix` that is `null` or the one edit that fixes it: `{ start, end, from, to, insert, title }`,
replacing `from`..`to` with `insert`.

`chant.noteAt(x, y)` returns the note under a point in the last layout, or the nearest note
on that line, or `null`. `chant.free()` releases the score; using a freed `Chant` throws.

Weights default to `DEFAULT_WEIGHTS`: one pulse per note, two for a dotted note, and pauses
that grow with the bar. Any key you pass with a number overrides its default.

If the engine ever stops on an internal error (a WebAssembly trap, or a `RangeError` for a
stack overflow), that call throws and so does every later one until you call `init()` again,
which starts a fresh engine. Make the `Chant`s again after that.

## Editors

A `Chant` can back a GABC editor with a live preview: update it on each change, lay it out,
and link the source and the score both ways.

```js
const chant = new Chant(textarea.value, { initial: 1 });
textarea.addEventListener("input", () => {
  chant.update(textarea.value);    // keeps the options; diagnostics follow the new source
  const page = chant.layout(host.clientWidth, { timeline: false, svg: "lines", ids: false });
  // page.svgParts: { head, defs, rest, lines: [{ top, svg }] }
});
host.addEventListener("click", (e) => {
  // Layout coordinates from the score's top left: `offsetX`/`offsetY` would be relative to
  // whichever line's <svg> was clicked. If `host` scrolls or has a border, count them too.
  const box = host.getBoundingClientRect();
  const x = e.clientX - box.left - host.clientLeft + host.scrollLeft;
  const y = e.clientY - box.top - host.clientTop + host.scrollTop;
  const hit = chant.sourceAt(x, y); // { kind, index, from, to, x, y, w, h, … }
  if (hit) textarea.setSelectionRange(hit.from, hit.to);
});
const lit = chant.elementsAt(textarea.selectionStart); // what to highlight for the caret
```

- **`chant.update(gabc)`** replaces the score, keeping the options. Lay it out again to see it.
- **`layout(width, { timeline: false })`** skips the playback timeline, which on a long score
  is most of the layout's cost. `page.timeline` is then absent.
- **`layout(width, { svg: "lines" })`** returns `page.svgParts` instead of `page.svg`:
  `head` (the `<svg>` start tag and style), `defs` (the glyph `<path>`s, for a `<defs>`),
  `rest` (the initial and its annotations) and `lines`, each `{ top, svg }` with the line's
  elements positioned relative to its top. Wrap each in
  `<g transform="translate(0 top)">`. A line that only moved keeps its `svg` string, and with
  `ids: false` (no `data-note` or `data-syllable`) so does a line after notes added above it,
  so an editor can replace only the lines whose string changed. On a long score, replacing
  the whole SVG costs the browser far more than the engine's work. Giving each line its own
  `<svg>` (`<use>` finds the glyphs in one shared `<defs>` anywhere in the page) keeps the
  browser's work to the changed line as well; the example below does.
- **`chant.sourceAt(x, y)`** returns what is under a point of the last layout, most specific
  first: a notehead, a bar (within half a staff space), a syllable's box, or the nearest
  syllable on that line; `null` outside the lines. The result is
  `{ kind: "note"|"bar"|"syllable", index, start, end, from, to, line, x, y, w, h }`:
  `from`..`to` is the source to select, and `x`, `y`, `w`, `h` the box drawn (a syllable's
  box spans its line's height, across its notes and lyric). `index` is the note id, or the
  bar's or syllable's index in the score.
- **`chant.elementsAt(caret)`** returns what to highlight for a caret: the notes and bar
  whose source holds it, then a box per line for its syllable. A caret just after a note,
  as after typing it, counts as on it. Pass `{ units: "utf8" }` to give a byte offset.

[`examples/editor.html`](examples/editor.html) is a dependency-free editor built on these: a textarea, the score
redrawn on each keystroke (coalesced to animation frames, patching only changed lines),
diagnostics underlined in the source with their messages on hover and one-click fixes,
click-to-select from the score, and the caret's note, bar and syllable highlighted. Serve
the repository root over HTTP and open `crates/neuma-wasm/examples/editor.html` after
building `dist/neuma.mjs`.

### CodeMirror 6

The same calls fit CodeMirror's lint extension, which draws the underlines, shows each
message on hover and offers the fixes as actions; positions are already UTF-16:

```js
import { linter } from "@codemirror/lint";
import { EditorView } from "@codemirror/view";

const gabcLint = linter((view) => {
  chant.update(view.state.doc.toString());
  return chant.diagnostics.map((d) => ({
    from: d.from,
    to: d.to,
    severity: d.severity,          // "error" | "warning" | "info"
    message: d.message,
    source: d.code,
    actions: d.fix ? [{
      name: d.fix.title,
      apply: (v) => v.dispatch({ changes: { from: d.fix.from, to: d.fix.to, insert: d.fix.insert } }),
    }] : [],
  }));
}, { delay: 0 });

// Highlight the caret's note, bar and syllable in the preview.
const follow = EditorView.updateListener.of((u) => {
  if (u.selectionSet || u.docChanged) highlight(chant.elementsAt(u.state.selection.main.head));
});
```

`highlight` draws the returned boxes over the preview, as the example page does.

## Library entries

`summarize(gabc)` reads a score's catalogue entry without engraving it for display, which is
cheap enough to index a whole library. `chant.summary` gives the same entry for a score you
have already loaded. Header fields are `null` when the source leaves them out, and TeX
markup is removed.

- `name`, `officePart`, `occasion`, `book`, `language`, `transcriber`, `gabcCopyright`,
  `scoreCopyright`, `commentary`, `annotations`: the headers as written. `occasion` is a
  short label; keep the full list of days a piece is sung in a calendar.
- `otherHeaders`: every other header as `{ name, value }`, in source order, so a library can
  keep its own fields (`source`, `translation-of`) in the score file. Values are only trimmed:
  TeX markup and empty values are kept.
- `kind`: what `officePart` names, in Latin or English, spelled out or abbreviated:
  `antiphon`, `introit`, `gradual`, `alleluia`, `tract`, `sequence`, `offertory`,
  `communion`, `hymn`, `responsory`, `short-responsory`, `versicle`, `chapter`, `collect`,
  `psalm`, `canticle`, `kyrie`, `gloria`, `credo`, `sanctus`, `agnus` or `other`.
- `mode`: `{ number, name, modifier, differentia }`. `number` is 1 to 8 when the header
  starts with one (`8`, `VIII`, `1g`), else `null` (`per`).
- `incipit`: the opening words, up to the first bar (not a virgula) at or after the end of
  the second word, at most eight.
- `text`: all the sung text, for search. Both skip psalm marks, and write an opening word
  in capitals (`PUER`) as `Puer`, an opening acronym included.
- `range` (`[lowest, highest]`) and `finalPitch`: in semitones above the clef's do.
- `notes`, `syllables`, `words`, and `duration` in pulses with the default weights.

For a preview, lay the score out with `maxLines: 1`. The first line is broken as it would
be in the whole score. An initial that spans more staves keeps its full size, and the page's
height includes it. The timeline ends with the kept lines: their notes and the pauses drawn
on them.

`neuma notes FILE` prints the same layout JSON from the command line, without the SVG.
`neuma info FILE...` prints one catalogue entry per file, as a line of JSON with a `file`
field.

## Psalm tones

`psalm(text, tone, { intone, pointing })` sets psalm text, a verse per line with the mediant
marked `*`, to a tone: a built-in name from `tones()` such as `"8.G"`, or a tone block of your
own. It returns `{ gabc, notes, diagnostics }`: engrave `gabc` with `new Chant(gabc)`, and
`notes[i]` gives note `i`'s verse, half and role in the tone (intonation, tenor, preparatory,
accent, ending). Text can be hand-pointed (`·`, acutes, `†`, `–`); half-verses with no marks
are pointed automatically unless `pointing: "manual"`, and a `point::unsure` diagnostic
flags each one worth checking.

`point(text, tone)` returns the pointed text itself, `{ text, halves, diagnostics }`, with the
pointer's confidence (0 to 1) for each half-verse it marked, for an editor to show.
