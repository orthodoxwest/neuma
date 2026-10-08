//! The renderer-neutral display list: glyphs, rectangles and text runs in output units.

use crate::engrave::{Ink, Mark, Piece, clef_pieces, custos_piece};
use crate::glyphs::UNITS_PER_SPACE;
use crate::layout::{INITIAL_BEFORE, Layout, PlacedLine};
use crate::score::{LyricRun, TextStyle};

/// The note a piece of ink belongs to, by score-wide note index.
pub type NoteRef = u32;

/// What a run of text is, so themes can style lyrics, rubrics and the initial separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextRole {
    /// A syllable's text under the staff.
    Lyric,
    /// A hyphen between two syllables of a word, or after the last syllable on a line when
    /// the word goes on.
    Hyphen,
    /// The drop-cap initial.
    Initial,
    /// A line above the initial: an `annotation` header, or the mode.
    Annotation,
    /// A syllable's text set all in the rubric style (`<c>`), as directions are.
    Rubric,
}

/// Text in one style.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TextRun {
    /// The text, to draw with ligatures off (see [`crate::text::TextMeasure`]).
    pub text: String,
    /// Its style: italic, bold, small caps, colored, underlined.
    pub style: TextStyle,
}

/// One thing to draw, in output units (origin top left, y down).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Item {
    /// A glyph outline (see [`crate::glyphs::glyph_outline`]) drawn with its anchor at (x, y),
    /// scaled by `scale` (output units per glyph unit).
    Glyph {
        /// The glyph's stable id; [`crate::glyph_outline`] gives its outline.
        glyph: u16,
        /// Where the outline's origin goes, in output units.
        x: f32,
        /// Where the outline's origin goes, in output units.
        y: f32,
        /// Output units per glyph unit: the outline's coordinates times this are output units
        /// (100 glyph units are one staff space).
        scale: f32,
        /// What the ink is, for styling.
        role: Ink,
        /// The note it draws, if any.
        note: Option<NoteRef>,
        /// For ink that draws several notes (a porrectus swash), the last of them; the ink
        /// belongs to every note from `note` through this one.
        through: Option<NoteRef>,
    },
    /// A filled rectangle: a staff or ledger line, a stem, a bar, an episema.
    Rect {
        /// Left edge, in output units.
        x: f32,
        /// Top edge, in output units.
        y: f32,
        /// Width, in output units.
        w: f32,
        /// Height, in output units.
        h: f32,
        /// What the ink is, for styling.
        role: Ink,
        /// The note it belongs to, if any (a stem, an episema or a ledger line of a note).
        note: Option<NoteRef>,
        /// For ink that draws several notes (a porrectus swash), the last of them; the ink
        /// belongs to every note from `note` through this one.
        through: Option<NoteRef>,
    },
    /// Text starting at x on `baseline`, `size` output units high.
    Text {
        /// Where the text starts, in output units.
        x: f32,
        /// The baseline's y, in output units.
        baseline: f32,
        /// The font size (one em), in output units.
        size: f32,
        /// The text, in runs of one style each, drawn one after another from `x`.
        runs: Vec<TextRun>,
        /// What the text is, for styling.
        role: TextRole,
        /// The index of the score's syllable it belongs to, for a lyric, a rubric or the
        /// initial.
        syllable: Option<u32>,
    },
}

/// One staff line's box, in output units.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct LineBox {
    /// y of the top of the line's box, half a staff space or more above its highest ink.
    pub top: f32,
    /// y of the bottom of the line's box: below its lyrics' descenders and its lowest ink.
    pub bottom: f32,
    /// y of the staff's middle (staff position 0).
    pub staff: f32,
    /// y of the line's lyric baseline.
    pub baseline: f32,
}

/// A layout as items to draw, for native canvases. Build it with [`Layout::display`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DisplayList {
    /// The layout's width, in output units: the width asked for, or wider if a line can't
    /// fit in it.
    pub width: f32,
    /// The layout's height, in output units.
    pub height: f32,
    /// Distance between staff positions (one staff space), in output units.
    pub staff_space: f32,
    /// Each line's box, top to bottom.
    pub lines: Vec<LineBox>,
    /// What to draw, in drawing order.
    pub items: Vec<Item>,
    /// The lyrics as plain text, for an accessibility label.
    pub alt_text: String,
}

/// Pushes `p`, whose note numbers count from `base`, moved by (`dx`, `dy`) and scaled by `s`.
fn push(items: &mut Vec<Item>, p: &Piece, dx: f32, dy: f32, s: f32, base: u32) {
    let note = p.note.map(|n| n + base);
    let through = p.through.map(|n| n + base);
    match p.mark {
        Mark::Glyph { glyph, x, y } => items.push(Item::Glyph {
            glyph: glyph.id(),
            x: (x + dx) * s,
            y: (y + dy) * s,
            scale: s / UNITS_PER_SPACE,
            role: p.role,
            note,
            through,
        }),
        Mark::Rect { x, y, w, h } => items.push(Item::Rect {
            x: (x + dx) * s,
            y: (y + dy) * s,
            w: w * s,
            h: h * s,
            role: p.role,
            note,
            through,
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

impl Layout {
    /// This layout as a display list.
    #[must_use]
    pub fn display(&self) -> DisplayList {
        let s = self.scale;
        let eng = &*self.eng;
        let mut items = Vec::new();
        let mut lines = Vec::new();
        for line in &self.lines {
            lines.push(LineBox {
                top: line.top * s,
                bottom: line.bottom * s,
                staff: line.staff * s,
                baseline: line.baseline * s,
            });
            self.push_line(&mut items, line, 0.0);
        }
        self.push_initial(&mut items);
        DisplayList {
            width: self.width * s,
            height: self.height * s,
            staff_space: s,
            lines,
            items,
            alt_text: eng.alt_text.clone(),
        }
    }

    /// Appends the initial and its annotations, which hang beside the first lines.
    pub(crate) fn push_initial(&self, items: &mut Vec<Item>) {
        let s = self.scale;
        let eng = &*self.eng;
        if let (Some(init), Some(placed), Some(_)) = (&eng.initial, &self.initial, self.lines.first()) {
            let column = placed.column;
            items.push(Item::Text {
                x: (placed.x + init.lead_em * placed.size) * s,
                baseline: placed.baseline * s,
                size: placed.size * s,
                runs: vec![TextRun {
                    text: init.text.clone(),
                    style: TextStyle::REGULAR,
                }],
                role: TextRole::Initial,
                syllable: Some(init.syllable),
            });
            let count = init.annotations.len();
            for (i, (text, w)) in init.annotations.iter().enumerate() {
                let above = (count - 1 - i) as f32 * init.annotation_size * 1.1;
                items.push(Item::Text {
                    x: (INITIAL_BEFORE + (column - w) / 2.0) * s,
                    baseline: (placed.annotation_baseline - above) * s,
                    size: init.annotation_size * s,
                    runs: vec![TextRun {
                        text: text.clone(),
                        style: TextStyle::REGULAR,
                    }],
                    role: TextRole::Annotation,
                    syllable: None,
                });
            }
        }
    }

    /// Appends the items of one line, `top` higher up than the layout places it.
    pub(crate) fn push_line(&self, items: &mut Vec<Item>, line: &PlacedLine, top: f32) {
        let s = self.scale;
        let eng = &*self.eng;
        let size = eng.lyric_size * s;
        let staff_weight = crate::engrave::neume::STEM;
        let staff = line.staff - top;
        let baseline = line.baseline - top;
        let staff_lines: &[f32] = if line.staffless { &[] } else { &[-3.0, -1.0, 1.0, 3.0] };
        for &k in staff_lines {
            let y = staff + k - staff_weight / 2.0;
            items.push(Item::Rect {
                x: line.indent * s,
                y: y * s,
                w: (self.width - line.indent) * s,
                h: staff_weight * s,
                role: Ink::Staff,
                note: None,
                through: None,
            });
        }
        if let Some(clef) = &line.clef {
            let (pieces, _) = clef_pieces(clef, 0.0);
            for p in &pieces {
                push(items, p, line.indent, staff, s, 0);
            }
        }
        for (i, seg) in eng.segments[line.first..=line.last].iter().enumerate() {
            let x = line.xs[i];
            for p in &seg.pieces {
                push(items, p, x, staff, s, seg.note_base);
            }
            if let Some(t) = &seg.lyric
                && t.lead_hyphen
            {
                items.push(Item::Text {
                    x: (x + t.left) * s,
                    baseline: baseline * s,
                    size,
                    runs: runs(&t.runs),
                    role: TextRole::Hyphen,
                    syllable: None,
                });
            } else if let Some(t) = &seg.lyric {
                let role = if t.runs.iter().all(|r| r.style.rubric) {
                    TextRole::Rubric
                } else {
                    TextRole::Lyric
                };
                items.push(Item::Text {
                    x: (x + t.left) * s,
                    baseline: baseline * s,
                    size,
                    runs: runs(&t.runs),
                    role,
                    syllable: Some(seg.syllable),
                });
            }
        }
        let hyphen_left = |c: f32| (c - eng.hyphen / 2.0) * s;
        for &c in &line.hyphens {
            items.push(Item::Text {
                x: hyphen_left(c),
                baseline: baseline * s,
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
                baseline: baseline * s,
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
            push(items, &piece, 0.0, staff, s, 0);
        }
        for &(y, l, r) in &line.bridges {
            items.push(Item::Rect {
                x: l * s,
                y: (y + staff) * s,
                w: (r - l) * s,
                h: staff_weight * s,
                role: Ink::Ledger,
                note: None,
                through: None,
            });
        }
    }
}
