//! Print booklets from neuma: an ordered list of pieces (scores, psalms set to a tone, rubrics
//! and text) set on fixed-size pages.
//!
//! [`Book::parse`] reads the `.book` format: page settings, then the pieces in order.
//! [`font`] loads and shapes the text face (and measures neuma's lyrics with it), and
//! [`text`] breaks paragraphs into lines of positioned [`page::Op`]s.

pub mod book;
pub mod font;
pub mod page;
pub mod text;

pub use book::{Book, BookError, PageNumbers, Piece, PsalmSet, Settings, Source};
pub use font::{FontFiles, Fonts};
pub use page::{Color, Op, Page};
