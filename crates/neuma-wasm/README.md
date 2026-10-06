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
chant.diagnostics;                 // [{ severity, start, end, code, message }], in UTF-8 bytes

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

Byte offsets (`spanStart`, `spanEnd` and the diagnostics' `start` and `end`) count UTF-8
bytes of the source, not JavaScript string indices. Convert with
`new TextDecoder().decode(new TextEncoder().encode(gabc).subarray(0, offset)).length`.

`chant.noteAt(x, y)` returns the note under a point in the last layout, or the nearest note
on that line, or `null`. `chant.free()` releases the score; using a freed `Chant` throws.

Weights default to `DEFAULT_WEIGHTS`: one pulse per note, two for a dotted note, and pauses
that grow with the bar. Any key you pass with a number overrides its default.

If the engine ever stops on an internal error, that call throws and so does every later one
until you call `init()` again, which starts a fresh engine. Make the `Chant`s again after
that.

## Library entries

`summarize(gabc)` reads a score's catalogue entry without engraving it for display, which is
cheap enough to index a whole library. `chant.summary` gives the same entry for a score you
have already loaded. Header fields are `null` when the source leaves them out, and TeX
markup is removed.

- `name`, `officePart`, `occasion`, `book`, `language`, `transcriber`, `gabcCopyright`,
  `scoreCopyright`, `commentary`, `annotations`: the headers as written. `occasion` is a
  short label; keep the full list of days a piece is sung in a calendar.
- `otherHeaders`: every other header as `{ name, value }`, in source order, so a library can
  keep its own fields (`source`, `translation-of`) in the score file.
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
