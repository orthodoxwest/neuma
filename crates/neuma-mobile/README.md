# neuma for iOS and Android

UniFFI bindings for neuma (namespace `neuma`, UniFFI 0.32). A `Chant` engraves a score
once, and `update(src)` engraves it again after an edit, reusing what didn't change.
`layout(width, options)` returns a `ChantLayout`: `page()` holds what to draw (glyphs,
rectangles and text), `timeline(weights)` the playback timeline (a `ChantTimeline`), and
`noteAt`, `sourceAt` and `elementsAt` its hit tests. (The `Chant` prefix keeps them clear of
SwiftUI's `Layout` and WidgetKit's `Timeline`; Rust calls them `Layout` and `Timeline`.) Every
call is synchronous, and a `Chant` can be shared across threads. Each `ChantLayout` answers
for itself, so one `Chant` can serve a thumbnail and the main view, and a layout keeps
showing the score it was made from after an update.

The bindings are a thin layer over the Rust `neuma::Chant`: the defaults, and what happens
to a value the engine can't use, are the engine's, the same as in the browser and on the
command line. Every option record field has a default, so `ChantOptions()`,
`LayoutOptions()`, `Weights()` and `PsalmOptions()` are the usual options. An enum field is
nullable and defaults to null, which means the engine's default (UniFFI can't give an enum
field any other default):

| Field | null means |
|---|---|
| `ChantOptions.font` | `LyricFont.GOOGLE` (`.google`): Google Fonts' EB Garamond |
| `LayoutOptions.lastLine` | `LastLine.RAGGED` (`.ragged`) |
| `PsalmOptions.intone` | `Intone.FIRST_VERSE` (`.firstVerse`): the intonation on the first verse only |

Ids,
indices, counts and offsets are signed `Int`s (`Int32` in Swift).

## Build

The crate is a plain Rust library; build the native library each platform loads with
`cargo rustc`:

```sh
# iOS: a static library per target, then an XCFramework as usual.
cargo rustc -p neuma-mobile --lib --release --target aarch64-apple-ios --crate-type staticlib
# Android: a shared library per ABI. Linking needs the NDK, e.g. through cargo-ndk.
cargo ndk -t arm64-v8a rustc -p neuma-mobile --lib --release --crate-type cdylib
# Bindings, from any host build of the shared library (.so on Linux, .dylib on macOS).
cargo rustc -p neuma-mobile --lib --crate-type cdylib
cargo run -p neuma-mobile --features bindgen --bin uniffi-bindgen -- \
  generate --library target/debug/libneuma_mobile.so --language kotlin --out-dir out
```

The library leaves out what an app doesn't use (the SVG writer). For a smaller library, build
it with link-time optimization in the app's release profile, `lto = "fat"` and
`codegen-units = 1`: about 12% smaller, and about as fast (engraving about 5% faster, line
breaking about 4% slower).

Generate Swift bindings with `--language swift`. Bindgen writes `Neuma.swift` plus the
`NeumaFFI` header and module map. The Kotlin bindings load the library through JNA, so an
Android app also depends on `net.java.dev.jna:jna:5.14.0@aar`.

If an app already has its own UniFFI crate, make that crate depend on `neuma-mobile` and
reference it so the linker keeps its exports (`use neuma_mobile as _;`). Then generate
bindings from the app's library in library mode: bindgen writes both crates' bindings, and
the app ships one native library. The two crates must use the same UniFFI version.

`crates/neuma-mobile/test.sh` runs one smoke test through each language it can on the
machine: the Python bindings everywhere, Kotlin on the JVM, and Swift on macOS. CI runs all
three.

## Use

```swift
let chant = Chant(gabc: source, options: ChantOptions())
let layout = chant.layout(width: Float(bounds.width), options: LayoutOptions())
for item in layout.page().items {
    switch item {
    case let .glyph(glyph, x, y, scale, role, notes): ...  // fill glyphOutline(id: glyph)!.path
    case let .rect(x, y, w, h, role, notes): ...
    case let .text(x, baseline, size, runs, role, syllable): ...
    }
}
```

```kotlin
val chant = Chant(source, ChantOptions(initial = 2))
val layout = chant.layout(width, LayoutOptions(scale = 8f))
val page = layout.page()      // keep it: each call copies the display list across
val timeline = layout.timeline(Weights(mediant = 3f))
val id = layout.noteAt(x, y)  // the note under a tap, or null
chant.update(edited)          // after an edit; lay it out again to see it
chant.setOptions(ChantOptions(initial = 2, lyricSize = 3f)) // a new text size; replaces all the options
chant.version()               // names the state: grows with each real change
layout.close()                // when a new layout replaces it
chant.close()
```

**A layout's lifetime.** A `ChantLayout` holds on to the engraving it was made from. While
a layout is held, the chant's next edit copies the engraving rather than changing it in
place, up to the edit; an app always holds the layout on screen, so this copy is part of an
edit's cost (on the longest scores, about 4,900 notes, about 0.1 ms of a 0.7 ms update; on
typical ones next to nothing). After the edit the old layout keeps the old engraving's lists
alive until it is freed, so
close the layout a view has replaced instead of leaving it to the garbage collector:
`close()`, `layout.use { }`, or in Compose a `DisposableEffect`. In Swift, drop the
reference. That bounds memory; it doesn't make the edit cheaper. `page()` is made once per
layout, but each call copies it across the boundary, so keep the value rather than calling
it on each recomposition or `body`.

**When to lay out again.** `chant.version()` names the chant's state: it grows with each
change that changed anything, no two chants share one, and `update` with the current source,
or `setOptions` with options that engrave the same, leave it as it is. `update` then
`version()` is not atomic: with several threads writing, another change can land between. Keep it in the app's state after each edit and key the layout on it, here
in Compose (in SwiftUI, `.id(version)` or an `onChange(of: version)`):

```kotlin
@Composable
fun Score(chant: Chant, width: Float, version: Long) { // version: chant.version(), as state
    val layout = remember(chant, width, version) { chant.layout(width, LayoutOptions()) }
    DisposableEffect(layout) { onDispose { layout.close() } }
    val page = remember(layout) { layout.page() }
    Canvas(Modifier.fillMaxWidth()) { draw(page) }
}
```

### Drawing

- **Coordinates.** Positions are in output units: `scale` units per staff space (default
  6), from the page's top left with y pointing down. Boxes (`Item.Rect`, `SourceElement`)
  are `x, y, w, h` from their top-left corner; a timeline note's notehead center is `cx,
  cy`. Pass the width in points and choose `scale` to suit the text size.
- **Glyphs.** Glyphs arrive as ids. Fetch each outline once with
  `glyphOutline(id)`; it is absolute `M L C Z` path data with nonzero fill. Draw it
  translated to (x, y) and scaled by the item's `scale`.
- **Rectangles** are staff lines, ledger lines, stems, bars and episemata.
- **Text.** Draw lyrics with EB Garamond at the item's x, baseline and size, ligatures off,
  and without the app's own tracking. `ChantOptions.font` (a `LyricFont`) says which EB
  Garamond the app bundles, so lyrics are measured the way they will be drawn. The default
  (null) is the Google Fonts build, as in the browser; use `GARAMOND12` (`.garamond12`) for
  the EB Garamond 12 release.
- **Highlighting.** `notes` lists the notes a piece of ink draws. A porrectus swash draws
  two. Every role (`Ink`, `TextRole`) can take its own color.
- **Accessibility.** `page.altText` is the plain lyric text.

### Timeline

`layout.timeline(weights)` holds `notes`, `pauses` and the total `duration`, made when asked
for, so a view that only draws never pays for it. `timeline.notes` lists the notes in
singing order, each a `TimelineNote` with:

- `id`: stable across layouts of one `Chant`.
- `cx`, `cy`: the notehead's center; `w`, `h`: its size.
- `start` and `duration`: in weight units.
- `semitones`: from the clef's do, flats applied.
- the syllable and word.
- flags for `accent`, `newSyllable` and `recitation`.
- `verse` and `half`.
- the source range: `sourceStart`/`sourceEnd` in UTF-8 bytes like the diagnostics' `start`
  and `end`, and `sourceUtf16Start`/`sourceUtf16End` in UTF-16 units, Swift's `NSRange`
  and Kotlin's string indices.

`timeline.pauses` have kinds that include the mediant `*` and the flex `†`, and a `start` and
`duration`. A mark is the whole pause at its bar, so a bar right after one lasts 0. Each
pause's `bar` is where its bar is drawn: `line`, `left` and `right` (both strokes of a double
bar), `cx`, `top` and `bottom`, and the bar's `index` in the score. A mark gives the bar it
sits at; `bar` is null when no bar is drawn for the pause.
`Weights` are the relative durations, and any field left out keeps its default. `virgula`
also times the minimis bar, and `half` the Dominican bars. `layout.noteAtTime(t, weights)`
finds the note sounding at time `t` for a playhead (null in a pause), keeping the timeline
between calls; `layout.noteAt(x, y)` hit-tests a tap.

### Library entries

`summarize(gabc)` returns a score's `Summary` without engraving it for display, which is
cheap enough to index a library. `chant.summary()` gives the same entry for a loaded score.
The entry holds:

- the typed headers: name, office part with its `OfficePart` kind, mode with number and
  differentia, occasion and the rest, plus `otherHeaders` for any other header;
- the incipit and full text, for search;
- the lowest, highest and final pitches, in semitones above do;
- counts, and the length in pulses.

For a preview row, lay out with `maxLines = 1`. The line is broken as it would be in the
whole score. An initial that spans more staves keeps its full size, and the page's
height includes it. The timeline ends with the kept lines: their notes and the pauses drawn
on them.

### Psalm tones

`psalm(text, tone, PsalmOptions())` sets psalm text (a verse per line, the mediant marked
`*`) to a built-in tone from `toneNames()`, and `psalmWithTone` to a tone block of your own.
The `PsalmSetting` holds GABC and each note's verse, half and role in the tone, with the sung
syllable's range in the text named as a timeline note's (`sourceStart`/`sourceEnd` and
`sourceUtf16Start`/`sourceUtf16End`). Half-verses with no pointing marks are pointed
automatically unless `PsalmOptions(autoPoint = false)`; `point(text, tone)` returns the
pointed text with the pointer's confidence and source range for each half-verse.
`PsalmOptions(intone = Intone.EVERY_VERSE)` sings the intonation on every verse. A
`ToneException`'s message says what is wrong ("no built-in tone 9.z").

`Chant.fromPsalm(text, tone, PsalmOptions(), ChantOptions())` sets and engraves the psalm in
one step, with its sources in the text: a tap on a note finds its syllable in the psalm, and
diagnostics (`point::unsure` among them) count the text. `tone` is a built-in name or a tone
block. `chant.psalm()` gives the setting as `psalm` returns it (`gabc`, `notes` with each
note's place in the tone, and `diagnostics`), and `chant.update(text)` sets new text to the
same tone and updates it. It is null for a chant made from GABC.

### A pointed psalter

Most often a psalm is shown as a pointed psalter prints it: the tone once, as a line of notes
with no words, and the verses beneath as text with their pointing marks.
`Chant.fromTone(tone, ChantOptions())` is the tone's line (lay it out with
`LayoutOptions(lastLine = LastLine.JUSTIFIED)` at about its natural width), and
`psalmDisplay(text, tone, PsalmOptions(accents = Accents.OUTSIDE_FLEX))` the verses (a
pointed psalter prints no acute in a flex; `Accents.NONE` prints none at all). Its
`toneLabel` names the tone as a psalter prints it beside the tone ("Tone 8 G", "Tonus
peregrinus"). Each `PsalmVerse` has its `number` and its line as `runs`, each a `text` and a
`kind` that says how to style it, which fits a styled `Text` directly. The space between a
mark and its syllable is U+00A0, and U+2060 (word joiner) follows each `–` and spelling
hyphen, so a line never breaks there; to search the text, read U+00A0 as a space and drop
U+2060. Text copied from the display keeps the invisible U+2060, so a search of the copy for
"blood-guiltiness" misses it. Drawing the runs with a text system that doesn't shape them,
glyph by glyph, drop U+2060: it has no width, but a font may draw a missing-glyph box for it. To sing the text again, keep `point`'s text rather than a copy of the display: the display leaves out the verse numbers and rubric brackets, and with `OutsideFlex` the acutes of a flex. `toneLabel(tone)` names a tone without any text. In Compose:

```kotlin
val red = Color(0xFFA3211C)
val display = remember(text) { psalmDisplay(text, "8.G", PsalmOptions(accents = Accents.OUTSIDE_FLEX)) }
for (verse in display.verses) {
    Text(buildAnnotatedString {
        verse.number?.let { withStyle(SpanStyle(color = red)) { append("$it ") } }
        for (run in verse.runs) {
            val style = when (val kind = run.kind) {
                PsalmRunKind.Point, PsalmRunKind.Held -> SpanStyle(color = red, fontWeight = FontWeight.Bold)
                PsalmRunKind.Mediant, PsalmRunKind.Flex -> SpanStyle(color = red)
                PsalmRunKind.Rubric -> SpanStyle(color = red, fontStyle = FontStyle.Italic)
                is PsalmRunKind.Syllable ->
                    if (kind.syllable.flexDrop) SpanStyle(fontStyle = FontStyle.Italic) else SpanStyle()
                PsalmRunKind.Text -> SpanStyle()
            }
            withStyle(style) { append(run.text) }
        }
    })
}
```

In SwiftUI:

```swift
func line(_ verse: PsalmVerse) -> AttributedString {
    var out = AttributedString(verse.number.map { "\($0) " } ?? "")
    out.foregroundColor = .red
    for run in verse.runs {
        var piece = AttributedString(run.text)
        switch run.kind {
        case .point, .held: piece.foregroundColor = .red; piece.inlinePresentationIntent = .stronglyEmphasized
        case .mediant, .flex: piece.foregroundColor = .red
        case .rubric: piece.foregroundColor = .red; piece.inlinePresentationIntent = .emphasized
        case .syllable(let syllable): if syllable.flexDrop { piece.inlinePresentationIntent = .emphasized }
        case .text: break
        }
        out += piece
    }
    return out
}
// ForEach(display.verses, id: \.sourceStart) { Text(line($0)) }
```

`·` and `–` are bold red, `*` and `†` red, rubrics red italic, and in a flex the syllables the
voice drops on (`syllable.flexDrop`) italic. A syllable run's kind carries its `syllable`
(a sealed class in Kotlin, an associated value in Swift), which also gives its `part`,
`role` in the tone, `accent`, `wordStart` and its range in the text in both units,
for a tap or a highlight that follows the singing. Unmarked half-verses are pointed for the
tone, and the `diagnostics` are `psalm`'s, `point::unsure` among them. `tone` is a built-in
name or a tone block.

### Editors

- **Diagnostics** carry `start`/`end` in UTF-8 bytes and `utf16Start`/`utf16End` in UTF-16
  code units (Kotlin string indices, `NSRange`). Codes are stable; docs/diagnostics.md lists
  them. `fix` is null or the one edit that fixes the problem: replace its range with
  `replacement`; `title` labels it in a menu.
- **`layout.sourceAt(x, y)`** returns the note, bar or syllable under a tap, as a
  `SourceElement` with its source range (both units), line, box and `cx` (a note's notehead
  center).
- **`layout.elementsAt(offset, unit)`** returns what to highlight for a caret, most specific
  first: the notes and bar whose source holds it, then a box per line for its syllable. Pass
  `OffsetUnit.UTF16` for a text view's caret, `OffsetUnit.UTF8` for a byte offset.

On each edit, call `chant.update(src)` and lay it out again: the engraving around the edit
is redone, and the rest, and the line breaks it can, are reused. A tap waits on no layout
in progress: hit tests run on the `ChantLayout`, not the `Chant`. The source or options the
chant already has change nothing, and leave `version()` as it was.
