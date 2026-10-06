//! Print booklets from neuma: an ordered list of pieces (scores, psalms set to a tone, rubrics
//! and text) set on fixed-size pages.
//!
//! [`Book::parse`] reads the `.book` format: page settings, then the pieces in order.

pub mod book;

pub use book::{Book, BookError, PageNumbers, Piece, PsalmSet, Settings, Source};
