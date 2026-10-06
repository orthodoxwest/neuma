//! Print booklets from neuma: an ordered list of pieces (scores, psalms set to a tone, rubrics
//! and text) set on fixed-size pages.
//!
//! The pipeline: [`Book::parse`] reads the `.book` format, [`compose`] sets each piece into
//! blocks (a staff line or a line of text each, with keep-together rules), and [`paginate`]
//! puts the blocks on pages with running headers and page numbers. [`font`] loads and shapes
//! the text face (and measures neuma's lyrics with it).

pub mod book;
pub mod compose;
pub mod font;
pub mod page;
pub mod paginate;
pub mod text;

pub use book::{Book, BookError, PageNumbers, Piece, PsalmSet, Settings, Source};
pub use compose::Problem;
pub use font::{FontFiles, Fonts};
pub use page::{Color, Op, Page};

/// A typeset booklet.
#[derive(Clone, Debug)]
pub struct Document {
    pub pages: Vec<Page>,
    /// Problems in the pieces: GABC diagnostics, pointing that needs checking, unknown tones.
    pub problems: Vec<Problem>,
    pub title: Option<String>,
}

/// Sets the book's pieces on pages. The book's files must be read first ([`Book::resolve`]).
pub fn typeset(book: &Book, fonts: &Fonts) -> Document {
    let (blocks, problems) = compose::blocks(book, fonts);
    let pages = paginate::paginate(&blocks, &book.settings, fonts);
    let title = book.pieces.iter().find_map(|p| match p {
        Piece::Title(t) => Some(t.clone()),
        _ => None,
    });
    Document { pages, problems, title }
}
