//! The renderer-neutral display list: glyphs, rectangles and text runs in output units.

use crate::engrave::{Ink, Mark, Piece, clef_pieces, custos_piece};
use crate::glyphs::UNITS_PER_SPACE;
use crate::layout::Layout;
use crate::score::{LyricRun, TextStyle};

/// The note a piece of ink belongs to, by score-wide note index.
pub type NoteRef = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRole {
    Lyric,
    Hyphen,
    Initial,
    Annotation,
    Rubric,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A glyph outline (see [`crate::glyphs::glyph_outline`]) drawn with its anchor at (x, y),
    /// scaled by `scale` (output units per glyph unit).
    Glyph {
        glyph: u16,
        x: f32,
        y: f32,
        scale: f32,
        role: Ink,
        note: Option<NoteRef>,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        role: Ink,
        note: Option<NoteRef>,
    },
    /// Text starting at x on `baseline`, `size` output units high.
    Text {
        x: f32,
        baseline: f32,
        size: f32,
        runs: Vec<TextRun>,
        role: TextRole,
        syllable: Option<u32>,
    },
}

/// One staff line box, in output units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineBox {
    pub top: f32,
    pub bottom: f32,
    /// y of the staff's middle (staff position 0).
    pub staff: f32,
    pub baseline: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayList {
    pub width: f32,
    pub height: f32,
    /// Distance between staff positions (one staff space), in output units.
    pub staff_space: f32,
    pub lines: Vec<LineBox>,
    pub items: Vec<Item>,
    pub alt_text: String,
}

fn push(items: &mut Vec<Item>, p: &Piece, dx: f32, dy: f32, s: f32) {
    match p.mark {
        Mark::Glyph { glyph, x, y } => items.push(Item::Glyph {
            glyph: glyph.id(),
            x: (x + dx) * s,
            y: (y + dy) * s,
            scale: s / UNITS_PER_SPACE,
            role: p.role,
            note: p.note,
        }),
        Mark::Rect { x, y, w, h } => items.push(Item::Rect {
            x: (x + dx) * s,
            y: (y + dy) * s,
            w: w * s,
            h: h * s,
            role: p.role,
            note: p.note,
        }),
    }
}

fn runs(r: &[LyricRun]) -> Vec<TextRun> {
    r.iter()
        .map(|r| TextRun {
            text: r.text.clone(),
            style: r.style,
        })
        .collect()
}

impl Layout<'_> {
    pub fn display(&self) -> DisplayList {
        let s = self.scale;
        let eng = self.eng;
        let size = eng.lyric_size * s;
        let mut items = Vec::new();
        let mut lines = Vec::new();
        let staff_weight = crate::engrave::neume::STEM;
        for line in &self.lines {
            lines.push(LineBox {
                top: line.top * s,
                bottom: line.bottom * s,
                staff: line.staff * s,
                baseline: line.baseline * s,
            });
            for k in [-3.0f32, -1.0, 1.0, 3.0] {
                let y = line.staff + k - staff_weight / 2.0;
                items.push(Item::Rect {
                    x: 0.0,
                    y: y * s,
                    w: self.width * s,
                    h: staff_weight * s,
                    role: Ink::Staff,
                    note: None,
                });
            }
            if let Some(clef) = &line.clef {
                let (pieces, _) = clef_pieces(clef, 0.0);
                for p in &pieces {
                    push(&mut items, p, 0.0, line.staff, s);
                }
            }
            for (i, seg) in eng.segments[line.first..=line.last].iter().enumerate() {
                let x = line.xs[i];
                for p in &seg.pieces {
                    push(&mut items, p, x, line.staff, s);
                }
                if let Some(t) = &seg.lyric {
                    let role = if t.runs.iter().all(|r| r.style.rubric) {
                        TextRole::Rubric
                    } else {
                        TextRole::Lyric
                    };
                    items.push(Item::Text {
                        x: (x + t.left) * s,
                        baseline: line.baseline * s,
                        size,
                        runs: runs(&t.runs),
                        role,
                        syllable: Some(t.syllable),
                    });
                }
            }
            let hyphen_left = |c: f32| (c - eng.hyphen / 2.0) * s;
            for &c in &line.hyphens {
                items.push(Item::Text {
                    x: hyphen_left(c),
                    baseline: line.baseline * s,
                    size,
                    runs: vec![TextRun {
                        text: "-".into(),
                        style: TextStyle::REGULAR,
                    }],
                    role: TextRole::Hyphen,
                    syllable: None,
                });
            }
            if let Some(c) = line.hyphen {
                items.push(Item::Text {
                    x: hyphen_left(c),
                    baseline: line.baseline * s,
                    size,
                    runs: vec![TextRun {
                        text: "-".into(),
                        style: TextStyle::REGULAR,
                    }],
                    role: TextRole::Hyphen,
                    syllable: None,
                });
            }
            if let Some((p, x)) = line.custos {
                let (piece, _) = custos_piece(p, x);
                push(&mut items, &piece, 0.0, line.staff, s);
            }
        }
        DisplayList {
            width: self.width * s,
            height: self.height * s,
            staff_space: s,
            lines,
            items,
            alt_text: eng.alt_text.clone(),
        }
    }
}
