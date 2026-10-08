# neuma-metrics

Builds the font metrics tables the [neuma](https://crates.io/crates/neuma) chant engine
measures lyrics with. neuma spaces the text under the notes from each glyph's advance and the
kerning between pairs of characters, so its line breaks match what a renderer draws with the
same font. It has tables for EB Garamond built in; this tool makes one for another font.

```sh
cargo install neuma-metrics

neuma-metrics --regular Font-Regular.otf --italic Font-Italic.otf -o font.nmet
```

It takes `--regular` and, optionally, `--italic`, `--bold` and `--bold-italic` TrueType or
OpenType files. It shapes each character, and each pair of characters lyrics are likely to
set, with the features renderers use for lyrics (kerning on, ligatures off), and writes the
advances and kerning, with no outlines. A table records the SHA-256 of each font it was
measured from.

Load the table with `MetricsTable::from_bytes` and engrave with it:

```rust,no_run
use std::sync::Arc;
use neuma::{Chant, ChantOptions, MetricsTable};

let table = MetricsTable::from_bytes(&std::fs::read("font.nmet").unwrap()).unwrap();
let options = ChantOptions::default().with_measure(Arc::new(table));
let chant = Chant::with_options("(c4) Al(f)le(gf)lú(gh)ia.(g.) (::)", options);
```

The pages that show the chant must then set its lyrics in that font.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/orthodoxwest/neuma/blob/main/LICENSE-APACHE))
- MIT license ([LICENSE-MIT](https://github.com/orthodoxwest/neuma/blob/main/LICENSE-MIT))

at your option.
