# Changelog

## Unreleased

Editor support: source maps, diagnostics with fixes, SVG a line at a time, and faster
engraving and layout. Then the API for 0.1: one front door (`neuma::Chant`) that the
browser, mobile and command-line front ends wrap, owned layouts that answer their own hit
tests, and one name for each concept in Rust, JavaScript, Kotlin/Swift and the JSON.

What renders is byte-identical to before for the same options (SVG, display lists,
timelines, PDFs). Some behavior did change:

- A `LayoutOptions::scale` that isn't positive and finite now falls back to the default 6
  (it was 1), and a lyric size that isn't to 2.45, in every front end.
- The mobile bindings' lyric font default is now Google Fonts' EB Garamond, as in the browser
  and the CLI (it was EB Garamond 12).
- Hit tests answer for the layout asked, not for whichever layout ran last: a thumbnail laid
  out after the main view no longer changes what a tap on the main view finds.
- The browser package makes a page's timeline only when asked (`page.timeline()`), and the
  mobile `ChantLayout` makes its timeline when asked.
- A page answers for the score it shows for as long as it is kept, whatever the chant has
  become since, in Rust, the browser and on mobile. `update` with the current source, or
  `set_options` with options that engrave the same, changes nothing and says so.
- While a layout is held, the chant's next edit copies the engraving rather than changing
  it in place. An editor always holds the layout on screen, so this is part of an edit's
  cost: about 1.7 ms of a 5 ms update on the longest scores (4,900 notes), next to nothing on
  typical ones. A layout kept past an edit keeps a whole engraving alive; dropping or freeing
  the layouts a view has replaced bounds memory, though it doesn't make edits cheaper.
- Weights follow one rule everywhere: a negative or non-finite weight keeps its default, and
  none goes above 1000.
- `neuma book` reports `point::unsure` at the half-verse's syllables rather than the whole
  verse.
- Pointed text keeps a spelling hyphen (`\-`, "blood\-guiltiness"): `point` writes it back
  (it wrote "bloodguiltiness"), and `neuma book` prints "blood-guiltiness" in a pointed
  psalm's verses. A `·` the pointer puts right after one is written `\-·` ("pre\-·eminence";
  it was "pre·eminence"). Each piece of such a word is a syllable with its own span, where
  every piece had the whole word's. The sung setting (`psalm`'s GABC, a book's chant
  verses) is unchanged.
- `neuma book` labels a pointed psalm's tone with `Tone::label()`: "Tone 8 G" (it printed
  "Tone 8.G") and "Tonus peregrinus" (it printed "Tone per").
- A verse number after rubrics that open the line ("[Stand.] 5 For I …") is the verse
  number; it was sung as a word. `point` writes it first ("5 [Stand.] For I …").
- A `·` among a half-verse's leading dashes ("* – · – – práise") stays where it was written
  in `point`'s text and a book's verses, which moved it to the end ("* – – – · práise"). How
  the half is sung is unchanged.
- A hyphen with the `·` after it ("pre-·eminence") is a spelling hyphen where the cadence
  starts, as in printed psalters, and is printed; it was a dotted split that dropped the
  hyphen. `·-` ("hon·-our") is still a dotted split. The sung setting is unchanged.
- A hyphen at either end of a word, or a second one in a row (`\-dashes\-`, `c\-\-d`), is
  kept as text in `point`'s text and a book's verses (it was dropped), and a `-` without a
  letter on both sides is text rather than a sung split (`x-,` sang `,` as a syllable of its
  own).
- Rubrics written before a half-verse's leading dashes ("* [Sit.] – · – and") stay before
  them in `point`'s text and a book's verses; they moved after them.
- Pointed text reads any Unicode space between words, U+00A0 among them, drops U+2060
  (word joiner), and reads `-` and U+2060, or U+2011, as a spelling hyphen: a line copied from
  a psalm display reads as the text it came from. A no-break space used to be read as part of
  a word, so held dashes were sung as words.

### The 0.1 API (breaking)

**Rust: `neuma`**

- New: **`Chant`**, an owned, `Send + Sync` score that keeps its engraving and the caches
  that make the next edit and layout cheap. `Chant::new(gabc)`,
  `Chant::with_options(gabc, ChantOptions)`, `Chant::from_score(score, source, diagnostics,
  options)`, `update(source)` and `update_score(…)` (incremental), `set_options(ChantOptions)`
  (engraves again only if the options engrave differently), each returning whether anything
  changed, `version()` (names the state: unique across every chant in the process, growing
  with each change that changed anything), `diagnostics()` (reading and engraving, one list), `summary()`, and
  `layout(width)` / `layout_with(width, &LayoutOptions)` through `&self` (it remembers its
  last few layouts, keyed on the sanitized options). `ChantOptions` holds the lyric font, the `StyleOptions` (with a
  setter for each of its fields), and optionally a custom `TextMeasure` (`with_measure`).
  Its `Debug` leaves out the engraving and caches.
- **`Layout`** is owned (`'static`), cheap to clone and `Send + Sync`: it shares its
  engraving through an `Arc`. Hit tests live on it: `note_at(x, y)`, `source_at(x, y)` and
  `elements_at(byte_offset)` / `elements_at_utf16(caret)`, over `source_map()`, which is made
  once per layout and shared by its clones. `Layout::utf16()` is the source's UTF-16 index,
  for layouts a `Chant` made.
- New: **`Layout::svg_parts_reusing(&previous, &SvgOptions)`**, the SVG in parts with each
  line that draws as one of `previous`'s taking its string, and saying which in
  `SvgLine::reused_from`. It keeps no state, so any number of views can each patch their own
  page, and it reads only what the engine wrote into `previous`, never its public fields, so
  a caller may drain or change them (`reused_from` is in the engine's numbering of the
  previous lines, as returned). `SvgLine.svg` is an `Arc<str>`, shared with that copy.
  Parts carry what reuse needs, about two thirds again the memory of the line strings: keep
  only the parts a page shows.
- `Score::engrave` returns an **`Arc<Engraving>`**, and `Engraving::layout` takes one, so
  layouts share it; `Chant::update` takes it back without a copy once no layout holds it.
- Calls with options come in two forms: the plain one uses the defaults and `_with` takes
  them. `Engraving::layout(width, &opts)` is now `layout(width)` / `layout_with(width, &opts)`;
  `Layout::svg(&opts)` is `svg()` / `svg_with(&opts)`; `Layout::svg_parts(&opts)` is
  `svg_parts()` / `svg_parts_with(&opts)`; `DisplayList::svg(&opts)` likewise.
- `NoteMap` → **`Timeline`**, `MappedNote` → **`TimelineNote`**, `Layout::notes(&weights)` →
  **`Layout::timeline()`** / `timeline_with(&weights)`. `TimelineNote.x`/`y` → **`cx`/`cy`**
  (they are the notehead's center). `TimelineNote.weight` (the same as `duration`) and
  `quilisma` (the same as `shape == Quilisma`) are gone; `staff_position`, `semitones` and
  `half` are `i32`/`u32` like the other numbers, as are `Summary`'s pitches. `Pause.weight`
  → **`duration`**. `Timeline::note_at(x, y)` is gone: hit testing is the layout's. New:
  `Timeline::note_at_time(t)` for a playhead.
- `BarKind` speaks English, as the JSON, the bindings and `Weights` do: `Minima`, `Minor`,
  `Maior`, `DottedMaior`, `Finalis` → **`Quarter`, `Half`, `Full`, `DottedFull`, `Double`**.
- `Weights` fields `minima`, `minor`, `maior`, `finalis` → **`quarter`, `half`, `full`,
  `double`**. `Weights::sanitized` applies the rule above by hand.
- `Font` → **`LyricFont`**, `Font::table()` → `LyricFont::metrics()`, `Font::table_bytes()` →
  `LyricFont::metrics_bytes()`. The metrics table's `Face` → `FaceMetrics`, so the one
  `Face` is the text style's.
- `SourceMap::at(offset)` → **`SourceMap::elements_at(offset)`**; new `SourceMap::note_at(x, y)`.
  `SourceMap.lines` are `LineBox`es (top, bottom, staff, baseline), and `Element.cx`, a
  note's notehead center, is public (and in the JSON as `cx`).
- `Summary.range: Option<(i16, i16)>` → **`lowest`, `highest`**.
- The score model (`Score`, `Header`, `Syllable`, `Lyric`, `LyricRun`, `Note`, `Clef`, `Bar`,
  `Episema`) is `#[non_exhaustive]`, with constructors: `Score::new`, `Header::new` and
  `push`, `Syllable::new`, `Lyric::new`, `LyricRun::new`, `Note::new`, `Clef::new`,
  `Bar::new`, `Episema::new`, and `ScoreBuilder`.
- Options and output structs, and enums that may grow, are `#[non_exhaustive]`: build options
  with `with_*` setters (`LayoutOptions::default().with_scale(8.0)`) instead of struct
  literals, and give matches on `Item`, `Ink`, `TextRole`, `PauseKind`, `BarKind`,
  `NoteShape`, `OfficePart`, `ElementKind` a `_` arm. `Severity` stays exhaustive.
  `Diagnostic::new(severity, span, code, message).with_fix(fix)` builds a diagnostic.
  `Initial::from_staves(n)` reads a staff count as the bindings and CLI do (0 or less is
  none, at most 4).
- `#[must_use]` on the pure functions and builders.
- Re-exported at the root: `BarKind`, `NoteShape`, `TextStyle`, `MetricsError`. Removed from
  the root: `to_gabc` (use `Score::to_gabc`; `gabc::to_gabc` is private). Private now:
  `EngraveCache`, `LayoutCache`, `Engraving::layout_cached` (a `Chant` keeps them) and
  `decimal`. `SvgCache` is gone (see `svg_parts_reusing`). Hidden: `glyphs` (the
  generated table; `glyph_outline` stays), the metrics builders `FaceMetrics::new`,
  `FaceMetrics::set_*`, `MetricsTable::to_bytes` (for `neuma-metrics`), and
  `json::{string, number, span, source_span}` (for the bindings).
- New `json` feature: `neuma::json`, the JSON the browser package and the CLI print.

**Rust: `neuma-tones`**

- `apply_text(tone, text, &Options)` → **`psalm(text, tone, &PsalmOptions)`**,
  `point_text(tone, text)` → **`point(text, tone)`**: the text comes first.
  `PsalmSetting.text` is the text it was set from.
- New: **`PsalmChant`**, psalm text set to a tone and engraved with its spans in the text
  and its diagnostics (such as `point::unsure`) as the chant's:
  `PsalmChant::new(text, &tone, &PsalmOptions, ChantOptions)`, `update(text)` (sets new text
  to the same tone; returns whether it changed), `set_options`, `notes()`, `gabc()`,
  `setting_diagnostics()`, `tone()`, `psalm_options()`, and the `neuma::Chant` it derefs to
  (`chant()`, `into_chant()`). It keeps the setting's score only as its chant's.
- New: **`AnyChant`**, `Gabc(Chant)` or `Psalm(PsalmChant)`, for an app or binding that
  takes either: it derefs to `Chant`, and its `update` and `set_options` read the new source
  as the chant was made from. The browser and mobile bindings use it.
- `Options` → **`PsalmOptions`** (setters `with_intone`, `with_auto_point`,
  `with_accents`, `with_name`); `no_auto_point` → **`auto_point`** (default `true`);
  `strip_accents` → **`accents`**: `Accents::All` (the default), `None`, or `OutsideFlex`
  (none in a flex, as a pointed psalter prints it), also in the browser (`accents: "all" |
  "none" | "outsideFlex"`) and on mobile.
- `Setting` → **`PsalmSetting`**, `NoteRole` → **`PsalmNote`** (its `source` → **`span`**),
  `Role` → **`ToneRole`**, `PartKind` → **`VersePart`**, matching the mobile bindings.
- `PsalmSetting.score`'s spans now count bytes of the psalm text (a note's is its sung
  syllable's, a bar's is empty at the end of its half-verse).
- `Pointing` is `{ text, halves, diagnostics }`: the pointed text as a field (it was the
  `text()` method beside `Pointed::to_text()`), and each `HalfPointing` has its `span` in the
  text. The parsed `Pointed` model (`Verse`, `Part`, `Joint`, `Syllable`), `apply`/
  `psalm_pointed` and `point`/`point_pointed` on it are private; `VersePart` stays.
- `ToneError` is an enum, `Unknown { name }` or `Invalid { reason }`, and
  `Tone::named` returns `Result<&Tone, ToneError>`.
- `UNSURE` is at the root; the modules are private (everything public is re-exported), and
  `point::training` and `syllable::syllables` are gone.

**Rust: `neuma-book`.** `compose`, `paginate`, `pdf`, `svg` and `text` are private; the
`Document`, `Book` and font APIs are unchanged.

**Browser (`neuma.mjs`) and the JSON (`neuma notes`, `neuma info`)**

- Pages: `chant.layout(width, options)` returns a **`Page`**: `width`, `height`, `svg` or
  `svgParts`, `timeline()` (made on the first call), and its own `noteAt(x, y)`,
  `sourceAt(x, y)` and `elementsAt(caret, { unit })`, which answer for the score that page
  shows for as long as it is held, whatever the chant has laid out or become since (and
  after `chant.free()`); `page.source` is the source it shows. No page throws for its age:
  the engine keeps the layouts behind pages in a cache of the most recently used (4 by
  default, across every chant, view and page; **`setLayoutBudget(count)`**), and a page
  whose layout was dropped, or freed with `page.free()` (or `Symbol.dispose`), lays itself
  out again when next asked, from its chant's source and options at the time, to the same
  answers. A `FinalizationRegistry` only frees a collected page's layout sooner. New:
  **`engineStats()`**, `{ memory, layouts, budget }`. `page.stale` says the chant has changed since the page was laid
  out, and `page.version` is the chant's version it was laid out at. `chant.layout` makes a
  new page on each call. `chant.noteAt`, `sourceAt` and `elementsAt`, and
  `layout(…, { timeline: false })`, are gone.
- New: **views**, for a place that shows the score and lays it out again on each change:
  `const view = chant.view(options)`, then `view.layout(width, { weights })`. A view
  returns the same page for the same width and weights while nothing changed (its current
  page, `view.page`, or the one before), and in parts reuses its last page's lines.
- `chant.free()` is a hint too: a freed chant engraves its source again when next used, as
  the same state. After the engine restarts (`init()` after an internal error), chants,
  views and pages made before carry on.
- `update` and `setOptions` return whether anything changed; `chant.version` names the
  chant's state, unique across chants; `chant.source` is the source it was last given.
- New: `Chant.fromPsalm(text, tone, { intone, autoPoint, …chantOptions })`, with
  `chant.psalm` (the setting, `{ gabc, notes, diagnostics }`, as `psalm` returns it) and
  `update(text)` setting new text to the same tone;
  `chant.setOptions(options)`; `noteAtTime(timeline, t)`.
- Diagnostics, fixes and source elements: `from`/`to` → **`utf16Start`/`utf16End`**;
  `fix.insert` → **`fix.replacement`**. Source elements gain `cx`.
- Timeline notes: `x`/`y` → **`cx`/`cy`**; `spanStart`/`spanEnd` → **`sourceStart`/
  `sourceEnd`**, with new `sourceUtf16Start`/`sourceUtf16End`; `quilisma` is gone (it is
  `shape`). Pauses: `weight` → **`duration`**.
- Psalm notes: `start`/`end` → **`sourceStart`/`sourceEnd`**, with
  `sourceUtf16Start`/`sourceUtf16End`, as the timeline names a note's source. Pointed halves
  gain the same four.
- Library entries: `range: [lo, hi]` → **`lowest`, `highest`**.
- `tones()` → **`toneNames()`**; `psalm(…, { pointing: "manual" })` → `psalm(…, { autoPoint:
  false })`.
- The scale, weight, initial and lyric-size rules are the engine's; the glue no longer
  checks the scale itself.
- `neuma-wasm` (the Rust side) is a thin wrapper over `neuma::Chant`; its `json` module
  moved to `neuma::json`, and `Outputs` is gone.

**Mobile (UniFFI)**

- `Chant.layout(width, options)` returns a **`ChantLayout`** object: `page()` (what to draw,
  made once), `timeline(weights)` (a **`ChantTimeline`**; the names keep clear of SwiftUI's
  `Layout` and WidgetKit's `Timeline`), `noteAtTime(t, weights)` (keeping the timeline between calls),
  `noteAt`, `sourceAt` and `elementsAt`. Each answers for its layout, and a tap no longer
  waits on a layout in progress. `Page.timeline`, `LayoutOptions.timeline` and
  `LayoutOptions.weights` are gone, as are `Chant.noteAt`, `sourceAt` and `elementsAt`.
- New: `Chant.update(src)` (engrave again after an edit), `Chant.setOptions(options)` (a new
  text size), `Chant.version()` (names the state, unique across chants, to key a view on),
  and `Chant.fromPsalm(text, tone, PsalmOptions, ChantOptions)` with `psalm()`, the setting
  as `psalm` returns it.
- `ToneException`'s message is the error's ("no built-in tone 9.z").
- Every option record field has a default: `ChantOptions()`, `LayoutOptions()`,
  `Weights()`, `PsalmOptions()`. `defaultChantOptions()`, `defaultLayoutOptions()` and
  `defaultWeights()` are gone. Enum fields (`font`, `lastLine`, `intone`) are nullable, null
  meaning the default each field names, since the bindings can't give an enum field another
  default.
- `ChantOptions.lyricSize` defaults to 2.45, `LayoutOptions.scale` to 6, and a value the
  engine can't use falls back the same way.
- Ids, indices, counts and offsets are signed `Int` (`Int32` in Swift), glyph ids
  included.
- `Note` → **`TimelineNote`**, `x`/`y` → `cx`/`cy`, `spanStart`/`spanEnd` →
  `sourceStart`/`sourceEnd`, plus `sourceUtf16Start`/`sourceUtf16End`; `quilisma` is gone.
  `Pause.weight` → **`duration`**. `SourceElement` gains `cx`.
- `psalm(text, tone, intone)` → `psalm(text, tone, PsalmOptions(intone, autoPoint))`, likewise
  `psalmWithTone`; `PsalmNote`'s range is `sourceStart`/`sourceEnd` and
  `sourceUtf16Start`/`sourceUtf16End`, and `HalfPointing` gains the same.

**Command line.** `neuma psalm --no-point` → `--no-auto-point`. `neuma-cli` no longer depends on
`neuma-wasm`. The JSON renames above apply to `neuma notes` and `neuma info`.

- Diagnostics have one form in every command: `FILE:LINE:COL: SEVERITY: CODE: MESSAGE`,
  with the fix, if there is one, on the next line. `FILE` is `<stdin>` for stdin, and in
  `neuma book` the piece's file (or the book and the piece's number). It was
  `LINE:COL: severity[code] at START..END: message` from `check`, `psalm` and `point`, and
  `BOOK: piece N: severity[code] at START..END: message` from `book`, whose positions are
  now a line and column in the piece's own text. `point`'s unsure half-verses are
  `point::unsure` diagnostics with a column. The column counts characters and skips a
  byte-order mark at the start of the file.
- `neuma check` takes any number of files, sorts each file's diagnostics by position, and
  exits 1 if any file has an error and 2 if a file can't be read. It no longer takes
  `--width`, `--scale` or `--max-lines`, which didn't change its diagnostics.
- `--help`, `-h` and `--version`, alone or after any command, answer without reading stdin
  (`neuma --help` used to wait for it). `--version` is new.
- Arguments are checked: an unknown command or flag, a flag that belongs to another
  command (`neuma info --width 500`), a flag without its value, a second file for a command
  that takes one, or an argument to `neuma tones` is an error with the usage and exit status
  2. They used to be ignored, taken for a file name, or read as stdin. `--` ends the
  options, and `neuma book -` reads the book from stdin (with `-o`).
- `--width` takes a number above 0 and at most 1000000, and `--scale` above 0 and at most
  1000; anything else is an error. A value that didn't parse used to keep the default
  silently, a zero, negative or NaN one was laid out as the narrowest column or the default
  scale, and a huge scale wrote `inf` into the SVG.
- A write error on stdout, such as a full disk, is reported with exit status 2; it used to
  be ignored. A closed pipe still ends the output quietly, and `check` stops there. `neuma
  book` exits 2, not 1, when it can't write the PDF or an SVG page.
- An argument that isn't valid UTF-8 is a usage error rather than a panic.

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
- Incremental engraving, line breaking and SVG: after an edit, `neuma::Chant` redoes each
  only where the edit could change it, with the same result as doing it afresh, and the
  browser package passes only the lines that changed from the engine to the page.
- Browser package: `Chant.update`, hit tests, `layout(…, { svg: "lines", ids: false })`, and
  UTF-16 offsets and fixes on diagnostics; an example editor in
  `crates/neuma-wasm/examples/editor.html`.
- Mobile bindings: hit tests for editors, UTF-16 offsets and fixes.
- A pointed psalter's display, for apps that show a psalm as text under its tone rather
  than in chant over every verse: `Tone::gabc()` (the tone as one line of notes with no
  words, moved from `neuma-book`, whose output is unchanged), and `PsalmDisplay` in
  `neuma-tones`, the text pointed verse by verse as styled runs (syllables with their place
  in the text and the tone, the `·`, `–`, `*` and `†` marks, rubrics), with automatic
  pointing and `point::unsure` diagnostics as `psalm` gives them. In the browser,
  `psalmDisplay(text, tone, options)` and `Chant.fromTone(tone, options)`, with an example
  page, `crates/neuma-wasm/examples/psalm.html`; on mobile, `psalmDisplay(text, tone,
  PsalmOptions)` returning records (a run's kind a sealed class in Kotlin and an enum with an
  associated value in Swift) and `Chant.fromTone(tone, ChantOptions)`. A line's runs use
  U+00A0 between a mark and its syllable, and U+2060 after each `–` and spelling hyphen, so
  it never breaks there; pointed text reads both back, so a line copied from the display sets
  as its source. The browser's psalm options throw on an unknown value.
  `Tone::label()` names a tone as a psalter prints it ("Tone 8 G", "Tonus peregrinus";
  `toneLabel(tone)` and `toneLabel` on the display in the browser and on mobile).
