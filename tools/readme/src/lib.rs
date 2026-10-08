//! Runs the Rust snippets in the repository's README and the crates' READMEs as doctests, so
//! they keep compiling against the current API: `cargo test -p readme-images --doc`. The
//! image generator is the binary (`src/main.rs`).

#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
pub struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../../crates/neuma/README.md")]
pub struct NeumaReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../../crates/neuma-tones/README.md")]
pub struct TonesReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../../crates/neuma-metrics/README.md")]
pub struct MetricsReadmeDoctests;
