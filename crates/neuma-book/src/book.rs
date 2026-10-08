//! The `.book` format: settings, then an ordered list of pieces.
//!
//! ```text
//! # A comment
//! page: a5
//! margins: 15mm 14mm 18mm
//! header: Compline
//!
//! title: Compline
//! rubric: All stand.
//! score: antiphons/miserere.gabc
//! psalm tone=8.G gloria: psalms/4.txt
//! text dropcap:
//!     Brethren, be sober, be vigilant; because your adversary the devil,
//!     as a roaring lion, walketh about, seeking whom he may devour.
//! break
//! ```
//!
//! Each line is `name: value` or `name option option=value: value`. Lines indented under it
//! continue its value. The crate README documents every setting and piece.

use std::fmt;
use std::path::{Path, PathBuf};

/// A length in points.
pub type Pt = f32;

/// Where a score's or psalm's text comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Path(PathBuf),
    Inline(String),
}

/// How a psalm is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PsalmSet {
    /// Pointed text, with the tone shown once as a small score (as hand-pointed psalters print).
    #[default]
    Pointed,
    /// The first verse as chant, the rest as pointed text.
    First,
    /// Every verse as chant.
    Chant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Psalm {
    pub source: Source,
    pub tone: String,
    /// A file of tone blocks to look `tone` up in before the built-in tones.
    pub tone_file: Option<PathBuf>,
    pub intone: neuma_tones::Intone,
    pub gloria: bool,
    /// `None` takes the book's `psalms:` setting.
    pub set: Option<PsalmSet>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// A large centered title. Pages with a title have no running header.
    Title(String),
    /// A centered section heading; it stays with what follows.
    Heading(String),
    /// Red italic directions; they stay with what follows.
    Rubric(String),
    /// Paragraphs (separated by `\n\n`), justified, with an optional drop cap. With
    /// `lines`, each `\n` ends a line, as in verse.
    Text {
        text: String,
        dropcap: bool,
        center: bool,
        lines: bool,
    },
    Score {
        source: Source,
        /// Drop-cap height in staves; `None` takes the book's `initial:`.
        initial: Option<u8>,
    },
    Psalm(Psalm),
    /// Changes the running header from the next page on (or this one, at its top).
    Header(String),
    /// Starts a new page.
    Break,
    /// Vertical space.
    Space(Pt),
}

/// Where page numbers go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageNumbers {
    #[default]
    Bottom,
    /// The outside top corner, beside the running header.
    Outer,
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub width: Pt,
    pub height: Pt,
    /// Top, right, bottom, left.
    pub margins: [Pt; 4],
    /// Swap the left and right margins on even pages, for printing both sides.
    pub mirror: bool,
    pub font: Option<PathBuf>,
    pub font_italic: Option<PathBuf>,
    pub font_bold: Option<PathBuf>,
    pub font_bold_italic: Option<PathBuf>,
    pub text_size: Pt,
    /// Height of the staff from its top line to its bottom line.
    pub staff_size: Pt,
    /// Lyric size; `None` is the text size.
    pub lyric_size: Option<Pt>,
    pub page_numbers: PageNumbers,
    /// Draw text as outlines, so the output needs no font.
    pub text_as_paths: bool,
    /// Default drop-cap height for scores, in staves (0 for none).
    pub initial: u8,
    pub psalms: PsalmSet,
    /// The rubric colour, as `#rrggbb`.
    pub red: [u8; 3],
    /// Space between lines of text, as a multiple of the text size.
    pub leading: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        let (width, height) = paper("a5").unwrap_or((419.53, 595.28));
        Settings {
            width,
            height,
            margins: [mm(16.0), mm(14.0), mm(18.0), mm(14.0)],
            mirror: false,
            font: None,
            font_italic: None,
            font_bold: None,
            font_bold_italic: None,
            text_size: 11.0,
            staff_size: mm(6.5),
            lyric_size: None,
            page_numbers: PageNumbers::Bottom,
            text_as_paths: false,
            initial: 1,
            psalms: PsalmSet::Pointed,
            red: [0xa3, 0x21, 0x1c],
            leading: 1.3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Book {
    pub settings: Settings,
    pub pieces: Vec<Piece>,
    /// Where each parsed piece is in the file, by index ([`Book::origin`]).
    origins: Vec<Origin>,
}

/// Where a piece is in its `.book` file, so that a problem in it can be reported at the
/// book's own lines ([`Book::origin`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    /// The 1-based line of the piece's entry.
    line: usize,
    /// Each line of the piece's text on indented lines under its entry, if it has any: its
    /// 1-based line in the book, and the bytes of indentation taken off it.
    body: Vec<(usize, usize)>,
}

impl Origin {
    /// The 1-based line of the piece's entry in the book.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// The 1-based line and column in the book (the column in characters) of byte `offset`
    /// in `text`, the piece's text from the indented lines under its entry; `None` if the
    /// piece has no such text.
    #[must_use]
    pub fn position(&self, text: &str, offset: usize) -> Option<(usize, usize)> {
        let (line, col) = neuma::diag::line_col(text, offset);
        let &(at, indent) = self.body.get(line - 1)?;
        Some((at, col + indent))
    }
}

/// A problem in a `.book` file, with its 1-based line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for BookError {}

fn mm(v: f32) -> Pt {
    v * 72.0 / 25.4
}

/// A named paper size, portrait, in points.
pub fn paper(name: &str) -> Option<(Pt, Pt)> {
    Some(match name.to_ascii_lowercase().as_str() {
        "a4" => (mm(210.0), mm(297.0)),
        "a5" => (mm(148.0), mm(210.0)),
        "a6" => (mm(105.0), mm(148.0)),
        "b5" => (mm(176.0), mm(250.0)),
        "letter" => (612.0, 792.0),
        "half-letter" | "statement" => (396.0, 612.0),
        "legal" => (612.0, 1008.0),
        _ => return None,
    })
}

/// A length such as `12pt`, `5mm`, `1.2cm` or `0.5in`; a bare number is points.
pub fn length(s: &str) -> Option<Pt> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let v: f32 = num.trim().parse().ok()?;
    let pt = match unit {
        "" | "pt" => v,
        "mm" => mm(v),
        "cm" => mm(v * 10.0),
        "in" => v * 72.0,
        "pc" => v * 12.0,
        _ => return None,
    };
    // Checked after the unit, which can overflow a finite number.
    (pt.is_finite() && pt >= 0.0).then_some(pt)
}

/// The largest page side PDF allows, in points (200 inches).
pub const MAX_PAGE: Pt = 14_400.0;
/// The least room the margins must leave for text, in points.
const MIN_MEASURE: Pt = 36.0;

fn color(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

fn yes_no(s: &str) -> Option<bool> {
    match s.trim() {
        "yes" | "true" | "on" => Some(true),
        "no" | "false" | "off" => Some(false),
        _ => None,
    }
}

fn psalm_set(s: &str) -> Option<PsalmSet> {
    match s {
        "pointed" => Some(PsalmSet::Pointed),
        "first" => Some(PsalmSet::First),
        "chant" => Some(PsalmSet::Chant),
        _ => None,
    }
}

/// One entry: its name, options, value, and the line it starts on.
struct Entry {
    line: usize,
    name: String,
    options: Vec<(String, Option<String>)>,
    /// The value on the entry's own line.
    value: String,
    /// The indented lines under it, with the common indentation removed.
    body: Vec<String>,
    /// Each line of `body`'s 1-based line in the file, and the bytes of indentation removed.
    body_lines: Vec<(usize, usize)>,
}

/// Bytes of leading spaces and tabs.
fn indent_of(l: &str) -> usize {
    l.len() - l.trim_start_matches([' ', '\t']).len()
}

fn entries(src: &str) -> Result<Vec<Entry>, BookError> {
    let mut out: Vec<Entry> = Vec::new();
    for (i, raw) in src.lines().enumerate() {
        let line = i + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let indented = raw.starts_with([' ', '\t']);
        if indented {
            if raw.trim_matches([' ', '\t']).is_empty() {
                if let Some(e) = out.last_mut() {
                    e.body.push(String::new());
                    e.body_lines.push((line, 0));
                }
                continue;
            }
            match out.last_mut() {
                Some(e) => {
                    e.body.push(raw.to_string());
                    e.body_lines.push((line, 0));
                }
                None => {
                    return Err(BookError {
                        line,
                        message: "an indented line continues the entry above it, but there is none".into(),
                    });
                }
            }
            continue;
        }
        let t = raw.trim();
        if t.is_empty() {
            // A blank line inside an entry's indented text separates paragraphs; trailing
            // ones are dropped below.
            if let Some(e) = out.last_mut() {
                e.body.push(String::new());
                e.body_lines.push((line, 0));
            }
            continue;
        }
        if t.starts_with('#') {
            continue;
        }
        let (head, value) = match t.split_once(':') {
            Some((h, v)) => (h, v.trim()),
            None => (t, ""),
        };
        let mut words = head.split_whitespace();
        let name = words.next().unwrap_or_default().to_ascii_lowercase();
        let options = words
            .map(|w| match w.split_once('=') {
                Some((k, v)) => (k.to_ascii_lowercase(), Some(v.to_string())),
                None => (w.to_ascii_lowercase(), None),
            })
            .collect();
        out.push(Entry {
            line,
            name,
            options,
            value: value.to_string(),
            body: Vec::new(),
            body_lines: Vec::new(),
        });
    }
    for e in &mut out {
        while e.body.last().is_some_and(|l| l.is_empty()) {
            e.body.pop();
            e.body_lines.pop();
        }
        let indent = e.body.iter().filter(|l| !l.is_empty()).map(|l| indent_of(l)).min().unwrap_or(0);
        for (l, (_, removed)) in e.body.iter_mut().zip(&mut e.body_lines) {
            if !l.is_empty() {
                // Only ASCII spaces and tabs indent, so `indent` is a char boundary; other
                // whitespace (a no-break space) is text.
                *removed = indent.min(indent_of(l));
                *l = l[*removed..].trim_end_matches([' ', '\t']).to_string();
            }
        }
    }
    Ok(out)
}

impl Entry {
    fn err(&self, message: impl Into<String>) -> BookError {
        BookError {
            line: self.line,
            message: message.into(),
        }
    }

    /// The value as running text: its own line and the lines under it, joined by spaces
    /// (or by `\n`, keeping `lines`); a blank line under it starts a new paragraph (`\n\n`).
    fn text(&self, lines: bool) -> String {
        let mut out = self.value.clone();
        for l in &self.body {
            if l.is_empty() {
                if !out.ends_with("\n\n") {
                    out.truncate(out.trim_end().len());
                    out.push_str("\n\n");
                }
            } else {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push(if lines { '\n' } else { ' ' });
                }
                out.push_str(l);
            }
        }
        out.trim().to_string()
    }

    /// A file named on the entry's line, or the lines under it as they stand.
    fn source(&self) -> Result<Source, BookError> {
        match (self.value.is_empty(), self.body.is_empty()) {
            (false, true) => Ok(Source::Path(PathBuf::from(&self.value))),
            (true, false) => Ok(Source::Inline(self.body.join("\n"))),
            (true, true) => Err(self.err(format!("`{}` needs a file name, or its text on indented lines below", self.name))),
            (false, false) => Err(self.err(format!("`{}` takes a file name or indented text, not both", self.name))),
        }
    }

    fn no_options(&self) -> Result<(), BookError> {
        match self.options.first() {
            Some((k, _)) => Err(self.err(format!("`{}` takes no option `{k}`", self.name))),
            None => Ok(()),
        }
    }

    fn value_only(&self) -> Result<&str, BookError> {
        self.no_options()?;
        if !self.body.is_empty() {
            return Err(self.err(format!("`{}` takes its value on one line", self.name)));
        }
        Ok(&self.value)
    }
}

impl Book {
    /// Parses a `.book` file. File names in it stay as written; [`Book::resolve`] reads them.
    /// A byte-order mark at the start is skipped.
    pub fn parse(src: &str) -> Result<Book, BookError> {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        let mut book = Book::default();
        let mut started = false;
        for e in entries(src)? {
            let s = &mut book.settings;
            let len = |e: &Entry| -> Result<Pt, BookError> {
                let v = e.value_only()?;
                length(v).ok_or_else(|| e.err(format!("`{v}` is not a length such as 11pt or 6mm")))
            };
            let setting = matches!(
                e.name.as_str(),
                "page"
                    | "margins"
                    | "mirror"
                    | "font"
                    | "font-italic"
                    | "font-bold"
                    | "font-bold-italic"
                    | "text-size"
                    | "staff-size"
                    | "lyric-size"
                    | "page-numbers"
                    | "text-as-paths"
                    | "initial"
                    | "psalms"
                    | "red"
                    | "leading"
            );
            if setting && started {
                return Err(e.err(format!("`{}` is a setting; put it before the first piece", e.name)));
            }
            match e.name.as_str() {
                "page" => {
                    let v = e.value_only()?;
                    let words: Vec<&str> = v.split_whitespace().collect();
                    let (w, h) = match words.as_slice() {
                        [name] => paper(name),
                        [name, "landscape"] => paper(name).map(|(w, h)| (h, w)),
                        [w, h] => length(w).zip(length(h)),
                        _ => None,
                    }
                    .ok_or_else(|| e.err(format!("`{v}`: give a paper size (a4, a5, letter …) or a width and a height")))?;
                    if !(1.0..=MAX_PAGE).contains(&w) || !(1.0..=MAX_PAGE).contains(&h) {
                        return Err(e.err(format!("`{v}`: each side of a page must be more than 0 and at most 200in")));
                    }
                    s.width = w;
                    s.height = h;
                }
                "margins" => {
                    let v = e.value_only()?;
                    let ls: Option<Vec<Pt>> = v.split_whitespace().map(length).collect();
                    s.margins = match ls.as_deref() {
                        Some([a]) => [*a; 4],
                        Some([v, h]) => [*v, *h, *v, *h],
                        Some([t, h, b]) => [*t, *h, *b, *h],
                        Some([t, r, b, l]) => [*t, *r, *b, *l],
                        _ => return Err(e.err("margins take one to four lengths: top, right, bottom, left")),
                    };
                }
                "mirror" => s.mirror = yes_no(e.value_only()?).ok_or_else(|| e.err("mirror takes yes or no"))?,
                "font" => s.font = Some(PathBuf::from(e.value_only()?)),
                "font-italic" => s.font_italic = Some(PathBuf::from(e.value_only()?)),
                "font-bold" => s.font_bold = Some(PathBuf::from(e.value_only()?)),
                "font-bold-italic" => s.font_bold_italic = Some(PathBuf::from(e.value_only()?)),
                "text-size" => s.text_size = len(&e)?.max(1.0),
                "staff-size" => s.staff_size = len(&e)?.max(1.0),
                "lyric-size" => s.lyric_size = Some(len(&e)?.max(1.0)),
                "page-numbers" => {
                    s.page_numbers = match e.value_only()? {
                        "bottom" => PageNumbers::Bottom,
                        "outer" => PageNumbers::Outer,
                        "none" => PageNumbers::None,
                        _ => return Err(e.err("page-numbers takes bottom, outer or none")),
                    }
                }
                "text-as-paths" => s.text_as_paths = yes_no(e.value_only()?).ok_or_else(|| e.err("text-as-paths takes yes or no"))?,
                "initial" => {
                    s.initial = e
                        .value_only()?
                        .parse()
                        .ok()
                        .filter(|n| *n <= 4)
                        .ok_or_else(|| e.err("initial takes a number of staves from 0 to 4"))?
                }
                "psalms" => s.psalms = psalm_set(e.value_only()?).ok_or_else(|| e.err("psalms takes pointed, first or chant"))?,
                "red" => s.red = color(e.value_only()?).ok_or_else(|| e.err("red takes a colour such as #a3211c"))?,
                "leading" => {
                    s.leading = e
                        .value_only()?
                        .parse()
                        .ok()
                        .filter(|v: &f32| (0.8..=3.0).contains(v))
                        .ok_or_else(|| e.err("leading takes a multiple of the text size, such as 1.3"))?
                }
                _ => {
                    started = true;
                    book.pieces.push(piece(&e)?);
                    book.origins.push(Origin {
                        line: e.line,
                        body: e.body_lines.clone(),
                    });
                }
            }
        }
        let s = &book.settings;
        let measure = s.width - s.margins[1] - s.margins[3];
        let room = s.height - s.margins[0] - s.margins[2];
        if measure < MIN_MEASURE || room < MIN_MEASURE {
            return Err(BookError {
                line: 0,
                message: format!(
                    "the margins leave {:.0}pt by {:.0}pt for text on a {:.0}pt by {:.0}pt page; they need to leave at least {MIN_MEASURE:.0}pt each way",
                    measure.max(0.0),
                    room.max(0.0),
                    s.width,
                    s.height
                ),
            });
        }
        Ok(book)
    }

    /// Where piece `piece` is in the file it was parsed from, or `None` for a piece the
    /// parser didn't make.
    #[must_use]
    pub fn origin(&self, piece: usize) -> Option<&Origin> {
        self.origins.get(piece)
    }

    /// Reads every file the book names, relative to `base` (the book's directory), so the
    /// pieces hold their text.
    pub fn resolve(&mut self, base: &Path) -> Result<(), BookError> {
        let read = |p: &PathBuf| -> Result<String, BookError> {
            let path = base.join(p);
            std::fs::read_to_string(&path).map_err(|err| BookError {
                line: 0,
                message: format!("{}: {err}", path.display()),
            })
        };
        let s = &mut self.settings;
        for f in [&mut s.font, &mut s.font_italic, &mut s.font_bold, &mut s.font_bold_italic]
            .into_iter()
            .flatten()
        {
            *f = base.join(&*f);
        }
        for piece in &mut self.pieces {
            match piece {
                Piece::Score { source, .. } => {
                    if let Source::Path(p) = source {
                        *source = Source::Inline(read(p)?);
                    }
                }
                Piece::Psalm(ps) => {
                    if let Source::Path(p) = &ps.source {
                        ps.source = Source::Inline(read(p)?);
                    }
                    if let Some(t) = &mut ps.tone_file {
                        *t = base.join(&*t);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn piece(e: &Entry) -> Result<Piece, BookError> {
    Ok(match e.name.as_str() {
        "title" => {
            e.no_options()?;
            Piece::Title(e.text(false))
        }
        "heading" => {
            e.no_options()?;
            Piece::Heading(e.text(false))
        }
        "rubric" => {
            e.no_options()?;
            Piece::Rubric(e.text(false))
        }
        "header" => {
            e.no_options()?;
            Piece::Header(e.text(false))
        }
        "text" => {
            let mut dropcap = false;
            let mut center = false;
            let mut lines = false;
            for (k, v) in &e.options {
                match (k.as_str(), v) {
                    ("dropcap", None) => dropcap = true,
                    ("center", None) => center = true,
                    ("lines", None) => lines = true,
                    _ => return Err(e.err(format!("text takes the options dropcap, center and lines, not `{k}`"))),
                }
            }
            Piece::Text {
                text: e.text(lines),
                dropcap,
                center,
                lines,
            }
        }
        "score" => {
            let mut initial = None;
            for (k, v) in &e.options {
                match (k.as_str(), v.as_deref().map(str::parse::<u8>)) {
                    ("initial", Some(Ok(n))) if n <= 4 => initial = Some(n),
                    _ => return Err(e.err(format!("score takes the option initial=N (0 to 4), not `{k}`"))),
                }
            }
            Piece::Score {
                source: e.source()?,
                initial,
            }
        }
        "psalm" => {
            let mut tone = None;
            let mut tone_file = None;
            let mut intone = neuma_tones::Intone::FirstVerse;
            let mut gloria = false;
            let mut set = None;
            for (k, v) in &e.options {
                match (k.as_str(), v.as_deref()) {
                    ("tone", Some(t)) => tone = Some(t.to_string()),
                    ("tone-file", Some(t)) => tone_file = Some(PathBuf::from(t)),
                    ("gloria", None) => gloria = true,
                    ("intone", Some("first")) => intone = neuma_tones::Intone::FirstVerse,
                    ("intone", Some("every")) => intone = neuma_tones::Intone::EveryVerse,
                    ("intone", Some("never")) => intone = neuma_tones::Intone::Never,
                    ("set", Some(v)) => {
                        set = Some(psalm_set(v).ok_or_else(|| e.err("set takes pointed, first or chant"))?);
                    }
                    _ => {
                        return Err(e.err(format!(
                            "psalm takes tone=NAME, tone-file=FILE, intone=first|every|never, set=pointed|first|chant \
                             and gloria, not `{k}`"
                        )));
                    }
                }
            }
            let tone = tone.ok_or_else(|| e.err("a psalm needs tone=NAME, such as tone=8.G"))?;
            Piece::Psalm(Psalm {
                source: e.source()?,
                tone,
                tone_file,
                intone,
                gloria,
                set,
            })
        }
        "break" => {
            e.no_options()?;
            if !e.value.is_empty() || !e.body.is_empty() {
                return Err(e.err("break takes no value"));
            }
            Piece::Break
        }
        "space" => {
            let v = e.value_only()?;
            Piece::Space(length(v).ok_or_else(|| e.err(format!("`{v}` is not a length such as 12pt")))?)
        }
        other => return Err(e.err(format!("unknown entry `{other}`"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths() {
        assert_eq!(length("12"), Some(12.0));
        assert_eq!(length("1in"), Some(72.0));
        assert!((length("25.4mm").unwrap() - 72.0).abs() < 1e-3);
        assert_eq!(length("3furlongs"), None);
        assert_eq!(length("-3pt"), None);
        // Finite before the unit, infinite after it.
        assert_eq!(length("3e38in"), None);
    }

    #[test]
    fn settings_and_pieces() {
        let book = Book::parse(
            "# comment\npage: a4\nmargins: 10mm 20mm\ntext-size: 12pt\nred: #ff0000\n\n\
             title: Compline\nrubric: All stand.\nscore initial=2: a.gabc\npsalm tone=8.G gloria set=chant: p.txt\n\
             text dropcap:\n    Brethren, be sober,\n    be vigilant.\n\n    A second paragraph.\nbreak\nheader: Notes\n\
             text lines center:\n  One,\n  two.\n",
        )
        .unwrap();
        let s = &book.settings;
        assert!((s.width - 595.28).abs() < 0.01);
        assert!((s.margins[1] - mm(20.0)).abs() < 1e-3);
        assert_eq!(s.text_size, 12.0);
        assert_eq!(s.red, [255, 0, 0]);
        assert_eq!(book.pieces[0], Piece::Title("Compline".into()));
        assert_eq!(
            book.pieces[2],
            Piece::Score {
                source: Source::Path("a.gabc".into()),
                initial: Some(2)
            }
        );
        let Piece::Psalm(p) = &book.pieces[3] else { panic!() };
        assert_eq!(p.tone, "8.G");
        assert!(p.gloria);
        assert_eq!(p.set, Some(PsalmSet::Chant));
        assert_eq!(
            book.pieces[4],
            Piece::Text {
                text: "Brethren, be sober, be vigilant.\n\nA second paragraph.".into(),
                dropcap: true,
                center: false,
                lines: false
            }
        );
        assert_eq!(book.pieces[5], Piece::Break);
        assert_eq!(book.pieces[6], Piece::Header("Notes".into()));
        assert_eq!(
            book.pieces[7],
            Piece::Text {
                text: "One,\ntwo.".into(),
                dropcap: false,
                center: true,
                lines: true
            }
        );
    }

    #[test]
    fn inline_sources_keep_their_lines() {
        let book = Book::parse("score:\n    name: x;\n    %%\n    (c4) A(g)\npsalm tone=1.D:\n  1 a * b\n  2 c * d\n").unwrap();
        assert_eq!(
            book.pieces[0],
            Piece::Score {
                source: Source::Inline("name: x;\n%%\n(c4) A(g)".into()),
                initial: None
            }
        );
        let Piece::Psalm(p) = &book.pieces[1] else { panic!() };
        assert_eq!(p.source, Source::Inline("1 a * b\n2 c * d".into()));
    }

    /// A no-break space is text, not indentation (it used to split a character and panic).
    #[test]
    fn no_break_space_in_indented_text() {
        let book = Book::parse("text:\n \u{a0}x\n  y\n").unwrap();
        let Piece::Text { text, .. } = &book.pieces[0] else { panic!() };
        assert_eq!(text, "x  y");
    }

    #[test]
    fn errors_name_the_line() {
        let err = Book::parse("title: A\npage: a4\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(Book::parse("psalm: x.txt").unwrap_err().message.contains("tone"));
        assert!(Book::parse("bogus: 1").unwrap_err().message.contains("unknown"));
        assert!(Book::parse("score: a.gabc\n    (c4)").unwrap_err().message.contains("not both"));
        assert!(Book::parse("  indented").is_err());
        assert!(Book::parse("margins: 1 2 3 4 5").is_err());
        assert!(Book::parse("page: 0 0").unwrap_err().message.contains("more than 0"));
        assert!(Book::parse("page: 3e38in 10in").is_err());
        assert!(Book::parse("page: 300in 10in").unwrap_err().message.contains("200in"));
        let err = Book::parse("page: a6\nmargins: 60mm").unwrap_err();
        assert!(err.message.contains("margins leave"), "{err}");
    }

    #[test]
    fn origins_place_inline_text_in_the_book() {
        let src = "page: a5\ntitle: A\nscore: a.gabc\nscore:\n    name: x;\n\n# a comment\n      %%\n    (c4) A(g\nbreak\n";
        let book = Book::parse(src).unwrap();
        let origins: Vec<&Origin> = (0..book.pieces.len()).map(|i| book.origin(i).unwrap()).collect();
        assert!(book.origin(book.pieces.len()).is_none());
        assert_eq!(origins.iter().map(|o| o.line()).collect::<Vec<_>>(), [2, 3, 4, 10]);
        assert!(origins[1].body.is_empty());
        let Piece::Score {
            source: Source::Inline(text),
            ..
        } = &book.pieces[2]
        else {
            panic!("{:?}", book.pieces[2]);
        };
        // The comment line isn't part of the text; the indentation is counted back in.
        let at = |needle: &str| origins[2].position(text, text.find(needle).unwrap());
        assert_eq!(at("name"), Some((5, 5)));
        assert_eq!(at("%%"), Some((8, 7)));
        assert_eq!(at("A(g"), Some((9, 10)));
        assert_eq!(origins[1].position("", 0), None);
    }

    #[test]
    fn a_byte_order_mark_is_skipped() {
        let src = "title: A\nscore:\n    (c4) A(g)\n";
        assert_eq!(Book::parse(&format!("\u{feff}{src}")).unwrap(), Book::parse(src).unwrap());
    }
}
