//! The example booklet, end to end: parse, typeset, paginate, and write PDF and SVG.

use std::path::PathBuf;

use neuma_book::{Book, Document, FontFiles, Fonts, Op, Page, typeset};

fn example() -> Book {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/compline");
    let src = std::fs::read_to_string(dir.join("compline.book")).unwrap();
    let mut book = Book::parse(&src).unwrap();
    book.resolve(&dir).unwrap();
    book
}

/// The text a page draws, run by run.
fn texts(p: &Page) -> Vec<String> {
    p.ops
        .iter()
        .filter_map(|o| match o {
            Op::Text { run, .. } => Some(run.glyphs.iter().map(|g| g.text.as_str()).collect()),
            _ => None,
        })
        .collect()
}

fn page_with(doc: &Document, word: &str) -> usize {
    doc.pages
        .iter()
        .position(|p| texts(p).iter().any(|t| t == word))
        .unwrap_or_else(|| panic!("no page has `{word}`"))
}

fn check(doc: &Document, book: &Book) {
    let s = &book.settings;
    assert!(doc.pages.len() >= 4, "{} pages", doc.pages.len());
    for (i, p) in doc.pages.iter().enumerate() {
        for op in &p.ops {
            let (x, y) = match op {
                Op::Neume { x, y, .. } | Op::Rect { x, y, .. } => (*x, *y),
                Op::Text { x, baseline, .. } => (*x, *baseline),
            };
            assert!(x >= s.margins[3] - 0.5, "page {}: {op:?} starts in the left margin", i + 1);
            assert!(
                op.right() <= s.width - s.margins[1] + 0.5,
                "page {}: {op:?} runs into the right margin",
                i + 1
            );
            assert!(y > 0.0 && y < s.height, "page {}: {op:?} is off the page", i + 1);
        }
    }
    // The antiphon, the tone and the psalm's first verse share a page.
    let antiphon = page_with(doc, "AVE");
    assert_eq!(page_with(doc, "Tone 8.G"), antiphon);
    assert_eq!(page_with(doc, "Hear"), antiphon);
    // The title page has no running header; later pages do.
    assert_eq!(texts(&doc.pages[0]).iter().filter(|t| *t == "Compline").count(), 1);
    assert!(texts(&doc.pages[1]).iter().any(|t| t == "Compline"));
    // Page numbers.
    assert!(texts(&doc.pages[1]).iter().any(|t| t == "2"));
    let pdf = doc.pdf_for_test();
    check_pdf(&pdf, doc.pages.len());
    for i in 0..doc.pages.len() {
        let svg = doc.svg(i, &Fonts::standard()).unwrap();
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    }
}

/// The cross-reference table points at each object, and the page count is right. Works on
/// bytes: embedded fonts aren't text.
fn check_pdf(pdf: &[u8], pages: usize) {
    assert!(pdf.starts_with(b"%PDF-1.7"));
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).rposition(|w| w == needle);
    let sx = find(pdf, b"startxref\n").unwrap() + 10;
    let tail = std::str::from_utf8(&pdf[sx..]).unwrap();
    let start: usize = tail.lines().next().unwrap().parse().unwrap();
    let xref = std::str::from_utf8(&pdf[start..]).unwrap();
    assert!(xref.starts_with("xref"));
    let mut n = 0;
    for (i, line) in xref.lines().skip(3).take_while(|l| l.ends_with(" n ")).enumerate() {
        let off: usize = line[..10].parse().unwrap();
        assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "object {}", i + 1);
        n += 1;
    }
    assert!(n > pages);
    assert!(find(pdf, format!("/Count {pages} ").as_bytes()).is_some());
}

trait PdfForTest {
    fn pdf_for_test(&self) -> Vec<u8>;
}

impl PdfForTest for Document {
    fn pdf_for_test(&self) -> Vec<u8> {
        self.pdf(&Fonts::standard())
    }
}

#[test]
fn example_with_standard_fonts() {
    let book = example();
    let fonts = Fonts::standard();
    let doc = typeset(&book, &fonts);
    check(&doc, &book);
    let pdf = String::from_utf8_lossy(&doc.pdf(&fonts)).into_owned();
    assert!(pdf.contains("/BaseFont /Times-Roman"));
    // The rubric colour.
    assert!(pdf.contains("0.639 0.129 0.11 rg"));
    assert!(
        doc.problems.iter().all(|p| p.diagnostic.severity != neuma::Severity::Error),
        "{:?}",
        doc.problems
    );
}

#[test]
fn example_with_eb_garamond_when_installed() {
    let Some(files) = FontFiles::find_default() else { return };
    let book = example();
    let fonts = Fonts::new(&files);
    let mut doc = typeset(&book, &fonts);
    let has = |pdf: &[u8], s: &str| pdf.windows(s.len()).any(|w| w == s.as_bytes());
    let pdf = doc.pdf(&fonts);
    check_pdf(&pdf, doc.pages.len());
    assert!(has(&pdf, "/FontFile2") || has(&pdf, "/FontFile3"));
    assert!(has(&pdf, "/ToUnicode"));
    doc.set_text_as_paths(true);
    let pdf = doc.pdf(&fonts);
    check_pdf(&pdf, doc.pages.len());
    assert!(!has(&pdf, "/FontFile"));
    let svg = doc.svg(0, &fonts).unwrap();
    assert!(!svg.contains("<text"));
}
