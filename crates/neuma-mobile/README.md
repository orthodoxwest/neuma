# neuma for iOS and Android

UniFFI bindings for neuma (namespace `neuma`, UniFFI 0.32). A `Chant` engraves a score
once. `layout(width, options)` returns a `Page`, which holds what to draw (glyphs,
rectangles and text) and the playback timeline. Every call is synchronous, and a `Chant`
can be shared across threads.

## Build

```sh
cargo build -p neuma-mobile --release --target aarch64-apple-ios      # libneuma_mobile.a
cargo build -p neuma-mobile --release --target aarch64-linux-android  # libneuma_mobile.so
cargo run -p neuma-mobile --features bindgen --bin uniffi-bindgen -- \
  generate --library target/release/libneuma_mobile.so --language kotlin --out-dir out
```

Generate Swift bindings with `--language swift`. Bindgen writes `Neuma.swift` plus the
`NeumaFFI` header and module map.

If an app already has its own UniFFI crate, make that crate depend on `neuma-mobile` and
generate bindings from the app's library in library mode. Bindgen then writes both crates'
bindings, and the app ships one native library. The two crates must use the same UniFFI
version.

`crates/neuma-mobile/test.sh` runs one smoke test through each language it can on the
machine: the Python bindings everywhere, Kotlin on the JVM, and Swift on macOS. CI runs all
three.

## Use

```swift
let chant = Chant(gabc: source, options: defaultChantOptions())
let page = chant.layout(width: Float(bounds.width), options: defaultLayoutOptions())
for item in page.items {
    switch item {
    case let .glyph(glyph, x, y, scale, role, notes): ...  // fill glyphOutline(id: glyph)!.path
    case let .rect(x, y, w, h, role, notes): ...
    case let .text(x, baseline, size, runs, role, syllable): ...
    }
}
```

```kotlin
val chant = Chant(source, defaultChantOptions())
val page = chant.layout(width, LayoutOptions(lastLine = LastLine.RAGGED, weights = Weights(mediant = 3f)))
val id = chant.noteAt(x, y)   // the note under a tap, or null
chant.close()                 // or let the cleaner free it
```

### Drawing

- **Coordinates.** Positions are in output units: `scale` units per staff space (default
  6), with y pointing down. Pass the width in points and choose `scale` to suit the text
  size.
- **Glyphs.** Glyphs arrive as `u16` ids. Fetch each outline once with
  `glyphOutline(id)`; it is absolute `M L C Z` path data with nonzero fill. Draw it
  translated to (x, y) and scaled by the item's `scale`.
- **Rectangles** are staff lines, ledger lines, stems, bars and episemata.
- **Text.** Draw lyrics with EB Garamond at the item's x, baseline and size, ligatures off,
  and without the app's own tracking. `ChantOptions.font` says which EB Garamond the app
  bundles, so lyrics are measured the way they will be drawn. The default is EB Garamond
  12; use `google` for the Google Fonts build.
- **Highlighting.** `notes` lists the notes a piece of ink draws. A porrectus swash draws
  two. Every role (`Ink`, `TextRole`) can take its own color.
- **Accessibility.** `page.altText` is the plain lyric text.

### Timeline

`page.notes` lists the notes in singing order. Each note has:

- `id`: stable across layouts of one `Chant`.
- `x`, `y`, `w`, `h`: the notehead's center and size.
- `start` and `duration`: in weight units.
- `semitones`: from the clef's do, flats applied.
- the syllable and word.
- flags for `accent`, `newSyllable` and `recitation`.
- `verse` and `half`.
- the source byte span.

`page.pauses` have kinds that include the mediant `*` and the flex `†`. A mark is the whole
pause at its bar, so a bar right after one has weight 0. `Weights` are the relative
durations, and any field left out keeps its default. `noteAt(x, y)` hit-tests the last
layout.
