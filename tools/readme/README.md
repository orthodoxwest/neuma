# README images

The images in the repository's [README](../../README.md) live in
[docs/images](../../docs/images) and are drawn by neuma itself. To make them again after an
engraving change:

```sh
# hero.svg, reflow.svg, psalm-tone.svg and timeline.svg
cargo run -p readme-images [-- OUT_DIR [FONT_DIR]]

# booklet.png: three pages of examples/compline, rasterized with pdftoppm
sh tools/readme/booklet.sh

# editor.gif: the example editor recorded in headless Chromium
cargo build -p neuma-wasm --target wasm32-unknown-unknown --profile wasm
node crates/neuma-wasm/build.mjs
node tools/readme/record-editor.mjs

# pointed-psalm.png: the pointed psalm example, drawn in headless Chromium (build as above)
node tools/readme/record-psalm.mjs
```

What each needs:

- **The SVGs** need EB Garamond 12 (`fonts-ebgaramond` on Debian and Ubuntu; `FONT_DIR`
  defaults to `/usr/share/fonts/opentype/ebgaramond`). Lyrics are measured with neuma's
  built-in EB Garamond 12 metrics and drawn as outlines from those files, because GitHub shows
  SVG through `<img>`, which loads no fonts. Each image has a white card behind it so it reads
  in light and dark themes. `reflow.svg` and `timeline.svg` animate with SMIL, which works in
  `<img>`.
- **`booklet.sh`** needs the same fonts, Poppler (`pdftoppm`) and ImageMagick (`convert`).
- **`record-editor.mjs`** needs [Playwright](https://playwright.dev) with Chromium and
  `ffmpeg`. `PLAYWRIGHT` names the module to import if it isn't installed where Node finds it,
  and `FONTS_DIR` serves the Google Fonts stylesheet and files from a local directory (see the
  script's header). It hides the editor's timing readout, which depends on the machine.
  **`record-psalm.mjs`** needs Playwright with Chromium, and takes the same `PLAYWRIGHT` and
  `FONTS_DIR`.

The scores: [scores/puer-natus.gabc](scores/puer-natus.gabc) (the example editor's sample,
with the annotation added) for the hero, the golden test scores
`salve-regina-simple.gabc` and `regina-caeli-simple.gabc` from
[crates/neuma/tests/golden](../../crates/neuma/tests/golden) for the reflow and timeline, and
Psalm 117 from the Book of Common Prayer psalter, [scores/psalm-117.txt](scores/psalm-117.txt), for the
psalm tone. The pointed psalm is Psalm 4 from the same psalter, partly hand-pointed, with the
antiphon of examples/compline, both in the example page. All are settings of public-domain texts and melodies.

The crate's library target has no code of its own: it includes the README so that
`cargo test -p readme-images --doc` compiles and runs the README's Rust snippets.
