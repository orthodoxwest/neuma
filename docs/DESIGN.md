# Neuma: design proposal

Status: approved, 2026-10-05; M1 implemented. Repository:
`orthodoxwest/neuma`.

Neuma is a Rust engine that turns GABC, or pointed psalm text plus a psalm
tone, into engraved square-note chant. It runs natively on the server, in the
iOS and Android apps through the Office's existing UniFFI core, and in the
browser as WebAssembly. It is built for the AWRV's English repertoire first,
without closing the door on Latin.

## 1. Goals and non-goals

### Goals

1. **One engine, every front.** The same layout code runs on office-web
   (server-rendered SVG), in the browser (WebAssembly reflow), and in the
   native apps (a display list that Swift and Kotlin draw). For the same
   width, options and metrics table, all three produce the same line breaks
   and coordinates.
2. **Reflow.** Layout runs at the actual column width and is fast enough to
   redo on every resize, rotation or text-size change. Parsing and neume
   construction happen once; only line breaking is redone.
3. **Platform-neutral output.** The engine never draws. It returns a display
   list of positioned glyphs, rectangles and text runs. An SVG writer ships
   with it, and the apps map the list onto their own canvases.
4. **Practice support.** Every note in the output carries its pitch, duration
   weight, position and source span, so a practice or playback tool can drive a cursor,
   play a guide melody and score a singer from the same layout the student
   sees.
5. **Psalmody without hand-written GABC.** Pointed text plus a tone becomes a
   score through the same model (section 15).
6. **Robustness.** No input panics. Malformed input returns diagnostics with
   source spans, and the engine renders what it can.
7. **Deterministic and testable**, under the rules in section 13.
8. **Permissive license.** MIT OR Apache-2.0, so the Office, practice tools,
   and anyone else can vendor it.

### Non-goals for v1

- Replacing GregorioTeX for print. The Office's print editions keep Gregorio.
- An editor UI. Live preview works because layout is fast, but editing UI
  belongs to consumers.
- NABC (adiastematic neumes), polyphony, modern notation, multi-voice scores,
  and staves other than four lines. A `staff-lines:` value other than 4 gets a
  diagnostic and renders on four lines.
- Rasterizing. That's left to the consumer (resvg in tests).

## 2. Consumers and what each needs

| Consumer | Where it runs | Needs |
|---|---|---|
| office-web | Axum server, Rust | Inline SVG for first paint, no-JS readers and print, sized so the client relayout doesn't shift the page (section 16). Themed through CSS (Nave/Apse, dark mode). |
| Office web client | Browser; `app.js` is a classic deferred script | Relayout at the real column width, swapping SVG in place. WebAssembly loaded by dynamic `import()` only on pages with chant, and precached by `sw.js`. |
| Office iOS/Android | `mobile-ffi` (UniFFI) | A display list at the device's width and text size, through a per-score handle. Glyph outlines once each, as absolute `M/L/C/Z` path data, because iOS's `SVG.swift` parses only `M L H V C A Z` and silently stops at anything else. |
| Practice or playback tool | Browser (WebAssembly) | The same SVG, plus a note map: pitch, weight, x/y, line and source span per note, and which SVG element is which note, for highlighting. |
| Psalm-tone pointer | Library, CLI and browser | Pointed psalm text plus a tone, turned into a score. Writes the same markup back out for print. |
| Tooling (CLI, CI) | Native | Lint GABC (hyphens inside syllables, as in Psalm 134), render SVG/PNG for review, dump note maps as JSON. |

## 3. Package layout

```
neuma/
  Cargo.toml                 workspace; edition 2024, MSRV 1.94 (matches the Office)
  LICENSE-MIT  LICENSE-APACHE
  NOTICE                     glyph outlines: exsurge (MIT, (c) Fr. Matthew Spencer, OSJ), drawn
                             from his Caeciliae font; vowel tables: Gregorio documentation
  crates/
    neuma/                   core: parse, resolve, engrave, break lines, output. No I/O, no
                             platform calls, #![forbid(unsafe_code)], no required dependencies
      src/
        lib.rs               public API (section 5)
        gabc/                lexer, parser, AST with byte spans, diagnostics, writer (to_gabc)
        score.rs             resolved model + ScoreBuilder: syllables, notes, staff positions
        neume.rs             groups notes into neume shapes (section 7)
        glyphs/              generated outline table, u16 glyph ids (section 8)
        text.rs              lyric markup, vowel finding from vowel-rule data (section 10)
        engrave.rs           width-independent stage: units with ink and lyric boxes
        lines.rs             optimal-fit breaking, justification, per-line clef and custos
        display.rs           display list (section 9)
        notes.rs             note map (section 12)
        metrics.rs           reader for prebuilt metrics tables (implements TextMeasure)
        svg.rs               SVG writer (feature "svg", default on)
      vowels/                la.vowels, en.vowels in Gregorio's vowel-file format
      fonts/                 built-in EB Garamond metrics tables (feature "fonts", default on)
    neuma-metrics/           builds metrics tables from TTF/OTF (ttf-parser), pinned to the font
                             files' hashes; can also measure from font files at runtime
    neuma-tones/             psalm tones as data, the pointed-text parser and writer,
                             tone + pointed text → Score (section 15)
    neuma-wasm/              browser package: a hand-written C ABI and one ES module with
                             the wasm inlined
    neuma-mobile/            UniFFI bindings for Swift and Kotlin
    neuma-cli/               `neuma render|check|notes|fmt|point`
  tools/
    gen-glyphs/              exsurge Glyphs.js → normalized glyphs/table.rs (checked in)
  tests/
    corpus/                  real scores with provenance (section 14), per-token fixtures
    golden/                  expected SVG and note-map JSON per fixture and width
  fuzz/                      cargo-fuzz targets for the parser and full pipeline
```

The core crate has no required dependencies. `neuma-metrics` pulls in
`ttf-parser`, but only for building tables or measuring from font files at
runtime. Consumers that load a prebuilt table use the core's own reader.

**Office vendoring.** The Office depends on `neuma`, `neuma-tones` and the
metrics table as git dependencies pinned to a tag (crates.io later). Inside
the Office, a new `crates/render-chant` is the only crate that knows about
neuma (section 16).

## 4. Pipeline

```
GABC text ── parse ──► Ast ── resolve ──┐
                                        ├──► Score ── engrave(metrics, style) ──► Engraving
pointed text + tone ── neuma-tones ─────┘         (width-independent; cache it)
                                                       │
                                                       └── layout(width, opts) ──► Layout
                                                                ├── display() ──► DisplayList
                                                                ├── svg() ──────► String
                                                                └── notes() ────► NoteMap
```

- **Score** is the one model. The GABC resolver, `neuma-tones` and
  `ScoreBuilder` all produce it, and `Score::to_gabc()` writes it back out.
- **Engrave** builds a sequence of *units*, with geometry relative to each
  unit's own origin: ink extent, lyric box, the anchor where notes meet the
  vowel, and break opportunities with their penalties after the unit (and,
  for recitations, inside it).
- **Layout** places units on lines. It's the only width-dependent stage, so a
  resize skips parsing and neume construction.

### Units and coordinates

Engraving geometry is in **staff spaces**. One staff space (1.0) is the
distance between adjacent staff positions, a line and the next space, so
staff lines are 2.0 apart. The four lines sit at positions −3, −1, 1 and 3,
and y grows downward. `LayoutOptions::scale` sets the output size of one staff
space in px or pt, and the display list comes out in output units, already
scaled. Lyrics are sized in staff spaces too, so text and notation scale
together. A text-size change only changes `scale` and reruns line breaking,
because the same width then holds fewer staff spaces.

## 5. Public API sketch

```rust
pub fn parse(src: &str) -> Parsed;                      // never fails; carries diagnostics
pub struct Parsed { pub score: Score, pub diagnostics: Vec<Diagnostic> }
pub struct Diagnostic { pub severity: Severity, pub span: Range<usize>, pub code: &'static str, pub message: String }

pub trait TextMeasure {
    /// Advance of `text` in `style`, in ems, with the shaping the renderers use (section 11).
    fn advance(&self, text: &str, style: TextStyle) -> f32;
    /// Ascent and descent in ems for `style`'s face.
    fn vertical(&self, style: TextStyle) -> (f32, f32);
    /// Whether this measurer has a real face for `style` (section 11).
    fn has_face(&self, style: TextStyle) -> bool;
}
pub struct MetricsTable { /* deserialized from bytes; implements TextMeasure */ }

pub struct StyleOptions {
    pub lyric_size: f32,               // lyric font size, in staff spaces
    pub initial: Initial,              // None | Lines(n): drop cap height in staff lines, fixed at engrave time as in GregorioTeX
    pub annotation: bool,              // show header annotation(s) above the initial
    pub spacing: Spacing,              // separations, minima, justification caps
    pub vowels: Option<VowelRules>,    // overrides the rules chosen by `language:`
    pub alterations: AlterationScope,  // section 6.4
    pub custos: CustosPolicy,          // Auto (default) | Never
}
pub struct LayoutOptions {
    pub scale: f32,                    // output units per staff space; a text-size change only changes this
    pub last_line: LastLine,           // Ragged | Justified
    pub recitation: Continuation,
}

impl Score {
    pub fn engrave(&self, metrics: &dyn TextMeasure, style: &StyleOptions) -> Engraving;
    pub fn to_gabc(&self) -> String;
}
impl Engraving {
    /// `width` in output units; breaking runs at `width / opts.scale` staff spaces, so a uniform
    /// text-size change is a relayout, never a re-engrave. Buffers are reused across relayouts.
    pub fn layout(&self, width: f32, opts: &LayoutOptions) -> Layout;
}
impl Layout {
    pub fn size(&self) -> (f32, f32);
    pub fn display(&self) -> DisplayList;
    pub fn notes(&self, weights: &Weights) -> NoteMap;
    #[cfg(feature = "svg")] pub fn svg(&self, opts: &SvgOptions) -> String;
}
pub struct GlyphOutline { pub d: String, pub width: f32 }   // absolute M/L/C/Z, nonzero fill
pub fn glyph_outline(id: u16) -> Option<GlyphOutline>;
```

**Bindings.** The binding layer flattens types that UniFFI and wasm-bindgen
can't carry: diagnostic codes become `String`, and spans become two `u64`s.
Glyphs cross every FFI as `u16` ids with a lookup, not as an
enum, so the glyph set can grow without breaking Swift or Kotlin `switch`
statements (the Office denies wildcard arms). Strings are owned. Over UniFFI
and wasm-bindgen, the engraving lives behind a handle:
`Chant::new(source, kind, style)` (where `kind` is GABC, or pointed text with
its tone id) `-> handle`, `handle.layout(width, options) -> size`,
`handle.display() -> DisplayList`, `handle.svg() -> String`,
`handle.notes() -> NoteMap`. A relayout at an unchanged width returns the
cached result and copies nothing.

## 6. GABC coverage

The grammar follows Gregorio's GABC documentation (GregorioRef, "The GABC
File"). We use the specification, not Gregorio's GPL source.

Each token gets one of four treatments:

- **E1:** engraved in M1.
- **E2:** engraved in M2.
- **A:** accepted and ignored, with an info diagnostic. The token changes
  nothing visible, or only affects TeX output.
- **U:** unsupported. A warning, then recovery: render what can be recovered,
  such as the bare pitches as puncta.

The tiers are provisional. The corpus survey in M0 (section 14) counts which
tokens real AWRV material uses and moves rows between E1 and E2.

### 6.1 File structure and headers

| Item | Treatment |
|---|---|
| `name: value;` headers, multi-line values ending `;;`, `%` comments, the `%%` separator | E1 |
| `name`, `office-part`, `occasion`, `transcriber`, `gabc-copyright`, `score-copyright`, other descriptive headers | A (kept in `Score::header`) |
| `language:` | E1: selects vowel rules (`la` default, `en`, with Gregorio's aliases) |
| `annotation:` (one or two; the first is the upper line) | E1 |
| `mode`, `mode-modifier`, `mode-differentia` | E1: shown above the initial when there's no `annotation` (plain text; TeX markup stripped) |
| `staff-lines:` | E1 for 4. Any other value U, with pitches kept: `n` and `p` still tokenize, so recovery can draw them as puncta |
| `oriscus-orientation: legacy` | U (default rules used) |
| `def-mN` | A: the bodies are TeX. Uses (`[nmN] [gmN] [emN] [altmN]`) are dropped with an info note |
| `nabc-lines`, and NABC after `|` | U |

### 6.2 Lyric text

| Item | Treatment |
|---|---|
| Syllable text, with words split by spaces and line ends | E1 |
| `$` escape | E1 |
| `<i> <b> <sc> <ul> <c>` | E1 (`<b>` subject to section 11's face rules) |
| `<tt>` | A (set as regular) |
| Default `<sp>` characters: `V/ R/ A/ * + -`, `ae oe 'ae 'oe 'æ 'œ`, `\ & # _` | E1 |
| `<sp>ot ut dt ~</sp>` | U |
| `{…}` centering override | E1 |
| `~` lyric tie | E2 |
| `<e>` elision | E1: italic, and counts as a consonant for vowel finding |
| `<eu>` Euouae | E1: a high break penalty inside and a preferred break before it (Gabc.tex promises "special typographic consideration for line breaks", not a ban). Revisit after the M0 survey |
| `<nlba>…</nlba>` | E1: no line breaks inside |
| `<clear>` | E2 |
| `<pr>`, `<pr:n>` | A |
| `<alt>…</alt>` above-lines text | E2 |
| `[…]` and `[/]` translations | E2 |
| `<v>…</v>` verbatim TeX | A (dropped, with a warning when it held visible text) |

### 6.3 Notes, shapes and signs

| Item | Treatment |
|---|---|
| Pitches `a`–`m` (four lines), uppercase inclinata | E1 |
| `G0 G1 G2` inclinatum leaning | E2 (E1 draws the automatic lean) |
| `v` virga, `V` virga reversa, `vv` `vvv` | E1 |
| `s` stropha, `ss` `sss` | E1 |
| `o` `O` oriscus and scapus, `o0` `o1` orientation | E2 |
| `w` quilisma, `W` quilisma quadratum | E1 / E2 |
| `~` `<` `>` liquescents, `-g` initio debilis | E1 / E2 (`-g` E2) |
| `q` quadratum pes | E2 |
| `=` linea, `R`, `r0` | U |
| `r` cavum | E1 |
| `r1`–`r5` accents and circles above the staff | E2 |
| `r6`–`r8` musica ficta | E2 |
| `x y #` alterations | E1 |
| `x? #? y?` parenthesized alterations | E2 |
| `X ## Y` soft alterations | E1 (scope per section 6.4) |
| `.` `..` mora, `.0` `.1` placement | E1 |
| `_` horizontal episema with digit suffixes `0`–`5` | E1 for `_`, `_0`, `_1`; E2 for `_2`–`_5` |
| `'` `'0` `'1` vertical episema | E1 |
| `+` custos at a pitch, `z0` automatic custos | E1 |
| `[nocustos]` | E1 |
| `[ll:0/1]`, `[oll:]`, `[ull:]`, `[oh:]`, `[uh:]`, `[shape:]`, `[cs:]`, `[cn:]`, braces (`[ob:]` etc.), `[nv:]` `[gv:]` `[ev:]`, slurs | U |

### 6.4 Grouping, spacing, bars, clefs and breaks

| Item | Treatment |
|---|---|
| Space inside notes (large separation, breakable) | E1 |
| `/` `//` `/0` `/!` `/[f]` (a negative `f` is a backspace; `//[f]` is `/` then `/[f]`) | E1 |
| `!` alone (zero-width split) and `!` before a space (non-breaking) | E1 |
| `<nlba>…</nlba>` inside notes (its spaces don't break) | E1 |
| `{…}` zero-width notes | A (drawn with their own width) |
| `@` manual fusion, `@[…]` auto fusion | E2 |
| `` ` `` `` `0 `` `^` `^0` `,` `,0` `;` `:` `:?` `::` | E1 |
| `;1`–`;8` Dominican bars | E2 |
| Bar suffixes `'` (episema) and `_` (brace) | U |
| `c1`–`c4`, `f1`–`f4`, `cb1`–`cb4`, mid-line clef changes | E1 |
| Double clefs `c1@c4` | U (first clef used) |
| `z` justified break, `Z` ragged break, `z+ z- Z+ Z-` custos control | E1 |
| A break at the end of the score | A (Gregorio discourages it; dropped) |

**Line-breaking inputs.** `z` and `Z` both force a break. `z` justifies the
line it ends, and `Z` leaves that line ragged. A `+` or `-` suffix forces or
suppresses that break's custos. The only ways to forbid a break are `<nlba>`
regions and `!` before a space. A line that can't end anywhere else within the
width, as after an unclosed `<nlba>`, ends at its last forbidden break rather
than running past the width. (v1's claim that `Z` forbids a break was
wrong.)

**Alteration scope.** GABC itself only says where an alteration sign is drawn.
How long the alteration lasts is a convention: Gregorio's default is the
line, the Solesmes books use the word or the next bar, and Dominican books
differ again. So `AlterationScope { Line, Word, Bar, WordOrBar, Note }` is an
explicit option. It defaults to `WordOrBar`, documented as Solesmes practice
rather than GABC. That one setting drives three things:

1. whether a soft alteration (`X ## Y`) is drawn: only when it would change
   the pitch in effect;
2. `semitones` in the note map;
3. whether a flat is redrawn after a line break inside its scope. It's drawn
   there only when the author wrote a soft flat at that point, which is what
   Gregorio expects authors to do.

A `cb` clef's flat is a key flat on B, at every octave, for the whole score
until the next clef change. An explicit natural cancels it within the
alteration scope. Each case gets a test.

## 7. Neume construction

`neume.rs` splits each note group at separators, then classifies each run by
contour and shape flags into builders: single, pes, clivis, porrectus swash,
inclinata run, and compounds of those. Longer groups decompose left to right
using the fusion primitives Gregorio documents (punctum, oriscus, quilisma,
virga reversa, flexus, pes, porrectus and virga, each with its fusion
constraints). Each builder emits components: glyphs, connecting stems and
virga tails, positioned in staff spaces.

The builders follow exsurge's `NeumeBuilder` (MIT), which is proven on
Solesmes books. They're ported and restructured, not transliterated. Two
cases where exsurge has no glyph:

- **Porrectus wider than a fourth.** Exsurge draws nothing. Neuma draws the
  two puncta joined by a stem, with an info diagnostic.
- **Stropha liquescent.** Exsurge refers to a `StrophaLiquescent` code that
  isn't in its glyph table. Neuma draws a scaled-down stropha, the same way
  the liquescent punctum relates to the punctum.

## 8. Glyphs

The outlines come from exsurge's `Glyphs.js`: 48 glyphs plus `None`, drawn
from the author's own Caeciliae font and published under MIT, about 18 KB of
path text in all. `tools/gen-glyphs` converts them once into
`glyphs/table.rs` and **normalizes every outline**:

- Every command becomes absolute `M`, `L`, `C` or `Z`. Relative forms and
  `s S q Q t T h v` are expanded, and quadratic curves become cubics.
- "Negative" parts, such as the hole in a punctum cavum, become subpaths
  wound opposite to their glyph's positive outline. The generator computes
  each positive path's signed area, because exsurge's outlines aren't
  consistently wound, and reverses any negative path with the same sign. A
  nonzero fill then leaves the hole open, so no even-odd flag is needed. The
  generator also renders every glyph both ways in headless Chromium and
  asserts the pixels agree.
- Coordinates are rounded to 3 decimals.

iOS's `SVG.swift` already parses the normalized form, and it's also smaller
and deterministic. An iOS test round-trips every glyph id through `svg(_:)`
and checks the subpath count. The checked-in table records the source file's
hash, so a regeneration shows up in review.

Stems, connecting lines, staff lines, ledger lines, bars and episemata are
rectangles rather than glyphs. They stay crisp, and themes can color them by
role.

Gregorio's greciliae font is **not** a fallback source. It's under the OFL
(and its TTF is generated by GPL tooling), so a table of its outlines couldn't
ship under MIT/Apache. Shapes exsurge lacks are composed from existing glyphs
and rectangles, or drawn fresh.

## 9. Display list

```rust
pub struct DisplayList { pub width: f32, pub height: f32, pub lines: Vec<LineBox>, pub items: Vec<Item>, pub alt_text: String }
pub enum Item {
    Glyph { glyph: u16, x: f32, y: f32, scale: f32, role: Ink, note: Option<NoteRef> },
    Rect  { x: f32, y: f32, w: f32, h: f32, role: Ink, note: Option<NoteRef> },
    Text  { x: f32, baseline: f32, size: f32, runs: Vec<TextRun>, role: TextRole, syllable: Option<u32> },
}
pub enum Ink { Staff, Ledger, Note, Stem, Bar, Episema, Mora, Ictus, Accidental, Clef, Custos }
pub enum TextRole { Lyric, Hyphen, Initial, Annotation, Rubric }
pub struct TextRun { pub text: String, pub style: TextStyle }   // italic, bold, small caps, rubric
```

- Every element has a role, so themes can color staff, notes and rubrics
  separately. SVG maps roles to CSS classes and fills with `currentColor`, so
  the Office's appearances and dark mode need no engine changes.
- Consumers draw text with the real font at the given x. Each syllable is
  placed on its own, so measurement differences can't accumulate across a
  line. Within a syllable, section 11 keeps measurement and rendering in step.
- `NoteRef` ties each piece of ink to its note, for highlighting and hit
  testing.
- `alt_text` is the plain lyric text. SVG uses it for `role="img"` and
  `aria-label`, and the native apps use it as the accessibility label.

## 10. Lyrics and vowel centering

- **Centering.** Notes sit over the syllable's vowel: the first note's center
  is over the center of the vowel nucleus, Gregorio's default `vowel`
  centering. `{}` overrides it. Elisions, `<sp>` and `<v>` count as
  consonants. A syllable with no vowel is centered as a whole.
- **Vowel rules are data**, in Gregorio's vowel-file format (`language`,
  `alias`, `vowel`, `prefix`, `suffix`, `secondary`). The crate ships the
  Latin and English rules exactly as Gabc.tex documents them, and the
  `language:` header picks one. The English rules have prefixes `qu` and `y`
  and suffixes `w` and `we`, so "yes" centers on *e* and "new" includes the
  *w*. If a choir prefers different conventions, an AWRV variant is another
  file, not code.
- **Hyphens.** The engine draws them itself: one centered between each pair
  of syllables in a word, and one after the last syllable on a line when a
  word continues on the next line. The `gabc::hyphen-in-syllable` lint flags
  an author's hyphen at either end of a syllable's text, both `ser(h)-vants`
  (at the start of the next syllable, as in Psalm 134) and `ser-(h)vants`.
  Gregorio would print the author's hyphen as well as its own.
  `<sp>-</sp>`, a zero-width hyphen, is allowed.
- **Widths.** A syllable wider than its notes widens its unit. Syllables with
  no text (bars, `*`) have no lyric box.

## 11. Text measurement

`TextMeasure` is the only text dependency. The engine's widths are right only
if every renderer shapes text the way it was measured, so neuma fixes the
shaping.

- **Lyrics are set without ligatures.** SVG output sets
  `font-variant-ligatures: none`. The Office apps do the same on chant text:
  `fontFeatureSettings = "'liga' 0"` on Android (the quoted CSS form the
  Office already uses in `Blocks.kt`), and a `.ligature: 0` attributed-string
  key on iOS. Lyric runs on iOS must not inherit the app's `.kern` tracking
  (`Blocks.swift`). The Office's EB Garamond 12 turns on `f_f f_i
  f_l f_t f_h f_k` under `liga` by default, which would otherwise make
  "fi-at", "of-fer" or "Lift" render narrower than measured.
- **Small caps are real.** `<sc>` text is measured with the face's `smcp`
  substitutions applied (the table builder applies GSUB single substitution
  for `smcp`). Renderers use the font feature: `font-variant-caps:
  small-caps` on web, and the `smcp` support the apps already use, such as
  Android's `ALL_SMALL_CAPS`.
- **Kerning is on.** The table holds GPOS pair adjustments (formats 1 and 2)
  and legacy `kern`, which is what browsers, Core Text and Android apply to
  plain Latin text. If the conformance test below shows a renderer diverging,
  the fallback is to turn kerning off for lyrics everywhere. The table is
  then advances only.
- **Faces.** A table holds one entry per face it was built from. When a style
  has no face, `has_face` is false: EB Garamond 12 has no finished bold, and
  the Office's iOS app bundles none. The engine then measures with the
  regular face widened by a fixed 3% and records a `text::synthetic-face`
  info. The display list keeps the requested style, so the consumer can
  synthesize it or substitute a face. The Office maps `<b>` to its own bold
  where it has one.
- **Pinned.** A metrics table records the SHA-256 of each font file it was
  built from. The Office's table is built from the Debian EBGaramond12 OTFs.
  According to the fonts README, the web woff2 subsets and the app fonts both
  come from those files. M0 checks that the woff2 glyph advances match the
  OTFs before one table is trusted for both web and native.
- **Conformance test.** For every syllable in the corpus, in each style, the
  table's advance must match Chromium's `canvas.measureText` (same font and
  features) within 1%. This runs in CI with Playwright. iOS and Android run the
  same check against Core Text and `Paint.measureText` in their own test
  suites.

A browser consumer may instead pass a measurer backed by `measureText`. Text
is then pixel-exact, but layouts are no longer identical across platforms.
The Office web client uses the table, so its server and client layouts match.

## 12. Note map

```rust
pub struct NoteMap { pub notes: Vec<MappedNote>, pub pauses: Vec<Pause> }
pub struct MappedNote {
    pub id: NoteRef, pub syllable: u32, pub line: u32,
    pub x: f32, pub y: f32,                 // notehead center, output units
    pub span: Range<usize>,                 // source span (GABC bytes, or pointed-text bytes)
    pub staff_position: i8,
    pub degree: Degree,                     // scale degree from the clef's do or fa, with alteration
    pub semitones: i16,                     // from the clef's reference (do = 0), per AlterationScope
    pub weight: f32,                        // from the chosen Weights table
    pub w: f32, pub h: f32,                 // notehead box, output units
    pub syllable_text: String, pub word: u32,
    pub vowel: Option<char>,                // the nucleus the engine centered on, for formant synthesis
    pub shape: NoteShape, pub liquescent: bool, pub quilisma: bool,
}
pub struct Pause { pub before_note: u32 /* notes.len() = after the last */, pub kind: BarKind, pub weight: f32 }
pub struct Weights { /* per-sign multipliers */ }
impl Weights { pub const SOLESMES: Weights = /* 1 per note; mora 2; episema 1.5; bars by kind */; }
```

Chant has no absolute pitch, so the map gives semitones from the clef's
reference. A practice tool picks a key, or detects the singer's, and adds an
offset. Weights are relative durations, not beats. Tools choose the tempo and
interpretation, and `SOLESMES` is one named table among others that could be
added. Each pause names the note it comes before, with `notes.len()` meaning
after the last, so a bar before the first note, or two bars in a row, can be
represented. The note map carries the syllable text and vowel because the
existing practice tools already use both, and they shouldn't have to find
vowels again.

## 13. Determinism

The same input, options and metrics table give byte-identical output
everywhere, under these rules:

- Layout uses only `f32` add, subtract, multiply, divide, comparison and
  `sqrt`. It uses no transcendental functions (`sin`, `powf`, `exp`) and no
  `mul_add`, whose results differ between libm builds and fused hardware.
- SVG writes coordinates with exactly two decimals. Rounding to 1/100 of an
  output unit happens once, at the display-list and SVG boundary, never
  between layout stages.
- The breaker's badness is cubic, computed as `r * r * r`, never `powf`.
- The optimal-fit breaker breaks ties explicitly (the earliest break wins),
  so iteration order never decides.
- Golden files carry the metrics table's hash in their names, so rebuilding a
  table shows up as a change.

## 14. Corpus and testing

**M0 builds the corpus before any engraving code.** Neume rules and spacing
can only be judged against real material, and goldens built from synthetic
input would lock in whatever the first implementation does.

| Source | What | Licence check |
|---|---|---|
| Office `data/texts/chant/` | Psalm 134, after its hyphen fix | Office repo |
| A hand-pointed English psalter (the test psalter) | Pointed psalm settings in several tones. This is the test set for `neuma-tones`: apply, then diff against the hand pointing. | Private test data, kept out of this repository |
| GregoBase | Only pieces whose `gabc-copyright` header is CC0 or a public-domain statement, chosen from the Latin-derived repertoire the AWRV sings | Per-file header |
| Per-token fixtures | One small GABC fixture per row in section 6, written by us | Ours (MIT) |

M0 then counts which tokens the corpus uses and retiers section 6 to fit. It
also runs a **pointed-markup survey** of the test psalter (section 15.1).

**Tests**

- **Unit tests** per module, including one per section 6 row and one per
  alteration-scope case.
- **Round-trip:** `parse → to_gabc → parse` gives an equal `Score` for every
  corpus file.
- **Golden snapshots:** SVG and note-map JSON per fixture at three widths
  (360, 600 and 900 px), with self-contained review tooling. CI rasterizes
  changed goldens with resvg (a dev dependency) and uploads the PNGs.
- **Differential against exsurge:** for the shared corpus, compare note
  positions and line breaks with exsurge's layout. Both are MIT, so this runs
  in CI under Node. Large disagreements get reviewed rather than failing the
  build.
- **Invariants** (property tests over the corpus at random widths):
  - no line exceeds its width unless the overflow diagnostic was emitted;
  - lyric boxes on a line never overlap;
  - the ink of adjacent units never overlaps;
  - each note appears exactly once in the note map;
  - layout at a given width is the same whether or not another width was laid
    out first.
- **Text conformance** (section 11).
- **Fuzzing** the parser, `neuma-tones` and the full pipeline with
  cargo-fuzz: no panics, and layout always finishes.
- **WebAssembly:** headless Chromium tests that browser and native give
  identical SVG for the same width and table.
- **By eye:** at milestones, compare a reference set with GregorioTeX output.
  Gregorio is a development tool here, never a dependency.

## 15. Psalm tones and pointed text

Anglican-style psalters point psalms by accent, with preparatory syllables.
Each psalm is pointed again by hand for each tone and ending, so the model
accepts **pointed text plus a tone** directly and doesn't need hand-written
GABC.

### 15.1 Pointed-markup survey (M0)

A count of the test psalter's markup found it richer than a simple accent model:

- **The cadence dot `·` falls inside words** as well as between them
  ("peo·ple", "re·joice", "e·ver befóre"), so it is also a syllable boundary.
- **Most cadence words aren't hyphenated.** Accented words with one syllable
  after the accent are unhyphenated far more often than not.
- **The en dash has several uses:** a note with no new syllable in two-accent
  cadences ("thou · árt – mý God"), verse-initial use ("7 – · God shall
  bléss us"), and sung-syllable splits in notated underlay
  ("Al–le–lu–ia").
- **A circumflex marks reading-tone cadences** ("côme: †", "thŷ protection")
  in collects and chapters.
- **Coverdale's own compounds** ("blood-guiltiness", "well-beloved") use the
  same hyphen as sung-syllable splits.

So the grammar is **derived from the source, not assumed**. M0 enumerates
every non-letter mark in the source document's runs (with font, colour and italic) and
its contexts: in a word, between words, at the start of a verse, before and
after `*` and `†`. It records each mark's meaning in `neuma-tones/MARKUP.md`,
and every meaning gets a fixture. The parser accepts exactly that grammar,
and anything outside it is a diagnostic with a span, never a silent guess.
The provisional meanings below are what the survey has to confirm or correct.

| Mark | Provisional meaning |
|---|---|
| `·` (red, bold) | The cadence begins at the next syllable. Inside a word, it is also a syllable boundary. |
| acute on a vowel | That syllable takes an accent slot. |
| `–` between words (pointed psalms) | A cadence note with no new syllable: the previous syllable is held. |
| `–` at the start of a verse | To be determined by the survey. |
| `–` inside a word (notated pieces) | A syllable split. It belongs to an underlay importer, not to the pointed grammar. |
| `-` inside a word | A sung-syllable split, unless the word is hyphenated in the base text (below). |
| `*` | Mediant. The same meaning as the Office psalter's `*` (`RunStyle::Mediant`). |
| `†` (with the italic syllable before it) | Flex; the italic marks the flex syllable. |
| circumflex on a vowel | A reading-tone cadence mark (collect and chapter tones). |
| Verse numbers; posture cues such as "Sit." | Kept as rubric spans and never sung. |

**Orthographic hyphens.** The parser takes an optional base text: the
unpointed verse, such as the Office psalter's. A hyphen that is also in the
base text is orthographic ("blood-guiltiness"), and any other in-word hyphen
is a sung split. Without a base text, `\-` writes an orthographic hyphen.
Some marks are carried by formatting (the italic flex syllable, the red bold
`·`), so the text form writes them as characters: `·` stays `·`, and the flex
syllable is the one before `†`. The round-trip test runs at the rich-run
level for the source-document importer (marks, italics and colour preserved) and is
byte-identical for the text form.

### 15.2 Syllabification ships with the tones (M2b)

`apply` has to place termination notes on the syllables after an accent, and
most cadence words in the test psalter aren't hyphenated. So English
syllabification is an **M2b requirement**, not an M5 addition:

- **Splits come from three sources, in priority order:** the markup's own
  splits (`-`, in-word `·`), an exception list, then hyphenation patterns. A
  psalter's hand-hyphenated cadence words and its in-word `·` splits are
  the first test set.
- **The patterns are TeX's `hyph-en-us`** (the ushyphmax patterns, which
  has a permissive notice that allows modification), vendored with that
  notice in NOTICE once the vendored file is confirmed to match.
- **Sung syllables differ from typographic hyphenation**, because patterns
  refuse short splits ("a·gain", "e·ver"). The syllabifier therefore runs with
  minimum fragment lengths of 1 and 1. Every fragment must still contain a
  vowel, which guards against spurious splits at those limits, and the
  exception list covers Coverdale's sung forms ("judg-ed", "bless-ed").
  Machine splits are reviewed grouped by word frequency.
- **Each split records its source** (markup, exception or pattern), so a
  review tool can show a reviewer only the machine-made splits that affect notes.

### 15.3 What the engine adds

- **`ScoreBuilder`** builds a `Score` from syllables, notes and bars, and
  `to_gabc()` exports it for review or for GregorioTeX.
- **Recitation units.** A recitation is a run of words sung on the reciting
  note: a *sequence of word sub-units that share one notehead*. Each word is a
  break opportunity, with a penalty that favours breaking at the end of the
  recitation. When a recitation continues on a new line,
  `Continuation::RepeatTenor` (the default) repeats the reciting note at the
  line start, and `Continuation::None` leaves the continuation without a
  note. Words are set as a phrase, without hyphens or per-syllable spacing,
  the way plainsong psalters print the tenor. In the note map, each
  recitation syllable (from 15.2) is a note on the reciting pitch.
- **`neuma-tones`:**
  - **Tone data:** one small text file per tone and differentia. Each gives
    the intonation, tenor, flex, mediant and termination as accent slots,
    preparatory slots and fixed notes, with `source:` and `licence:` fields.
    It starts with the eight tones and the tonus peregrinus, in the endings
    the test psalter uses (viii.1, vii.2, i.b.2 and so on).
  - **The pointed-markup parser and writer** (15.1) and the syllabifier
    (15.2).
  - **`apply(tone, pointed_psalm, options) -> Score`**, with options for
    intoning every verse or only the first, adding the Gloria Patri, and the
    continuation policy. Testing it against a hand pointing alone would be
    circular, because that pointing is the input, so the test also checks
    `apply`'s notes for each first verse against the tone formula printed
    with the psalm.
  - **Scope.** Psalm and canticle tones are in. Reading tones (collects and
    chapters, the circumflex marks) are parsed and preserved losslessly, but
    applying them is out of scope until M6.
  - **Automatic pointing (M5):** `point(text, tone) -> Pointed`, from a
    stress dictionary with manual overrides, tested against the test psalter.
- **Office tie-in.** The Office psalter's `*` already marks the mediant, so
  the pointer reads the Office text as it is. Accents, `·` and `†` are kept
  as a pointing overlay per psalm and tone, anchored by **verse number and
  word index**, never by byte offset. Office texts are hash-attested
  (`data/review/provenance.csv`), so a base-text edit is re-attested on
  purpose. `make validate` then checks that every overlay's word count still
  matches its verse, and that each anchored word is still the same word, and
  fails if not. The Office can then render any psalm in any tone without
  storing 150 × N GABC files.

## 16. Office integration

1. **`crates/render-chant`** loads `data/texts/chant/**` (GABC and pointing
   overlays) at startup, engraves each score once, and caches the
   `Engraving`. The apps get the same data through `mobile-ffi`'s embedded
   `data/`.
2. **`render-html`**, when a score exists and chant is on, emits inline SVG.
   To keep the first paint close to the client's relayout, the server lays
   out at its best guess of the reader's width: a phone or desktop default
   from the user agent, plus the reader's text size once `app.js` mirrors it
   from localStorage into a cookie (today only `tz` is a cookie). It also reserves that
   layout's height as `min-height`. A wrong guess still moves the page once
   when the client relays out; the guess keeps that move small and rare.
   Without JavaScript, and in print, the SVG scales with `width: 100%` and an
   `aspect-ratio`. The text stays in the page as the accessible fallback.
3. **`app.js` and `sw.js`.** On pages with `data-neuma`, a feature check is
   followed by a dynamic `import()` of the `--target web` glue, which loads
   `neuma.wasm` and the metrics table and relays out on ResizeObserver. All
   three files go in `sw.js` `CORE_ASSETS` with asset stamps, so offline
   relayout works. There's no CSP today; adding one would need
   `wasm-unsafe-eval`.
4. **`render-blocks`** gets `BlockKind::Score` carrying a score id. The apps
   call `mobile-ffi`'s `chant_layout(id, width_pt, text_scale)` on a cached
   handle, fetch each glyph outline once per `u16` id, and draw text with the
   bundled EB Garamond, ligatures off.
5. **Settings:** "Show chant" beside Prayer form, off by default, with its own
   usage key.

## 17. Milestones

0. **M0, corpus and groundwork.**
   - Assemble the corpus (section 14), count its tokens, and retier section 6.
   - Glyph normalization and its iOS check.
   - Build the EB Garamond metrics table, and verify the woff2 subsets against
     the OTFs.
   - Survey the test psalter's pointed markup (section 15.1), and check the
     hyphenation patterns' licence (section 15.2).
1. **M1, core.**
   - Parser (every token in section 6 tokenizes, and E1 rows engrave),
     resolver, `ScoreBuilder` and the `to_gabc` round-trip.
   - E1 neumes and signs, lyrics and vowel data.
   - Optimal-fit breaking with per-line clefs and custodes, and justification.
   - SVG, a minimal note map (positions, pitch, weights), and the CLI
     `render`, `check` and `notes` commands.
   - Renders Psalm 134 and the corpus's E1 pieces.
2. **M2, repertoire.** E2 rows, initials and annotations, the recitation unit,
   the text conformance test, and the differential test against exsurge.
3. **M2b, tones.** `neuma-tones`: the markup parser and writer (with the
   grammar from the M0 survey), English syllabification, tone data and
   `apply`, tested against the test psalter's pointings.
4. **M3, bindings.** The WebAssembly build, npm package and DOM helper, with
   wasm tests. Practice tools can switch over from exsurge.
5. **M4, Office.** Section 16, behind a flag.
6. **M5, automatic pointing.**
7. **M6, reading tones** (collects and chapters).

## 18. Open questions

1. Repository name and owner: `orthodoxwest/neuma` (free on crates.io as of
   2026-10-05; npm not checked). The maintainers to create it.
2. Publish to crates.io and npm, or use git dependencies only at first?
3. Do English choirs want different vowel centering from Gregorio's
   English rules?
4. Do we want an SVG mode that draws text as paths, for exports that must
   look identical without the font installed?
