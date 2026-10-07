# neuma for iOS and Android

UniFFI bindings for neuma (namespace `neuma`, UniFFI 0.32). A `Chant` engraves a score
once, and `update(gabc)` engraves it again after an edit, reusing what didn't change.
`layout(width, options)` returns a `Page`, which holds what to draw (glyphs, rectangles and
text) and the playback timeline. Every call is synchronous, and a `Chant` can be shared
across threads. `noteAt` answers for that Chant's most recent layout, so give each view that
lays the score out at its own width its own `Chant`.

The bindings are a thin layer over the Rust `neuma::Chant`: the defaults, and what happens
to a value the engine can't use, are the engine's, the same as in the browser and on the
command line. Every option record field has a default, so `ChantOptions()`,
`LayoutOptions()`, `Weights()` and `PsalmOptions()` are the usual options; an enum field
(`font`, `lastLine`, `intone`) defaults to null, which means the engine's default. Ids,
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
let page = chant.layout(width: Float(bounds.width), options: LayoutOptions())
for item in page.items {
    switch item {
    case let .glyph(glyph, x, y, scale, role, notes): ...  // fill glyphOutline(id: glyph)!.path
    case let .rect(x, y, w, h, role, notes): ...
    case let .text(x, baseline, size, runs, role, syllable): ...
    }
}
```

```kotlin
val chant = Chant(source, ChantOptions(initial = 2))
val page = chant.layout(width, LayoutOptions(scale = 8f, weights = Weights(mediant = 3f)))
val id = chant.noteAt(x, y)   // the note under a tap, or null
chant.update(edited)          // after an edit; lay it out again to see it
chant.close()                 // or let the cleaner free it
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

`page.timeline` (null with `LayoutOptions(timeline = false)`, for a view that only draws)
holds `notes`, `pauses` and the total `duration`. `timeline.notes` lists the notes in singing
order, each a `TimelineNote` with:

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

`timeline.pauses` have kinds that include the mediant `*` and the flex `†`. A mark is the whole
pause at its bar, so a bar right after one has weight 0. `Weights` are the relative
durations, and any field left out keeps its default. `virgula` also times the minimis bar,
and `half` the Dominican bars. `noteAt(x, y)` hit-tests the last layout.

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
The `PsalmSetting` holds GABC for `Chant` and each note's verse, half and role in the tone,
with the sung syllable's range in the text (`start`/`end` and `utf16Start`/`utf16End`).
Half-verses with no pointing marks are pointed automatically unless
`PsalmOptions(autoPoint = false)`; `point(text, tone)` returns the pointed text with the
pointer's confidence for each half-verse. `PsalmOptions(intone = Intone.EVERY_VERSE)` sings
the intonation on every verse.

### Editors

- **Diagnostics** carry `start`/`end` in UTF-8 bytes and `utf16Start`/`utf16End` in UTF-16
  code units (Kotlin string indices, `NSRange`). Codes are stable; docs/diagnostics.md lists
  them. `fix` is null or the one edit that fixes the problem: replace its range with
  `replacement`; `title` labels it in a menu.
- **`chant.sourceAt(x, y)`** returns the note, bar or syllable under a tap in the last layout,
  as a `SourceElement` with its source range (both units), line and box.
- **`chant.elementsAt(offset, unit)`** returns what to highlight for a caret, most specific
  first: the notes and bar whose source holds it, then a box per line for its syllable. Pass
  `OffsetUnit.UTF16` for a text view's caret, `OffsetUnit.UTF8` for a byte offset.

On each edit, call `chant.update(gabc)` and lay it out again: the engraving around the edit
is redone, and the rest, and the line breaks it can, are reused.
