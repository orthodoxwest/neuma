# Changelog

## Unreleased

Editor support: source maps, diagnostics with fixes, SVG a line at a time, and faster
engraving and layout.

### Breaking changes

- **`neuma::Diagnostic`** has a new public field, `fix: Option<Fix>`. Code that builds a
  `Diagnostic` with a struct literal must add `fix: None` (or a fix).
- **`neuma::SvgOptions`** has a new field, `ids: bool` (default `true`). Struct literals need
  it, or `..SvgOptions::default()`.
- **`neuma::score::Header`** has a private field (each field's source span), so it can no
  longer be built with a struct literal: start from `Header::default()` and push to
  `fields`. Equality still compares `fields` only.
- **`neuma-wasm`** (the Rust side of the browser package): `Chant::summary_json` takes
  `&mut self`, as it now builds the summary on first use; `json::diagnostics`,
  `json::setting` and `json::pointing` take an extra `Option<&Utf16Index>` for UTF-16
  offsets. `Chant::note_at` takes `&mut self`, as a layout without the timeline leaves the
  note map to be made when first asked for. `SvgOutput` has a new variant, `ChangedLines`.
- **`neuma-mobile`** (UniFFI): the `Diagnostic` record has new fields, `utf16Start`,
  `utf16End` and `fix`. Reading diagnostics is unaffected; Kotlin or Swift code that
  constructs one must pass them.
- **Diagnostics** now point at the source they are about (header fields, tags, the hyphen in
  a syllable) rather than at empty or approximate spans, and there are new codes
  (`gabc::no-clef`, `gabc::unclosed-tag`). `engrave::final-break` is retired: no score could
  reach it. See [docs/diagnostics.md](docs/diagnostics.md).

### Added

- `Layout::source_map()`: what is drawn where, and the source of each note, bar and
  syllable, both ways (`SourceMap::source_at`, `SourceMap::at`); `Utf16Index` for editors'
  offsets.
- `Fix` on diagnostics where one edit makes sense, with `Fix::apply`.
- `Layout::svg_parts()`: the SVG as a head, definitions and one string per line, so a page
  can replace only the lines an edit changed.
- `EngraveCache`, `Engraving::layout_cached` with `LayoutCache`, and
  `Layout::svg_parts_cached` with `SvgCache`: after an edit, engraving, line breaking and
  each line's SVG are redone only where the edit could change them, with the same result as
  doing them afresh. The browser package's `Chant` uses them, and passes only the lines that
  changed from the engine to the page.
- Browser package: `Chant.update`, `sourceAt`, `elementsAt`, `layout(…, { timeline: false,
  svg: "lines", ids: false })`, and UTF-16 offsets and fixes on diagnostics; an example
  editor in `crates/neuma-wasm/examples/editor.html`.
- Mobile bindings: `Chant.sourceAt`, `Chant.elementsAt`, UTF-16 offsets and fixes.
