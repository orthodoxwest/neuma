//! Runs the Rust snippets in the repository's README as doctests, so they keep compiling
//! against the current API: `cargo test -p readme-images --doc`. The image generator is the
//! binary (`src/main.rs`).

#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
pub struct ReadmeDoctests;
