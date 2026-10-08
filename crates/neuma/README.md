<p align="center">
  <img src="https://raw.githubusercontent.com/orthodoxwest/neuma/main/docs/images/hero.svg" width="100%" alt="The introit Puer natus est in square notation, engraved by neuma: a drop-cap P with its annotation, four-line staves, neumes, bars, a custos at the end of the first line and the Latin text under the notes">
</p>

# neuma

The engine of [neuma](https://github.com/orthodoxwest/neuma): it reads
[GABC](https://gregorio-project.github.io/gabc/), the text format of the Gregorio project, and
engraves Gregorian chant in square notation, in Gregorio's style, that reflows to any width.
A layout comes out as SVG, as a renderer-neutral display list of glyphs, rectangles and text,
and as a timeline with the position, pitch and duration of every note. The same engine, with
the same line breaks and coordinates, runs in the browser and on iOS and Android.

- **Engrave once, reflow cheaply.** Parsing, neumes and lyric spacing happen once; only line
  breaking reruns when the width changes, so a page can lay out again on every resize.
- **Never fails on input.** Malformed GABC comes back as diagnostics with stable codes, source
  spans and, where one edit makes sense, a fix. The engine draws what it can.
- **Built for editors.** A source map links each note, bar and syllable to its GABC both ways,
  and `Chant::update` engraves again only around an edit.
- **Library entries** without engraving: typed headers, mode, incipit, text for search, range
  and length.

```toml
[dependencies]
neuma = "0.1"
```

```rust
use neuma::Chant;

let gabc = "name: Regina caeli;\nmode: 6;\n%%\n\
    (c4) RE(f)gí(g)na(h) cae(hj)li,(h) lae(ghg)tá(fe)re,(f.) (;) al(f)le(g)lú(hg)ia.(g.) (::)";

// Parsing never fails: problems come back as diagnostics.
let chant = Chant::new(gabc); // engraved once; keep it, share it: it is Send + Sync
for d in chant.diagnostics() {
    eprintln!("{d}");
}

// Lay out at any width: cheap enough for every resize. A layout is an owned value.
let layout = chant.layout(720.0);
let svg = layout.svg(); // lyrics set in EB Garamond
let timeline = layout.timeline(); // where and when each note is
assert!(svg.starts_with("<svg") && timeline.notes.len() == 17);
let first = &timeline.notes[0];
assert_eq!(layout.note_at(first.cx, first.cy), Some(first.id)); // hit tests, per layout
```

The [API documentation](https://docs.rs/neuma) covers the options, the display list, the
coordinate system and the pipeline under `Chant`. Every diagnostic code is listed in
[docs/diagnostics.md](https://github.com/orthodoxwest/neuma/blob/main/docs/diagnostics.md).

## Features

| Feature | Default | What it adds |
| --- | --- | --- |
| `svg` | yes | `Layout::svg` and the SVG writer |
| `fonts` | yes | both built-in EB Garamond metrics tables (`font-google` and `font-garamond12`) |
| `font-google` | through `fonts` | the table for the EB Garamond Google Fonts serves (about 85 KB) |
| `font-garamond12` | through `fonts` | the table for the EB Garamond 12 release (about 23 KB) |
| `json` | no | `neuma::json`: the outputs as the JSON the browser module and the CLI give |

The crate has no dependencies. Without a metrics table lyrics are measured roughly; a table of
your own, for another font, is built with
[neuma-metrics](https://crates.io/crates/neuma-metrics).

## Related crates

- [neuma-tones](https://crates.io/crates/neuma-tones): psalm tones, and English psalm text
  pointed automatically and set to them.
- [neuma-book](https://crates.io/crates/neuma-book): booklets paginated into PDF and SVG pages.
- [neuma-cli](https://crates.io/crates/neuma-cli): the `neuma` command.
- The browser module and the Swift and Kotlin bindings are built from the
  [repository](https://github.com/orthodoxwest/neuma/blob/main/README.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/orthodoxwest/neuma/blob/main/LICENSE-APACHE))
- MIT license ([LICENSE-MIT](https://github.com/orthodoxwest/neuma/blob/main/LICENSE-MIT))

at your option. [NOTICE](https://github.com/orthodoxwest/neuma/blob/main/NOTICE) credits the work neuma builds on, with its licenses.
