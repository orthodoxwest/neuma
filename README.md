# neuma

A Gregorian chant engraving engine in Rust. neuma reads [GABC](https://gregorio-project.github.io/gabc/)
and draws square notation that reflows to any width, for the web (WebAssembly), servers and
native apps.

It is in early development.

## Goals

- Parse GABC, the format of the Gregorio project, with diagnostics instead of failures.
- Engrave once, then lay out at any width cheaply, so a page can reflow on resize.
- Output a renderer-neutral display list, SVG, and a note map (positions, pitches and
  durations) for practice and playback tools.
- Produce identical output on every platform.
- Set psalm tones from pointed text.
- Print booklets: a list of scores, psalms, rubrics and text set on pages as PDF and SVG
  (`neuma book`, see [crates/neuma-book](crates/neuma-book/README.md) and the example in
  [examples/compline](examples/compline/compline.book)).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. The glyph outlines and parts of the neume construction come from
[exsurge](https://github.com/bbloomf/exsurge) under the MIT license; see [NOTICE](NOTICE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
