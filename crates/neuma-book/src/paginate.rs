//! Puts blocks on pages, honouring keep-with-next chains and page breaks, and adds running
//! headers and page numbers.

use crate::book::{PageNumbers, Settings};
use crate::compose::{Block, Kind};
use crate::font::Fonts;
use crate::page::{Color, Op, Page};

/// What one page holds before its furniture is drawn.
#[derive(Clone, Debug, Default)]
struct Sheet {
    ops: Vec<Op>,
    /// The running header in force at the top of the page.
    header: Option<String>,
    title: bool,
    used: f32,
}

/// Groups blocks into runs that must share a page: each block with every block it keeps
/// with.
fn chains(blocks: &[Block]) -> Vec<&[Block]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in blocks.iter().enumerate() {
        let hard = matches!(b.kind, Kind::Break | Kind::Header(_));
        if hard {
            if start < i {
                out.push(&blocks[start..i]);
            }
            out.push(&blocks[i..=i]);
            start = i + 1;
        } else if !b.keep_with_next {
            out.push(&blocks[start..=i]);
            start = i + 1;
        }
    }
    if start < blocks.len() {
        out.push(&blocks[start..]);
    }
    out
}

/// Lays blocks out on pages of the book's size.
pub fn paginate(blocks: &[Block], s: &Settings, fonts: &Fonts) -> Vec<Page> {
    let room = (s.height - s.margins[0] - s.margins[2]).max(1.0);
    let mut sheets: Vec<Sheet> = vec![Sheet::default()];
    let mut header: Option<String> = None;
    let new_page = |sheets: &mut Vec<Sheet>, header: &Option<String>| {
        sheets.push(Sheet {
            header: header.clone(),
            ..Sheet::default()
        });
    };
    for chain in chains(blocks) {
        let first = &chain[0];
        match &first.kind {
            Kind::Break => {
                if sheets.last().is_some_and(|p| !p.ops.is_empty()) {
                    new_page(&mut sheets, &header);
                }
                continue;
            }
            Kind::Header(h) => {
                header = Some(h.clone());
                if let Some(p) = sheets.last_mut()
                    && p.ops.is_empty()
                {
                    p.header = header.clone();
                }
                continue;
            }
            _ => {}
        }
        let used = sheets.last().map_or(0.0, |p| p.used);
        let height: f32 = chain.iter().map(|b| b.height + b.space_before).sum::<f32>() - if used == 0.0 { first.space_before } else { 0.0 };
        if used > 0.0 && used + height > room && height - first.space_before <= room {
            new_page(&mut sheets, &header);
        }
        for b in chain {
            let page = sheets.last_mut().expect("there is always a page");
            let mut gap = if page.used == 0.0 { 0.0 } else { b.space_before };
            // A chain taller than a page breaks where it must.
            if page.used > 0.0 && page.used + gap + b.height > room + 0.01 {
                new_page(&mut sheets, &header);
                gap = 0.0;
            }
            let page = sheets.last_mut().expect("there is always a page");
            if b.kind == Kind::Title {
                page.title = true;
            }
            let y = page.used + gap;
            for op in &b.ops {
                let mut op = op.clone();
                op.shift(0.0, y);
                page.ops.push(op);
            }
            page.used = y + b.height;
        }
    }
    if sheets.len() > 1 && sheets.last().is_some_and(|p| p.ops.is_empty()) {
        sheets.pop();
    }
    sheets
        .into_iter()
        .enumerate()
        .map(|(i, sheet)| furnish(sheet, i + 1, s, fonts))
        .collect()
}

/// Moves a page's content into its margins, and draws the running header and page number.
fn furnish(sheet: Sheet, number: usize, s: &Settings, fonts: &Fonts) -> Page {
    let even = number.is_multiple_of(2);
    let (left, right) = if s.mirror && even {
        (s.margins[1], s.margins[3])
    } else {
        (s.margins[3], s.margins[1])
    };
    let mut ops = sheet.ops;
    for op in &mut ops {
        op.shift(left, s.margins[0]);
    }
    let size = s.text_size * 0.85;
    let top = s.margins[0] * 0.55;
    let text = |t: &str, face: usize| fonts.shape(t, face, false);
    let mut put = |t: &str, face: usize, align: f32, x: f32, baseline: f32, color: Color| {
        let run = text(t, face);
        let w = run.width * size;
        ops.push(Op::Text {
            x: x - w * align,
            baseline,
            size,
            run,
            color,
        });
    };
    let mid = left + (s.width - left - right) / 2.0;
    if let Some(h) = sheet.header.as_deref().filter(|_| !sheet.title) {
        put(h, fonts.resolve(true, false), 0.5, mid, top, Color::Red);
    }
    let n = number.to_string();
    match s.page_numbers {
        PageNumbers::Bottom => put(
            &n,
            fonts.resolve(false, false),
            0.5,
            mid,
            s.height - s.margins[2] * 0.45,
            Color::Black,
        ),
        PageNumbers::Outer if !sheet.title => {
            if even {
                put(&n, fonts.resolve(false, false), 0.0, left, top, Color::Black);
            } else {
                put(&n, fonts.resolve(false, false), 1.0, s.width - right, top, Color::Black);
            }
        }
        _ => {}
    }
    Page {
        width: s.width,
        height: s.height,
        ops,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(h: f32, keep: bool) -> Block {
        Block {
            height: h,
            space_before: 0.0,
            ops: vec![Op::Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h,
                color: Color::Black,
            }],
            keep_with_next: keep,
            kind: Kind::Content,
        }
    }

    fn settings() -> Settings {
        Settings {
            width: 200.0,
            height: 140.0,
            margins: [20.0, 20.0, 20.0, 20.0],
            page_numbers: PageNumbers::None,
            ..Settings::default()
        }
    }

    /// The y of each page's rects, by page.
    fn tops(pages: &[Page]) -> Vec<Vec<f32>> {
        pages
            .iter()
            .map(|p| {
                p.ops
                    .iter()
                    .filter_map(|o| match o {
                        Op::Rect { y, .. } => Some(*y),
                        _ => None,
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn kept_blocks_move_together() {
        // 100pt of room: 40 + 40 fit, then a chain of 30 + 30 moves to the next page.
        let f = Fonts::standard();
        let blocks = vec![block(40.0, false), block(40.0, false), block(30.0, true), block(30.0, false)];
        let pages = paginate(&blocks, &settings(), &f);
        assert_eq!(tops(&pages), vec![vec![20.0, 60.0], vec![20.0, 50.0]]);
    }

    #[test]
    fn a_chain_taller_than_a_page_breaks() {
        let f = Fonts::standard();
        let blocks = vec![block(60.0, true), block(60.0, true), block(60.0, false)];
        let pages = paginate(&blocks, &settings(), &f);
        assert_eq!(pages.len(), 3);
    }

    #[test]
    fn breaks_and_headers() {
        let f = Fonts::standard();
        let mut s = settings();
        s.page_numbers = PageNumbers::Bottom;
        let marker = |kind| Block {
            height: 0.0,
            space_before: 0.0,
            ops: Vec::new(),
            keep_with_next: false,
            kind,
        };
        let blocks = vec![
            marker(Kind::Header("Compline".into())),
            block(10.0, false),
            marker(Kind::Break),
            marker(Kind::Break),
            block(10.0, false),
        ];
        let pages = paginate(&blocks, &s, &f);
        // Two breaks in a row make one new page.
        assert_eq!(pages.len(), 2);
        let texts = |p: &Page| -> Vec<String> {
            p.ops
                .iter()
                .filter_map(|o| match o {
                    Op::Text { run, .. } => Some(run.glyphs.iter().map(|g| g.text.as_str()).collect()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(texts(&pages[0]), vec!["Compline", "1"]);
        assert_eq!(texts(&pages[1]), vec!["Compline", "2"]);
    }
}
