# neuma: architecture

neuma is a Rust engine that engraves Gregorian chant in square notation. It reads GABC, or
English psalm text and a psalm tone, and gives SVG, a renderer-neutral display list, a
timeline of every note and a map from the drawing back to the source. The same code runs on
a server, in the browser as WebAssembly and in iOS and Android apps, with the same line
breaks and coordinates on each.

This document describes how the engine is built and why. The API is documented in rustdoc
(`cargo doc --open`), the browser package in [crates/neuma-wasm](../crates/neuma-wasm/README.md),
the mobile bindings in [crates/neuma-mobile](../crates/neuma-mobile/README.md), and every
diagnostic code in [diagnostics.md](diagnostics.md).

**Contents:** [Goals](#goals) · [Consumers](#consumers) · [Crates](#crates) ·
[Data flow](#data-flow) · [Units and coordinates](#units-and-coordinates) ·
[Caching and incremental update](#caching-and-incremental-update) ·
[Determinism](#determinism) · [GABC coverage](#gabc-coverage) ·
[Neumes and glyphs](#neumes-and-glyphs) · [Display list and SVG](#display-list-and-svg) ·
[Lyrics and text measurement](#lyrics-and-text-measurement) · [Timeline](#timeline) ·
[Source map and diagnostics](#source-map-and-diagnostics) ·
[Psalm tones and pointing](#psalm-tones-and-pointing) · [Booklets](#booklets) ·
[Bindings](#bindings) · [Testing](#testing) · [Open questions](#open-questions)

## Goals

1. **One engine for every front end.** Server-rendered SVG, reflow in the browser, native
   drawing in apps, practice and playback tools, and print all use the same layout code. For
   the same width, options and metrics table they produce the same line breaks and
   coordinates.
2. **Reflow.** Layout runs at the real column width and is cheap enough to redo on every
   resize, rotation or text-size change. Reading and engraving happen once; only line
   breaking is redone.
3. **Renderer-neutral output.** The engine never draws. It returns positioned glyphs,
   rectangles and text runs; an SVG writer ships with it, and apps map the list onto their
   own canvases.
4. **Practice support.** Every note carries its pitch, relative duration, position and
   source span, so a tool can drive a cursor, play a guide melody and follow a singer from
   the layout the singer sees.
5. **Psalmody without hand-written GABC.** Pointed text and a tone become a score through
   the same model.
6. **Robustness.** No input panics. Malformed input gives diagnostics with source spans, and
   the engine draws what it can.
7. **Deterministic and testable**, under the rules in [Determinism](#determinism).
8. **A permissive license**, MIT OR Apache-2.0, so apps and tools can vendor it.

Non-goals: replacing GregorioTeX for print editions; an editor UI (editing is fast enough
for live preview, but the editor belongs to the app); NABC (adiastematic neumes),
polyphony, modern notation and staves other than four lines (a `staff-lines:` value other
than 4 gets a diagnostic and draws on four); rasterizing.

## Consumers

| Consumer | Where it runs | What it uses |
|---|---|---|
| [The Office](https://github.com/orthodoxwest/office), a daily-office app | Server-rendered web pages, the browser, iOS and Android | SVG for first paint and for readers without JavaScript; relayout in the browser at the column's width; the display list drawn natively at the device's width and text size; psalm tones and pointed psalms. |
| Practice and playback tools | The browser | SVG, the timeline (pitch, duration, position and source of each note) and `data-note` on the SVG's ink, to highlight what is sung. |
| Chant libraries | Server or browser | Library entries (`summarize`) to list and search scores without engraving them, and previews (`max_lines`). |
| Editors | The browser | Incremental update, diagnostics with fixes, the source map both ways, and SVG a line at a time. |
| Print | The command line | `neuma book`: booklets as PDF and SVG pages. |

## Crates

```
crates/
  neuma/           the engine: GABC parser, score model, engraving, layout, display list, SVG,
                   timeline, source map, library entries, built-in metrics tables. No I/O,
                   no platform calls, no dependencies, #![forbid(unsafe_code)].
  neuma-tones/     psalm tones: the pointed-text parser and writer, English syllabification,
                   the automatic pointer, settings (text and tone to a Score) and the pointed
                   psalter display. Depends on neuma.
  neuma-book/      booklets: the .book format, composition, pagination, a PDF writer with font
                   subsetting, and SVG pages. Depends on neuma, neuma-tones and rustybuzz.
  neuma-wasm/      the browser package: a thin wrapper over neuma::Chant behind a hand-written
                   C ABI, and the JavaScript glue (js/neuma.mjs) that build.mjs bundles with
                   the engine inlined.
  neuma-mobile/    UniFFI bindings for Swift and Kotlin, a thin wrapper over neuma::Chant.
  neuma-cli/       the `neuma` command: render, check, notes, info, tones, point, psalm, book.
  neuma-metrics/   builds the metrics tables lyrics are measured with, from font files
                   (rustybuzz); its examples measure text and look for collisions.
tools/
  gen-glyphs/      exsurge's Glyphs.js to crates/neuma/src/glyphs/table.rs (checked in).
  corpus/          the nightly GregoBase run's extraction script and expected counts.
  readme/          draws the README's images with neuma, and doctests the READMEs.
fuzz/              cargo-fuzz targets: parse, layout, round_trip.
```

The workspace uses edition 2024 and builds on Rust 1.95 and later (the workspace's
`rust-version`, which a CI job tests on). `neuma`, `neuma-tones`, `neuma-book`,
`neuma-metrics` and `neuma-cli` carry crates.io metadata and their license files, and CI
packages each on its own; `neuma-wasm` and `neuma-mobile` are built from the repository
and not published as crates.

## Data flow

```
GABC ── parse ──► Score ─┐
pointed text + tone ─────┤ (neuma-tones)
ScoreBuilder ────────────┘
                         │
                         ▼
       engrave(metrics, StyleOptions) ──► Engraving   width-independent; shared in an Arc
                         │
                         ▼
          layout(width, LayoutOptions) ──► Layout      cheap; redone on every resize
                         │
     ┌──────────┬────────┴─────┬──────────────┐
     ▼          ▼              ▼              ▼
    svg()   display()     timeline()     source_map()
```

- **Score** is the one model. The GABC parser, `neuma-tones` and `ScoreBuilder` all produce
  it, and `Score::to_gabc` writes it back. It holds the header fields and the syllables, each
  with its styled text and its notation (notes, clefs, bars, spaces, breaks), and the source
  span of everything.
- **Engraving** turns syllables into *segments*: a stretch of notation and the lyric under
  it, with geometry relative to the segment's own origin (ink pieces, the lyric's box, where
  the notes meet the vowel) and what may happen at the break after it. Neume construction,
  text measurement, vowel centering, hyphens, spacing inside a syllable, the initial and its
  annotations are all decided here.
- **Layout** places segments on lines. It is the only width-dependent stage: optimal-fit
  line breaking with GregorioTeX's penalties, justification, a clef at the start of each
  line, custodes, the hyphen of a word split across lines, and each line's height from its
  ink and lyrics. A `Layout` is an owned value that shares its engraving, so layouts at many
  widths can be kept at once.
- **Outputs** come from the layout: the SVG (whole, or in parts a line at a time), the
  display list, the timeline and the source map, which answers hit tests.

`neuma::Chant` is the front door: it owns the source, the score, the engraving and the
caches below, and its `update` engraves again only around an edit. The browser package, the
mobile bindings and the CLI all wrap a `Chant`.

## Units and coordinates

- **Staff spaces** measure the notation. One staff space is the distance between two
  adjacent staff positions, a line and the space beside it, so the staff's four lines are two
  staff spaces apart and a punctum is one staff space wide. This is half of what engravers
  (SMuFL, Gould's *Behind Bars*, GregorioTeX) call a staff space, the distance from one line
  to the next.
- **Staff positions** count those steps from the middle space: the lines are at −3, −1, 1
  and 3, positions grow upward, and GABC's `a` is −6 and `m` is 6.
- **Ems** measure text. Lyrics are sized in staff spaces (`StyleOptions::lyric_size`, 2.45 by
  default: GregorioTeX's 10 pt lyrics on its default staff), so text and notation scale
  together.
- **Glyph units** are the outlines' coordinates, 100 to a staff space.
- **Output units** are staff spaces times `LayoutOptions::scale` (6 by default): px in a
  browser, pt or dp on a phone. Layout breaks lines at `width / scale` staff spaces, so a
  text-size change is a new scale and a new layout, never a new engraving.

Every output shares one coordinate system: the origin is the layout's top left, y grows
downward, boxes are `x, y, w, h` from their top-left corner, and the one point given as a
center is named so (a timeline note's `cx, cy`). Source spans count UTF-8 bytes; the
bindings also give UTF-16 offsets, which JavaScript, Kotlin and Swift strings count.

## Caching and incremental update

Each change redoes only what depends on it, and every cache gives the same result as doing
the work afresh.

- **Reading.** A parse kept for editing records the reader's state after every syllable. For
  an edited source, the syllables read wholly before the edit are kept; reading starts again
  after them and stops as soon as, past the edit, a syllable ends where an old one ended with
  the same state. The rest is moved over with its source offsets shifted.
- **Engraving.** Engraving carries state from syllable to syllable (the clef, alterations in
  force, a pending break). The engrave cache keeps that state before every syllable, rolls
  the engraving back to the first syllable whose result could have changed, engraves until
  the state matches the old one again inside the unchanged end, and moves the rest over.
  Options that engrave the same (`ChantOptions` compared once sanitized) don't engrave again.
- **Sharing.** The engraving is behind an `Arc`, shared by every layout made from it. An
  update takes it back without a copy when no layout holds it; when one does (an editor
  always holds the layout on screen), the update copies the engraving's lists up to the edit
  and shares what follows, and each syllable's drawing is shared either way.
- **Line breaking.** Each line's candidate breaks depend only on its own segments, so the
  layout cache keeps them in a break table keyed on the width and the measure's numbers, and
  the rows of the breaker before an edit come out as last time. A line set from the same
  segments under the same conditions keeps its shape: only how far down the page it goes is
  worked out again.
- **Recent layouts.** A `Chant` remembers its last four layouts by width and sanitized
  options, enough for a main view and a thumbnail or two, and clears them on any change.
- **SVG.** `Layout::svg_parts_reusing(&previous, &options)` gives the SVG a line at a time,
  taking from the previous `SvgParts` the string of each line that draws the same. It keeps no state, so
  any number of views can each patch their own page.
- **Versions.** `Chant::version` names a chant's state: unique across every chant in the
  process and growing with each change that changed anything, so a view or cache keyed on it
  is never confused. An update that changes nothing keeps the version.

`tests/incremental.rs` checks the invariant: across the test corpora and random edits
(insertions, deletions, edits at either end, header and option changes), every update,
engraving, layout and SVG part equals what a fresh parse, engrave and layout give.

## Determinism

The same input, options and metrics table give byte-identical output everywhere:

- Layout and engraving use only `f32` addition, subtraction, multiplication, division and
  comparison. No `mul_add`, whose result differs between fused and unfused
  hardware, and no `powf`, `exp`, `sin` or other transcendental functions, whose results
  differ between libm builds. The breaker's badness is cubic, computed as `r * r * r`.
  `clippy.toml` disallows these methods, so clippy fails on any new use. (The pointer's
  confidence in `neuma-tones` uses `exp`: it only decides whether to report
  `point::unsure`, never which syllables are accented or what is drawn.)
- The breaker breaks ties explicitly (the earliest break wins), so iteration order never
  decides.
- SVG writes coordinates with exactly two decimals (glyph outlines in `<defs>` keep the
  table's three, and their scale four significant digits); rounding happens once, at the
  output, never between stages.
- Caches give the same result as fresh work (above), so output never depends on what was
  laid out before.

The golden snapshots and the incremental tests hold this in place, and every platform runs
the same Rust: the browser's WebAssembly and the apps' native libraries included.

## GABC coverage

The grammar follows Gregorio's documentation of GABC (GregorioRef, "The GABC File"),
written from the specification rather than Gregorio's source. Every token gets one of three
treatments:

- **Drawn**, as GregorioTeX draws it, give or take spacing.
- **Accepted** with an `info` diagnostic: it changes nothing visible, or only TeX output,
  or is drawn approximately (a fusion drawn unfused, a lyric tie drawn as a space).
- **Unsupported** with a `warning`, then recovery: what can be drawn is drawn, such as the
  bare pitches of a construct as puncta.

[diagnostics.md](diagnostics.md) lists each accepted and unsupported construct by its code.
Everything else in the format is drawn: headers (with neuma's recovery for a missing `;`),
styled lyrics and special characters, centering overrides, `<eu>` and `<nlba>`, every note
shape, liquescents, initio debilis, cavum, quadratum, alterations (soft and parenthesized),
morae, episemata, ictus marks, custodes, every space and bar (Dominican bars included), clef
changes and written line breaks with their custos suffixes.

**Line breaking.** Breaks fall between syllables and at spaces inside the notes. As in
GregorioTeX (`\gresetunbreakablesyllablenotes{10}{4}{4}`), a syllable of ten or more notes
may also break between its note groups, but not within four notes of either end, and at a
cost, so a syllable's end is preferred. `z` and `Z` force a break: `z` justifies the line it
ends and `Z` leaves it ragged, and `+` or `-` forces or suppresses its custos. Only
`<nlba>` regions and `!` before a space forbid a break. A line that can't end within the
width ends at its last forbidden break rather than running past it.

**Alteration scope.** GABC says where an alteration sign is drawn, not how long it lasts.
Gregorio's default is the line, Solesmes books use the word or the next bar, and Dominican
books differ again, so `AlterationScope { Line, Word, Bar, WordOrBar, Note }` is an option,
`WordOrBar` by default. It decides whether a soft alteration (`X`, `##`, `Y`) is drawn (only
when it changes the pitch in effect) and the timeline's semitones. A `cb` clef's flat is a
key flat on B at every octave until the next clef; an explicit natural cancels it within the
scope.

## Neumes and glyphs

The engraver splits each note group at its separators and classifies each run by contour and
shape into builders: single notes, pes, clivis, porrectus, torculus, scandicus, climacus,
salicus, inclinata runs and their compounds. Longer groups decompose left to right with the
fusion rules Gregorio documents. Each builder emits pieces, glyphs and rectangles (stems,
connecting lines, ledger lines, episemata), positioned in staff spaces. The builders follow
exsurge's `NeumeBuilder` (MIT), restructured rather than transliterated. Where exsurge has no
glyph, neuma composes one: a porrectus spanning more than a fifth is two puncta joined by a
line (with `engrave::wide-porrectus`), and a liquescent stropha is a scaled-down stropha.

The outlines come from exsurge's `Glyphs.js` (MIT), drawn from its author's Caeciliae font.
`tools/gen-glyphs` converts them once into `glyphs/table.rs`, which records the source
file's SHA-256, and normalizes every outline: every command becomes absolute `M`, `L`, `C`
or `Z`; holes become subpaths wound against their glyph's outline, so a nonzero fill leaves
them open; and coordinates are rounded to three decimals. Simple path parsers, such as ones
that handle only `M L H V C A Z`, read every glyph.

Glyphs cross every API as numeric ids with a lookup (`glyph_outline`), not as an enum, so the
glyph set can grow without breaking apps that match exhaustively on an enum. Gregorio's
greciliae font isn't a source: it is under the OFL, so a table of its outlines couldn't ship
under MIT and Apache.

## Display list and SVG

A `DisplayList` holds the layout's size, each line's box (top, bottom, staff center and lyric
baseline), the items to draw and the lyrics as plain text for an accessibility label. An
item is a glyph (id, position, scale), a rectangle, or text (position, baseline, size and
styled runs). Every item has a role (`Ink` for notation: staff, ledger, note, stem, bar,
episema, mora, ictus, accidental, clef, custos; `TextRole` for text: lyric, hyphen, initial,
annotation, rubric), and notation names the note or notes it draws.

The SVG writer maps roles to CSS classes (`neuma-note`, `neuma-staff`, `neuma-rubric`, …)
and fills with `currentColor`, so a page themes it with CSS, dark mode included, and lists
each piece of ink's notes in `data-note` for highlighting. Glyphs are defined once in
`<defs>` and used by reference. Their ids carry the scale, so scores drawn at different
scales never share one; two scores at one scale share ids (and identical definitions), so a
page that shows several gives each its own `SvgOptions::prefix` to keep ids unique. The SVG names the lyric font and asks for `font-variant-ligatures: none` and
`text-rendering: geometricPrecision` (below). It can come whole or in parts: a head, the
glyph definitions, one string per line and the rest (the initial).

## Lyrics and text measurement

- **Centering.** Notes sit over the syllable's vowel: the first note's center is over the
  center of the vowel nucleus, Gregorio's default. `{}` overrides it, and elisions, special
  characters and verbatim text count as consonants. A syllable without a vowel is centered
  as a whole. Vowel rules are data in Gregorio's vowel-file format; the crate ships the
  Latin and English rules Gregorio documents, and the `language:` header picks one.
- **Hyphens.** The engine draws them: one in the gap between two syllables of a word unless
  they touch, set right after the first syllable's text as GregorioTeX sets it, and one
  after the last syllable on a line when the word goes on. An author's own hyphen at either
  end of a syllable gets `gabc::hyphen-in-syllable`, since it would print twice.
- **Widths.** A syllable wider than its notes widens its segment; the gaps between words
  and syllables follow GregorioTeX's spacing.
- **Initials.** With the `initial-style:` header or `StyleOptions::initial`, the first letter
  becomes a drop cap spanning one to four staves, in a column before the staff, with the
  `annotation:` headers (or the mode) centered above it. Its size comes from the face's cap
  height, and its ink from small per-letter tables: how far a Q's tail or an accent reaches past the
  letter's box, so the capital clears the staff and the line below. The initial takes the
  marks after its letter (any Unicode mark), with room for an accent above it, or for two
  stacked. A letter outside Latin, Greek and Cyrillic, which the built-in faces lack and a
  fallback font draws, is given at least an em of width and a quarter em of room above.

`TextMeasure` is the engine's only text dependency. Widths are right only if every renderer
sets text the way it was measured, so neuma fixes the shaping:

- **No ligatures.** SVG output sets `font-variant-ligatures: none`. A native renderer does
  the same: `fontFeatureSettings = "'liga' 0"` on Android, a `.ligature: 0` attribute on iOS,
  and no extra tracking (`.kern`) on lyric runs. EB Garamond's ligatures would otherwise make
  "fi", "ff" or "ft" narrower than measured.
- **Real small caps.** `<sc>` text is measured with the face's `smcp` substitutions, and
  renderers use the font feature (`font-variant-caps: small-caps` on the web).
- **Kerning on.** The tables hold GPOS pair adjustments and legacy `kern`, which browsers,
  Core Text and Android apply to plain text.
- **Unrounded advances.** SVG output sets `text-rendering: geometricPrecision`. A browser that
  hints the face (Chrome on Linux and Windows) otherwise rounds each glyph's advance to a
  whole pixel, and since each syllable is its own `<text>`, touching syllables of a word come
  apart by an amount that changes with scale.
- **Faces.** A table holds a face per style it was built from. A style without a face (EB
  Garamond 12 has no finished bold) is measured with the regular face widened 3%, with a
  `text::synthetic-face` info; the display list keeps the style asked for, so a renderer can
  synthesize or substitute it.
- **Pinned.** A metrics table records the SHA-256 of each font file it was built from.
  neuma ships tables for the EB Garamond Google Fonts serves (the default) and for the EB
  Garamond 12 release; `neuma-metrics` builds one for any font.

A browser may instead measure with `canvas.measureText` through its own `TextMeasure`. Text
is then pixel-exact for that browser, but layouts are no longer identical across platforms.

## Timeline

`Layout::timeline` lists every note in singing order with its id, line, syllable, word, the
notehead's center and size, its source span, staff position, scale degree and semitones above
the clef's do (alterations applied per the scope), shape, liquescence, accent, and when it
starts and how long it lasts in weight units; and the pauses between notes, at bars and at a
psalm's mediant and flex, each before the note it names. Chant has no absolute pitch, so a
tool picks a key and adds an offset. Weights are relative durations, not beats: `Weights` has
one per sign (a plain note 1, a dotted note 2, an episema 1.5, pauses growing with the bar)
and a tool chooses the tempo. The timeline also marks the first note of each syllable,
inferred recitation (runs of three or more single-note syllables on one pitch), and verse and half-verse
counters, and gives the vowel the engine centered on.

## Source map and diagnostics

`Layout::source_map` lists every note, bar and syllable as drawn, with its source span and
its box on its line; a syllable split across lines has a box on each. It answers both ways:
`source_at(x, y)` and `note_at(x, y)` for a tap, and `elements_at(offset)` (or
`elements_at_utf16`) for a caret, most specific first. It is made once per layout and shared
by the layout's clones.

Parsing and engraving never fail. Each problem is a `Diagnostic` with a severity, a span, a
stable code and, where exactly one edit makes sense, a `Fix` that replaces a span. Codes are
stable and namespaced by stage (`gabc::`, `engrave::`, `text::`, `pointed::`, `apply::`,
`point::`, `book::`); messages are prose for people and may change. A test checks that
[diagnostics.md](diagnostics.md) lists every code the source can emit, and another that
every fix removes its diagnostic without losing notes.

## Psalm tones and pointing

Psalters point psalms by accent, with preparatory syllables, so `neuma-tones` takes pointed
text and a tone directly and needs no hand-written GABC.

- **Pointed text** is a verse per line: `*` the mediant, `†` the flex, `·` where the cadence
  starts, an acute on an accented syllable, `–` for a held note, `-` for a sung split, `\-`
  for a spelling hyphen, `[…]` for rubrics, and a verse number first. The parser accepts
  exactly this grammar and reports anything else as a `pointed::` diagnostic. `point` writes
  the canonical form back, and reading and writing it again gives the same text.
- **Tones** are small text blocks: an intonation, a tenor and a cadence for each half and the
  flex. A cadence is a formula of slots (`Slot`): an accented syllable, one unaccented
  syllable, or any number of unaccented syllables on one note. The Solesmes tones and their
  usual endings are built in, from jgabc's formulas.
- **Syllabification.** The cadence needs the syllables after each accent, and most cadence
  words are unhyphenated in a psalter. Splits come from the markup first, then an exception
  list, then a small set of English rules, with every piece holding a vowel.
- **Setting.** `psalm` fits each half-verse to its cadence (`apply::` diagnostics where the
  marks and the tone disagree) and builds a `Score` whose spans are bytes of the psalm text,
  so a `PsalmChant`'s timeline and hit tests answer in the text a reader sees. Each note
  knows its role in the tone: intonation, tenor, preparatory, accent or ending.
- **Automatic pointing.** A half-verse without marks is pointed by a small linear model over
  lexical stress (from the CMU Pronouncing Dictionary) and the shape of the cadence, fitted
  to a hand-pointed English psalter (not distributed). Its probability for the chosen
  pointing is its confidence; below 0.8 the half gets `point::unsure`. Marks written by hand
  are kept, so a correction survives pointing again.
- **The printed psalter.** Most often a psalm is shown as a pointed psalter prints it: the
  tone once as a line of notes (`Tone::gabc`, named by `Tone::label`) over the verses as text
  (`PsalmDisplay`), as styled runs with each syllable's place in the text and the tone.

## Booklets

`neuma-book` sets the chant of a service on fixed-size pages. A `.book` file lists settings
and then pieces: titles, headings, rubrics, text, scores and psalms. Composition sets each
piece into blocks (a staff line or a line of text each) with keep-together rules: a heading
or rubric stays with what follows, an antiphon with its psalm, and no staff is left alone at
a page break. Pagination adds running headers and page numbers. The PDF embeds and subsets
the text face (EB Garamond 12 when installed, any TrueType or OpenType font named, or the
standard Times faces), so its text can be searched and copied, and draws notation as paths;
SVG pages can draw text as paths too.

## Bindings

**Browser.** `neuma-wasm` exposes a `Chant` over a hand-written C ABI: strings cross as
UTF-8 in two buffers the module owns, so no generated glue is needed. `build.mjs` bundles
the JavaScript glue with the module inlined (gzipped, as base64) into one dependency-free ES
module, `dist/neuma.mjs`, which fetches nothing and works in sandboxed pages; a variant that
fetches `neuma.wasm` beside it is built too. The glue gives `Chant`, views and pages: a page
is a layout with its SVG, timeline and hit tests, answering for the score it shows for as
long as it is held. The engine keeps the layouts behind pages in two pools of the most
recently used, across every chant, view and page (by default 64 for pages showing their
chant as it is now and 2 for pages whose chant has changed, set with `setLayoutBudget`), and
a page whose layout was dropped lays itself
out again when next asked, to the same answers. Versions keep growing across a restart of
the engine. Features of the crate leave psalm tones, the pointer or a font table out for a
smaller module. TypeScript types ship beside the module.

**iOS and Android.** `neuma-mobile` wraps a `Chant` with UniFFI: `Chant.layout(width,
options)` returns a `ChantLayout` with the page to draw, the timeline and hit tests, each
answering for its own layout, so a thumbnail and the main view don't mix. Glyph outlines are
fetched once per id, and lyrics are drawn with the app's EB Garamond, ligatures off. Every
option record field has a default, and the engine's own rules apply to any value it can't
use. The crate's smoke test runs through the Python, Kotlin and Swift bindings.

Both bindings are thin: options, defaults and sanitizing are the core's, and names match
across Rust, JavaScript, Kotlin and Swift and the JSON (`neuma::json`, which the browser
package and the CLI print).

**Command line.** `neuma-cli` renders SVG, checks files (diagnostics as
`FILE:LINE:COL: SEVERITY: CODE: MESSAGE`, with fixes), prints the layout and timeline or a
library entry as JSON, lists tones, points and sets psalms, and builds booklets.

## Testing

- **Unit tests** in each module: the parser, every neume builder, alteration scopes,
  vowel rules, the tone grammar, the pointer, pagination and the PDF writer.
- **Golden snapshots** (`crates/neuma/tests/golden`): 22 reference scores laid out
  at 360 and 720 units wide, compared as text display lists. An intended change rewrites
  them (`UPDATE_SNAPSHOTS=1`) and the diff is reviewed with rendered images.
- **Incremental equivalence** (`tests/incremental.rs`): every cached path equals fresh
  work, across the corpora and random edits.
- **Diagnostics** (`tests/diagnostics.rs`): every code is documented, every span lands in
  the source, and every fix removes its diagnostic without losing notes.
- **Rendering** (`tests/render.rs`, `lines.rs`, `hit_boxes.rs`, `collisions.rs`): line
  breaking, line heights, hit boxes, and no ink colliding with lyrics or other ink. The
  `collisions` example of `neuma-metrics` runs the same check with the fonts' real outlines
  over any corpus at several widths.
- **Bindings**: the browser package's `test.mjs` and TypeScript check, and the mobile smoke
  tests, in CI.
- **Fuzzing** with cargo-fuzz: parse, layout and the GABC round trip.
- **GregoBase, nightly**: every score in GregoBase (about 18,800) through parsing,
  engraving, layout at three widths, the display list, timeline, SVG and a GABC round trip,
  failing on a panic, a slow file or an unstable round trip.

## Open questions

1. Whether English choirs want vowel centering other than Gregorio's English rules.
2. A core SVG mode that draws text as outlines, for exports that must look identical without
   the font. `neuma-book` already does this for booklets (`text-as-paths: yes`).
