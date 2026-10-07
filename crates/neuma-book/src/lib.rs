//! Print booklets from neuma: an ordered list of pieces (scores, psalms set to a tone, rubrics
//! and text) set on fixed-size pages, as SVG per page and as PDF.
//!
//! ```no_run
//! use neuma_book::{Book, Fonts, typeset};
//!
//! let mut book = Book::parse(&std::fs::read_to_string("compline.book").unwrap()).unwrap();
//! book.resolve(std::path::Path::new(".")).unwrap();
//! let files = neuma_book::font_files(&book.settings).unwrap();
//! let fonts = Fonts::new(&files);
//! let doc = typeset(&book, &fonts);
//! std::fs::write("compline.pdf", doc.pdf(&fonts)).unwrap();
//! ```
//!
//! The pipeline: [`Book::parse`] reads the `.book` format, composing sets each piece into
//! blocks (a staff line or a line of text each, with keep-together rules), pagination puts
//! the blocks on pages with running headers and page numbers, and [`Document::pdf`] and
//! [`Document::svg`] write the pages.

pub mod book;
mod compose;
pub mod font;
pub mod page;
mod paginate;
mod pdf;
mod svg;
mod text;
mod times;

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
    red: [u8; 3],
    text_as_paths: bool,
}

/// Sets the book's pieces on pages. The book's files must be read first ([`Book::resolve`]).
pub fn typeset(book: &Book, fonts: &Fonts) -> Document {
    let (blocks, problems) = compose::blocks(book, fonts);
    let pages = paginate::paginate(&blocks, &book.settings, fonts);
    let title = book.pieces.iter().find_map(|p| match p {
        Piece::Title(t) => Some(t.clone()),
        _ => None,
    });
    Document {
        pages,
        problems,
        title,
        red: book.settings.red,
        text_as_paths: book.settings.text_as_paths,
    }
}

impl Document {
    /// Draw text as outlines in the PDF and SVG, whatever the book says.
    pub fn set_text_as_paths(&mut self, on: bool) {
        self.text_as_paths = on;
    }

    /// The whole booklet as a PDF.
    pub fn pdf(&self, fonts: &Fonts) -> Vec<u8> {
        pdf::write(
            &self.pages,
            fonts,
            &pdf::PdfOptions {
                text_as_paths: self.text_as_paths,
                red: self.red,
            },
            self.title.as_deref(),
        )
    }

    /// One page (0-based) as SVG.
    pub fn svg(&self, page: usize, fonts: &Fonts) -> Option<String> {
        self.pages.get(page).map(|p| svg::page(p, fonts, self.red, self.text_as_paths))
    }
}

/// Reads the font files the settings name, or finds EB Garamond 12 when they name none. With
/// neither, the result is empty and [`Fonts::new`] falls back to the PDF standard Times faces.
pub fn font_files(s: &Settings) -> Result<FontFiles, String> {
    match &s.font {
        Some(regular) => FontFiles::load([
            Some(regular.as_path()),
            s.font_italic.as_deref(),
            s.font_bold.as_deref(),
            s.font_bold_italic.as_deref(),
        ]),
        None => Ok(FontFiles::find_default().unwrap_or_default()),
    }
}
