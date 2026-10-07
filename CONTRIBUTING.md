# Contributing

## Checks

CI runs these on every pull request; run them before you push:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

The browser package has its own build and smoke test (see
[crates/neuma-wasm](crates/neuma-wasm/README.md)):

```sh
cargo build -p neuma-wasm --target wasm32-unknown-unknown --profile wasm
node crates/neuma-wasm/build.mjs
node crates/neuma-wasm/test.mjs
```

## Golden snapshots

`cargo test` compares the engraving of about 30 reference scores, at two widths, with text
snapshots of their display lists in
[crates/neuma/tests/golden/snapshots](crates/neuma/tests/golden/README.md). When a change
to the engraving is intended, rewrite them and check the diff before you commit it:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p neuma --test golden
git diff crates/neuma/tests/golden/snapshots
```

Add a reference score by putting its `.gabc` file in `crates/neuma/tests/golden` and
running the same command. Only add scores whose source you know is free to share.

## Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets. It is its own
workspace, outside the main one, because cargo-fuzz needs a nightly toolchain.

| Target | What it checks |
| --- | --- |
| `parse` | The parser and `summarize` never panic, and diagnostics point inside the source. |
| `layout` | Parse, engrave and lay out at two widths and scales taken from the input's last bytes (including 0, negative, infinite and NaN widths), then draw the display list, note map and SVG. |
| `round_trip` | Writing a parsed score as GABC and parsing it again reaches a fixed point after one round, with no notes lost. |

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run -O parse corpus/parse ../crates/neuma/tests/corpus ../crates/neuma/tests/golden
```

The first directory collects the fuzzer's own corpus (ignored by git); the others seed it
with real scores. Add `-- -max_total_time=300` to stop after five minutes. A crash leaves its
input in `fuzz/artifacts/<target>/`; shrink it with `cargo +nightly fuzz tmin -O <target>
<file>`, fix the bug, and add the shrunk input to a regression test next to the code it
exercises.
