# Contributing

## Checks

CI runs these on every pull request; run them before you push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo deny --all-features check    # after changing a dependency; see deny.toml
```

CI also runs the tests on the oldest supported Rust, the `rust-version` in
[Cargo.toml](Cargo.toml).
[clippy.toml](clippy.toml) bans `mul_add` and the libm functions (`powf`, `exp`, `ln`, `sin`
and the rest), whose results vary between platforms, because the engraving must come out
byte-identical everywhere. A use whose result can't reach the output, or that has no
deterministic replacement, needs an `#[allow(clippy::disallowed_methods)]` that says why.

The browser package has its own build and smoke test (see
[crates/neuma-wasm](crates/neuma-wasm/README.md)):

```sh
cargo build -p neuma-wasm --target wasm32-unknown-unknown --profile wasm
node crates/neuma-wasm/build.mjs
node crates/neuma-wasm/test.mjs
```

CI builds it with wasm-opt (`npm install binaryen@132.0.0`, then `node
crates/neuma-wasm/build.mjs node_modules/binaryen/bin/wasm-opt`) and fails if
`dist/neuma.mjs` grows past its budget, 470 KiB (`NEUMA_MJS_BUDGET` in
[ci.yml](.github/workflows/ci.yml)). If a change needs more, raise the budget in the same pull
request and say why.

The README's Rust snippets run as doctests of [tools/readme](tools/readme/README.md), so
`cargo test` fails when an API change breaks one. After a change to the engraving, redraw the
README's images as that page describes.

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

## Corpus run

A nightly workflow ([corpus.yml](.github/workflows/corpus.yml)) runs every chant in
[GregoBase](https://gregobase.selapa.net/), about 18,800 scores, through parse, engrave,
layout at three widths, the display list, note map, SVG and a GABC round trip. It fails on
a panic, on a file slower than 5 seconds or still running after 60, and on a round trip
that doesn't reach a fixed point. Its report (in the run's summary and the `corpus-report`
artifact) counts diagnostics by code against
[tools/corpus/gregobase-counts.tsv](tools/corpus/gregobase-counts.tsv). The whole run takes
under a minute once built.

GregoBase isn't in this repository. Its scores are mostly transcriptions of public-domain
chant, but the dump has no license of its own and some entries are marked as still under
copyright, so the workflow fetches the public SQL dump from the
[GregoBase repository](https://github.com/gregorio-project/GregoBase) at a pinned commit,
checks its SHA-256, skips the chants GregoBase marks as copyrighted, and keeps the extracted
scores only in the runner's temporary directory. Nothing from the dump is cached, committed
or uploaded. To move to a newer dump, update the commit and checksum in the workflow and
the counts file in the same pull request.

To run it locally:

```sh
curl -sSfLO https://raw.githubusercontent.com/gregorio-project/GregoBase/2ebcda3f523f9b19933d59fa32bb4215cd8e7675/gregobase_online.sql
python3 -I tools/corpus/gregobase.py gregobase_online.sql /tmp/gregobase
cargo run --release -p neuma --example corpus -- /tmp/gregobase \
    --baseline tools/corpus/gregobase-counts.tsv --counts tools/corpus/gregobase-counts.tsv
```

`--counts` rewrites the baseline; commit it when a change to the diagnostics is intended.
The example takes any directory of `.gabc` files.
