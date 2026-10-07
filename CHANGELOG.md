# Changelog

## Unreleased

Editor support: source maps, diagnostics with fixes, SVG a line at a time, and faster
engraving and layout. Then the API for 0.1: one front door (`neuma::Chant`) that the
browser, mobile and command-line front ends wrap, and one name for each concept in Rust,
JavaScript, Kotlin/Swift and the JSON. The rendered output (SVG, display lists, timelines,
PDFs) is byte-identical to before; only names and shapes changed.

### The 0.1 API (breaking)

**Rust: `neuma`**

- New: **`Chant`**, an owned, `Send + Sync` score that keeps its engraving and its layout and
  SVG caches. `Chant::new(gabc)`, `Chant::with_options(gabc, ChantOptions)`,
  `Chant::from_score(score, source, options)`, `update(gabc)` (incremental),
  `diagnostics()` (parse and engrave, one list), `summary()`, `layout(width)` /
  `layout_with(width, &LayoutOptions)` (a repeat at the same width and options returns the
  kept layout), `svg_parts()` / `svg_parts_with(&SvgOptions)`, and hit tests on the last
  layout: `note_at(x, y)`, `source_at(x, y)`, `elements_at(offset, OffsetUnit)`.
  `ChantOptions` holds the lyric font, the `StyleOptions`, and optionally a custom
  `TextMeasure` (`with_measure`).
- Calls with options come in two forms: the plain one uses the defaults and `_with` takes
  them. `Engraving::layout(width, &opts)` is now `layout(width)` / `layout_with(width, &opts)`;
  `Layout::svg(&opts)` is `svg()` / `svg_with(&opts)`; `Layout::svg_parts(&opts)` is
  `svg_parts()` / `svg_parts_with(&opts)`; `DisplayList::svg(&opts)` likewise.
- `NoteMap` → **`Timeline`**, `MappedNote` → **`TimelineNote`**, `Layout::notes(&weights)` →
  **`Layout::timeline()`** / `timeline_with(&weights)`. `TimelineNote.x`/`y` → **`cx`/`cy`**
  (they are the notehead's center). New: `Timeline::note_at_time(t)` for a playhead.
- `Weights` fields `minima`, `minor`, `maior`, `finalis` → **`quarter`, `half`, `full`,
  `double`**, the names the bindings and pause kinds already used. `Layout::timeline_with`
  now applies the bindings' rule: a negative or non-finite weight keeps its default and
  none goes above `Weights::MAX` (1000); `Weights::sanitized` does it by hand.
- `Font` → **`LyricFont`**, `Font::table()` → `LyricFont::metrics()`, `Font::table_bytes()` →
  `LyricFont::metrics_bytes()`.
- `SourceMap::at(offset)` → **`SourceMap::elements_at(offset)`**; new `SourceMap::note_at(x, y)`
  (hit testing needs no `Weights`); new `OffsetUnit { Utf8, Utf16 }`.
- `Summary.range: Option<(i16, i16)>` → **`lowest`, `highest`**.
- Options and output structs, and enums that may grow, are `#[non_exhaustive]`: build options
  with `with_*` setters (`LayoutOptions::default().with_scale(8.0)`) instead of struct
  literals, and give matches on `Item`, `Ink`, `TextRole`, `PauseKind`, `BarKind`,
  `NoteShape`, `OfficePart`, `ElementKind` a `_` arm. `Severity` stays exhaustive.
  `Diagnostic::new(severity, span, code, message).with_fix(fix)` builds a diagnostic.
  `Initial::lines(n)` reads a staff count as the bindings and CLI do (0 or less is none, at
  most 4).
- A `LayoutOptions::scale` that isn't positive and finite now falls back to the default 6
  (it was 1); a lyric size that isn't is replaced by 2.45, everywhere.
- `#[must_use]` on the pure functions and builders.
- Re-exported at the root: `BarKind`, `NoteShape`, `TextStyle`, `MetricsError`,
  `OffsetUnit`. Removed from the root: `to_gabc` (use `Score::to_gabc`). Hidden:
  `decimal` (now private), `glyphs` (the generated table; `glyph_outline` stays), and the
  metrics builders `Face::new`, `Face::set_*`, `MetricsTable::to_bytes` (for
  `neuma-metrics`).
- New `json` feature: `neuma::json`, the JSON the browser package and the CLI print.

**Rust: `neuma-tones`**

- `apply_text(tone, text, &Options)` → **`psalm(text, tone, &PsalmOptions)`**, `apply(tone,
  &Pointed, …)` → `psalm_pointed(&Pointed, tone, …)`, `point_text(tone, text)` →
  **`point(text, tone)`**, `point(tone, &Pointed)` → `point_pointed(&Pointed, tone)`: the text
  comes first everywhere.
- `Options` → **`PsalmOptions`** (setters `with_intone`, `with_auto_point`,
  `with_strip_accents`, `with_name`); `no_auto_point` → **`auto_point`** (default `true`).
- `Setting` → **`PsalmSetting`**, `NoteRole` → **`PsalmNote`**, `Role` → **`ToneRole`**,
  `PartKind` → **`VersePart`**, matching the mobile bindings.
- `PsalmSetting.score`'s spans now count bytes of the psalm text (a note's is its sung
  syllable's, a bar's is empty at the end of its half-verse), so
  `Chant::from_score(setting.score, text, …)` hit-tests and times in the text.
- `ToneError` is an enum, `Unknown { name }` or `Invalid { reason }`, and
  `Tone::named` returns `Result<&Tone, ToneError>`.
- `UNSURE` is at the root; the modules are private (everything public is re-exported), and
  `point::training` and `syllable::syllables` are gone. `Pointed::parse` replaces
  `pointed::parse`.

**Rust: `neuma-book`.** `compose`, `paginate`, `pdf`, `svg` and `text` are private; the
`Document`, `Book` and font APIs are unchanged.

**Browser (`neuma.mjs`) and the JSON (`neuma notes`, `neuma info`)**

- Diagnostics, fixes and source elements: `from`/`to` → **`utf16Start`/`utf16End`**;
  `fix.insert` → **`fix.replacement`**.
- Timeline notes: `x`/`y` → **`cx`/`cy`**; `spanStart`/`spanEnd` → **`sourceStart`/
  `sourceEnd`**, with new `sourceUtf16Start`/`sourceUtf16End`.
- Library entries: `range: [lo, hi]` → **`lowest`, `highest`**.
- `tones()` → **`toneNames()`**; `psalm(…, { pointing: "manual" })` → `psalm(…, { autoPoint:
  false })`; `elementsAt(caret, { units })` → `elementsAt(caret, { unit })`. Psalm notes gain
  `utf16Start`/`utf16End`. New: `noteAtTime(timeline, t)`.
- The scale, weight, initial and lyric-size rules are the engine's; the glue no longer
  checks the scale itself. Laying out again at the same width and options reuses the layout.
- `neuma-wasm` (the Rust side) is a thin wrapper over `neuma::Chant`; its `json` module
  moved to `neuma::json`, and `Chant::layout_with` is `Chant::layout`.

**Mobile (UniFFI)**

- `Chant.update(gabc)`: engrave again after an edit, as in the browser.
- Every option record field has a default: `ChantOptions()`, `LayoutOptions()`,
  `Weights()`, `PsalmOptions()`. `defaultChantOptions()`, `defaultLayoutOptions()` and
  `defaultWeights()` are gone. Enum fields (`font`, `lastLine`, `intone`) are optional, null
  for the default, since the bindings can't give an enum field a default value.
- The lyric font default is now **Google Fonts' EB Garamond**, as in the browser and the CLI
  (it was EB Garamond 12). `ChantOptions.lyricSize` defaults to 2.45, `LayoutOptions.scale`
  to 6, and a value the engine can't use falls back the same way.
- Ids, indices, counts and offsets are signed `Int` (`Int32` in Swift), glyph ids
  included.
- `Page.notes`, `pauses` and `duration` → **`Page.timeline`** (`notes`, `pauses`,
  `duration`), null with `LayoutOptions(timeline = false)`. `Note` → **`TimelineNote`**, `x`/`y`
  → `cx`/`cy`, `spanStart`/`spanEnd` → `sourceStart`/`sourceEnd`, plus
  `sourceUtf16Start`/`sourceUtf16End`.
- `psalm(text, tone, intone)` → `psalm(text, tone, PsalmOptions(intone, autoPoint))`, likewise
  `psalmWithTone`; `PsalmNote` gains `utf16Start`/`utf16End`.

**Command line.** `neuma psalm --no-point` → `--no-auto-point`. `neuma-cli` no longer depends on
`neuma-wasm`. The JSON renames above apply to `neuma notes` and `neuma info`.

### Breaking changes (earlier in this release)

- **`neuma::Diagnostic`** has a new public field, `fix: Option<Fix>`. Code that builds a
  `Diagnostic` with a struct literal must add `fix: None` (or a fix).
- **`neuma::SvgOptions`** has a new field, `ids: bool` (default `true`). Struct literals need
  it, or `..SvgOptions::default()`.
- **`neuma::score::Header`** has a private field (each field's source span), so it can no
  longer be built with a struct literal: start from `Header::default()` and push to
  `fields`. Equality still compares `fields` only.
- **`neuma-wasm`** (the Rust side of the browser package): `Chant::summary_json` takes
  `&mut self`, as it now builds the summary on first use. `SvgOutput` has a new variant,
  `ChangedLines`.
- **`neuma-mobile`** (UniFFI): the `Diagnostic` record has new fields, `utf16Start`,
  `utf16End` and `fix`. Reading diagnostics is unaffected; Kotlin or Swift code that
  constructs one must pass them.
- **Diagnostics** now point at the source they are about (header fields, tags, the hyphen in
  a syllable) rather than at empty or approximate spans, and there are new codes
  (`gabc::no-clef`, `gabc::unclosed-tag`). `engrave::final-break` is retired: no score could
  reach it. See [docs/diagnostics.md](docs/diagnostics.md).

### Added

- `Layout::source_map()`: what is drawn where, and the source of each note, bar and
  syllable, both ways (`SourceMap::source_at`, `SourceMap::elements_at`); `Utf16Index` for
  editors' offsets.
- `Fix` on diagnostics where one edit makes sense, with `Fix::apply`.
- `Layout::svg_parts()`: the SVG as a head, definitions and one string per line, so a page
  can replace only the lines an edit changed.
- `EngraveCache`, `Engraving::layout_cached` with `LayoutCache`, and
  `Layout::svg_parts_cached` with `SvgCache`: after an edit, engraving, line breaking and
  each line's SVG are redone only where the edit could change them, with the same result as
  doing them afresh. `neuma::Chant` uses them, and the browser package passes only the lines
  that changed from the engine to the page.
- Browser package: `Chant.update`, `sourceAt`, `elementsAt`, `layout(…, { timeline: false,
  svg: "lines", ids: false })`, and UTF-16 offsets and fixes on diagnostics; an example
  editor in `crates/neuma-wasm/examples/editor.html`.
- Mobile bindings: `Chant.sourceAt`, `Chant.elementsAt`, UTF-16 offsets and fixes.
