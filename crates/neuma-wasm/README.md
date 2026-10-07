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
chant.diagnostics;                 // [{ severity, start, end, utf16Start, utf16End, code, message, fix }]

const page = chant.layout(host.clientWidth, {
  scale: 6,                        // output units per staff space
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

`page` is a `Page`: `{ width, height, svg }`, its `timeline()`, made on the first call, and
its hit tests (below). Every position is in output units (staff spaces times `scale`, the
SVG's user units) from the layout's top left, with y down. A box is `x`, `y`, `w`, `h` from
its top-left corner; a point that is a center is named `cx`, `cy`.

A page answers for the score it shows for as long as it lives, whatever the chant has laid
out or become since: a thumbnail made with `maxLines: 1` leaves the main page's hit tests
alone, and after `chant.update` or `chant.setOptions` a page made before still answers for
the score it shows, so a click or a caret move between an edit and the next frame finds what
is on screen. `page.stale` is true once the chant has changed since the page was laid out:
lay out again to show the new score. `update` and `setOptions` return whether anything
changed, and `chant.version` counts the changes (`page.version` is the chant's version the
page was laid out at), as in Rust and on mobile.

### Views: pages for a place that shows the score

Each page holds its layout, a whole engraving's worth of memory on a long score, in the
engine. A place that lays the score out again on every change, an editor's preview above
all, does it through a **view**, which frees the pages it replaces:

```js
const view = chant.view({ svg: "lines", ids: false }); // the options but width and weights
let page = view.layout(host.clientWidth);               // on each change and each resize
```

- `view.layout(width, { weights })` returns the view's page for the chant as it is now. Asked
  again with the same width and weights, and no change to the chant, it returns the same
  page; otherwise a new one, made reusing the last page's lines (with `svg: "lines"`).
- The view keeps its current page (`view.page`) and the one before it, so a click between an
  edit and the next frame still finds what was on screen. It frees any older page itself.
- `page.keep()` takes a page out of its view's care, for a page held longer (compared against
  later, shown elsewhere); free a kept page yourself. `view.free()` frees the view's pages
  but the kept ones.
- Give each place its own view (each panel, each component instance): two views never share
  pages, so one freeing its pages leaves the other's alone.

`chant.layout(width, options)` makes a one-off page, a new one on each call: a static
rendering, a print, a thumbnail drawn once. It lives until `page.free()` (or `using page =
chant.layout(…)`, where the runtime has explicit resource management) or until it is garbage
collected: a `FinalizationRegistry` frees its engine half then. That only runs between tasks
and when the collector gets to it, so an editor that lays out with `chant.layout` on every
keystroke and never frees grows the engine's memory far faster than the collector returns it
(hundreds of megabytes on the longest scores, which a phone's browser may not survive). A
view keeps an editor's memory flat.

**A freed page throws** from its hit tests and `timeline()`, with a message saying how it was
freed, as a freed `Chant` does: an answer of `null` would read as "nothing there" and hide
the bug of asking an old page. `page.freed` says so without throwing, and the page's `svg`,
`svgParts` and a timeline already made stay readable. With views, only a page held past two
layouts of its view, without `keep()`, is freed under you.

**Without `FinalizationRegistry`** (very old runtimes), nothing frees a page that is dropped:
views still free what they replace, and pages from `chant.layout` must be freed by hand.

**Memory and time.** A page held when the chant changes makes the update copy the engraving
rather than change it in place, and an editor always holds the page on screen, so this copy
is part of an edit's cost: on the longest scores (about 4,900 notes) about 1.7 ms of a 5 ms
update, on typical ones next to nothing. Freeing replaced pages doesn't avoid it; it bounds
memory, since each page kept past an edit keeps a whole engraving alive.

- **`timeline.notes`**: one entry per note, in singing order, with these fields:
  - `id`: stable across layouts of one `Chant`. Each SVG element lists the notes it draws
    in `data-note`, so select a note's ink with `[data-note~="<id>"]`: a porrectus swash
    draws two notes and reads `data-note="4 5"`.
  - `cx`, `cy`: the notehead's center. `w`, `h`: its size. For a porrectus, the swash's
    two ends.
  - `line`, `syllable`, `word`.
  - `start`, `duration`: in weight units. Choose your own tempo.
  - `staffPosition`, `degree`, `semitones`: `semitones` counts from the clef's do, with
    flats applied.
  - `syllableText`, `vowel`, `shape` (`quilisma` among them), `liquescent`.
  - `accent`: the syllable has an acute in the source.
  - `newSyllable`: the first note of its syllable.
  - `recitation`: inferred from three or more single-note syllables on one pitch.
  - `verse`, `half`: `verse` advances after each full or double bar. `half` turns to 1 at
    the mediant `*`.
  - `sourceStart`, `sourceEnd`: the note's bytes in the GABC source;
    `sourceUtf16Start`, `sourceUtf16End`: the same in UTF-16 units.
- **`timeline.pauses`**: `{ beforeNote, kind, start, duration }`. `kind` is one of
  `virgula`, `minimis`, `quarter`, `half`, `full`, `dotted-full`, `double`, `dominican`,
  `mediant` (`*`) or `flex` (`†`). A mediant or flex is the whole pause at its bar: the bar
  right after it stays in the list with duration 0.
- **`timeline.lines`**: each line's `top`, `bottom`, `staff` (the middle line) and
  `baseline` (the lyrics).
- **`timeline.duration`**: the total length.

`noteAtTime(timeline, t)` returns the note sounding at time `t`, or `null` during a pause,
for a playhead that follows audio.

Offsets named `start` and `end` (and the timeline's `sourceStart` and `sourceEnd`) count
UTF-8 bytes of the source, as the engine does. Offsets named with `utf16` (`utf16Start`,
`utf16End`, `sourceUtf16Start`, `sourceUtf16End`), and the carets the editor calls take,
count UTF-16 code units: JavaScript string indices, as `textarea` and
CodeMirror use. They differ once the source has a character outside ASCII (`é`, `℣`).

Each diagnostic has a `code` that stays stable across versions (see
[docs/diagnostics.md](../../docs/diagnostics.md) for the list), a `message` for people, and a
`fix` that is `null` or the one edit that fixes it:
`{ start, end, utf16Start, utf16End, replacement, title }`, replacing
`utf16Start`..`utf16End` with `replacement`.

`page.noteAt(x, y)` returns the note under a point, or the nearest note on that line, or
`null`. `chant.setOptions({ lyricSize: 3 })` engraves again with new options (those the
constructor takes). `chant.free()` releases the score; using a freed `Chant` throws, and its
pages keep answering for what they show.

Weights default to `DEFAULT_WEIGHTS`: one pulse per note, two for a dotted note, and pauses
that grow with the bar. Any key you pass with a number overrides its default, except a negative one; no weight
goes above 1000.

If the engine ever stops on an internal error (a WebAssembly trap, or a `RangeError` for a
stack overflow), that call throws and so does every later one until you call `init()` again,
which starts a fresh engine. Make the `Chant`s again after that.

## Editors

A `Chant` can back a GABC editor with a live preview: update it on each change, lay it out,
and link the source and the score both ways.

```js
const chant = new Chant(textarea.value, { initial: 1 });
const view = chant.view({ svg: "lines", ids: false });
let page = view.layout(host.clientWidth);
textarea.addEventListener("input", () => {
  chant.update(textarea.value);    // keeps the options; diagnostics follow the new source
  page = view.layout(host.clientWidth); // the view frees the pages it replaces
  // page.svgParts: { head, defs, rest, lines: [{ top, svg }] }
});
host.addEventListener("click", (e) => {
  // Layout coordinates from the score's top left: `offsetX`/`offsetY` would be relative to
  // whichever line's <svg> was clicked. If `host` scrolls or has a border, count them too.
  const box = host.getBoundingClientRect();
  const x = e.clientX - box.left - host.clientLeft + host.scrollLeft;
  const y = e.clientY - box.top - host.clientTop + host.scrollTop;
  const hit = page.sourceAt(x, y); // { kind, index, utf16Start, utf16End, x, y, w, h, cx, … }
  if (hit) textarea.setSelectionRange(hit.utf16Start, hit.utf16End);
});
const lit = page.elementsAt(textarea.selectionStart); // what to highlight for the caret
```

- **`chant.update(gabc)`** replaces the score, keeping the options, and returns whether it
  changed. Lay it out again to see it.
- **`page.timeline()`** is made only when called, so an editor that never reads it skips what on
  a long score is most of the layout's cost.
- **`{ svg: "lines" }`** gives `page.svgParts` instead of `page.svg`:
  `head` (the `<svg>` start tag and style), `defs` (the glyph `<path>`s, for a `<defs>`),
  `rest` (the initial and its annotations) and `lines`, each `{ top, svg }` with the line's
  elements positioned relative to its top. Wrap each in
  `<g transform="translate(0 top)">`. A line that only moved keeps its `svg` string, and with
  `ids: false` (no `data-note` or `data-syllable`) so does a line after notes added above it,
  so an editor can replace only the lines whose string changed. On a long score, replacing
  the whole SVG costs the browser far more than the engine's work. Giving each line its own
  `<svg>` (`<use>` finds the glyphs in one shared `<defs>` anywhere in the page) keeps the
  browser's work to the changed line as well; the example below does. Each view, such as
  the main page's and a thumbnail's, reuses the lines of its own last page; `chant.layout`
  reuses none. A page in parts keeps what the engine needs to reuse its lines, about two
  thirds again the memory of the strings.
- **`page.sourceAt(x, y)`** returns what is under a point of the page, most specific
  first: a notehead, a bar (within half a staff space), a syllable's box, or the nearest
  syllable on that line; `null` outside the lines. The result is
  `{ kind: "note"|"bar"|"syllable", index, start, end, utf16Start, utf16End, line, x, y, w, h, cx }`:
  `utf16Start`..`utf16End` is the source to select, and `x`, `y`, `w`, `h` the box drawn (a syllable's
  box spans its line's height, across its notes and lyric); `cx` is a note's notehead
  center. `index` is the note id, or the bar's or syllable's index in the score.
- **`page.elementsAt(caret)`** returns what to highlight for a caret: the notes and bar
  whose source holds it, then a box per line for its syllable. A caret just after a note,
  as after typing it, counts as on it. Pass `{ unit: "utf8" }` to give a byte offset.

[`examples/editor.html`](examples/editor.html) is a dependency-free editor built on these: a textarea, the score
redrawn on each keystroke (coalesced to animation frames, patching only changed lines),
diagnostics underlined in the source with their messages on hover and one-click fixes,
click-to-select from the score, and the caret's note, bar and syllable highlighted. Serve
the repository root over HTTP and open `crates/neuma-wasm/examples/editor.html` after
building `dist/neuma.mjs`.

### CodeMirror 6

The same calls fit CodeMirror. An update listener keeps the chant and the preview in step
with the document and highlights the caret's note, bar and syllable; CodeMirror's lint
extension draws the diagnostics' underlines, shows each message on hover and offers the
fixes as actions. Positions are already UTF-16:

```js
import { linter } from "@codemirror/lint";
import { EditorView } from "@codemirror/view";

const chant = new Chant(gabc, { initial: 1 });
const score = chant.view({ svg: "lines", ids: false }); // the preview's view
let page = null;                   // the page shown, which answers the caret and clicks

// Keep the chant and the preview in step with the document.
function show(doc) {
  chant.update(doc);
  const next = score.layout(host.clientWidth); // the view frees the pages it replaces
  if (next === page) return;
  page = next;
  draw(page);                      // patch the preview from page.svgParts, as below
}
const preview = EditorView.updateListener.of((u) => {
  if (u.docChanged) show(u.state.doc.toString());
  if (u.docChanged || u.selectionSet) highlight(page.elementsAt(u.state.selection.main.head));
});

// The linter brings the chant up to the document itself: when it already is, `update` is a
// free no-op, and when the preview lags (laid out in an animation frame), it still lints
// the text on screen.
const gabcLint = linter((view) => {
  chant.update(view.state.doc.toString());
  return chant.diagnostics.map((d) => ({
    from: d.utf16Start,
    to: d.utf16End,
    severity: d.severity,          // "error" | "warning" | "info"
    message: d.message,
    source: d.code,
    actions: d.fix ? [{
      name: d.fix.title,
      apply: (v) => v.dispatch({ changes: { from: d.fix.utf16Start, to: d.fix.utf16End, insert: d.fix.replacement } }),
    }] : [],
  }));
}, { delay: 0 });

const view = new EditorView({ doc: gabc, extensions: [preview, gabcLint], parent: editorHost });
show(view.state.doc.toString());

// A click in the preview selects the source of what's under it.
host.addEventListener("click", (e) => {
  const box = host.getBoundingClientRect();
  const hit = page.sourceAt(e.clientX - box.left, e.clientY - box.top);
  if (hit) view.dispatch({ selection: { anchor: hit.utf16Start, head: hit.utf16End } });
});
```

`draw` patches the preview from `page.svgParts` and `highlight` draws the returned boxes
over it, as the example page does. On a long score, laying out in an animation frame rather
than on every keystroke saves work; a caret move in between is answered by the page still
shown, for the score it shows (it is `stale` until the frame).

## Library entries

`summarize(gabc)` reads a score's library entry without engraving it for display, which is
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
- `lowest`, `highest` and `finalPitch`: in semitones above the clef's do (`null` with no
  notes).
- `notes`, `syllables`, `words`, and `duration` in pulses with the default weights.

For a preview, lay the score out with `maxLines: 1`. The first line is broken as it would
be in the whole score. An initial that spans more staves keeps its full size, and the page's
height includes it. The timeline ends with the kept lines: their notes and the pauses drawn
on them.

`neuma notes FILE` prints the page's size and timeline as JSON from the command line.
`neuma info FILE...` prints one library entry per file, as a line of JSON with a `file`
field.

## Psalm tones

`psalm(text, tone, { intone, autoPoint })` sets psalm text, a verse per line with the mediant
marked `*`, to a tone: a built-in name from `toneNames()` such as `"8.G"`, or a tone block of your
own. It returns `{ gabc, notes, diagnostics }`: `notes[i]` gives note `i`'s verse, half and
role in the tone (intonation, tenor, preparatory, accent, ending), with the sung syllable's
source in `text` named as the timeline names a note's (`sourceStart`, `sourceEnd`,
`sourceUtf16Start`, `sourceUtf16End`). Text can be hand-pointed (`·`, acutes, `†`, `–`);
half-verses with no marks are pointed automatically unless `autoPoint: false`, and a
`point::unsure` diagnostic flags each one worth checking.

`Chant.fromPsalm(text, tone, { intone, autoPoint, ...chantOptions })` sets and engraves it in
one step, with its sources in `text`: a tap on a note finds its syllable in the psalm text,
and the diagnostics count the text. `chant.psalm` is the setting as `psalm` returns it,
`{ gabc, notes, diagnostics }`, and `chant.update(text)` sets new text to the same tone. `new Chant(gabc)` engraves the GABC
instead, with sources in the GABC.

`point(text, tone)` returns the pointed text itself, `{ text, halves, diagnostics }`, with the
pointer's confidence (0 to 1) for each half-verse it marked, and each half's source in
`text`, for an editor to show.
