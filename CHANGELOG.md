# Changelog

All notable changes to neuma are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

The first release. neuma engraves Gregorian chant in square notation from GABC, or from
English psalm text and a psalm tone, with the same line breaks and coordinates on a server,
in the browser and on iOS and Android.

### Added

- `neuma`, the engine, with no dependencies:
  - a GABC reader that never fails: problems come back as diagnostics with stable codes,
    source spans and, where one edit makes sense, a fix;
  - engraving in Gregorio's style: every neume shape GABC writes, with exsurge's glyphs;
    lyrics centered on their vowels; GregorioTeX's spacing, bars included; drop-cap
    initials and their annotations;
  - optimal-fit line breaking at any width, cheap enough to redo on every resize. A bar ends
    the line before it, as in GregorioTeX, unless every other way sets a line far too
    loose, and a ℣ or ℟ under a bar stays with its verse;
  - SVG, styled with CSS classes and `currentColor`, whole or a line at a time;
  - a renderer-neutral display list of glyphs, rectangles and text, for native canvases;
  - a timeline with each note's position, pitch, duration and source, for practice and
    playback, with each pause's bar as drawn (`Pause::bar`: its line and its box, as in the
    source map), and a source map for hit tests both ways;
  - `Chant`, which keeps a score and redoes only what an edit changed, with the same result
    as starting afresh;
  - library entries (`summarize`): headers, mode, incipit, range and length, without
    engraving;
  - metrics tables for EB Garamond (Google Fonts and the EB Garamond 12 release).
- `neuma-tones`: pointed psalm text, the Solesmes psalm tones and their endings, settings of
  text to a tone, automatic pointing of English, and a pointed psalter's display.
- `neuma-book`: booklets from a `.book` file, as PDF (with the text font subset) or SVG pages.
- `neuma-metrics`: builds a metrics table for any font, checks a corpus for collisions, and
  counts lines that start with a bar.
- `neuma-cli`: the `neuma` command: `render`, `check`, `notes`, `info`, `tones`, `point`,
  `psalm` and `book`.
- The browser package (`neuma-wasm`): one self-contained ES module with the engine inlined,
  and TypeScript types.
- Bindings for Swift and Kotlin (`neuma-mobile`), through UniFFI.
- Documentation: rustdoc for every public item of `neuma` and `neuma-tones`,
  `docs/DESIGN.md` on the architecture, and `docs/diagnostics.md`, which lists every
  diagnostic code (a test keeps it complete).
- Packaging and CI:
  - The published crates carry license files (MIT OR Apache-2.0, plus NOTICE) and crates.io
    metadata, and CI packages each one.
  - SECURITY.md was added.
  - MSRV is 1.95, with a CI job.
  - CI is hardened: actions pinned to commits, read-only permissions, cargo-deny, rustdoc
    `-D warnings`, a size budget of 470 KiB for the browser module, and checksummed
    downloads.
  - Clippy rejects `mul_add`, `powf` and the other functions whose results differ between
    platforms.

[0.1.0]: https://github.com/orthodoxwest/neuma/releases/tag/v0.1.0
