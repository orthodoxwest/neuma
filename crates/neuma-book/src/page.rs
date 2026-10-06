//! A laid-out page: positioned neume glyphs, rectangles and shaped text, in points from the
//! page's top-left corner, y down.

use crate::font::Shaped;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    Black,
    Red,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// A neuma glyph ([`neuma::glyph_outline`]) with its anchor at (x, y), scaled by `scale`
    /// points per glyph unit.
    Neume {
        glyph: u16,
        x: f32,
        y: f32,
        scale: f32,
        color: Color,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
    /// Shaped text starting at x on `baseline`, `size` points.
    Text {
        x: f32,
        baseline: f32,
        size: f32,
        run: Shaped,
        color: Color,
    },
}

impl Op {
    pub fn shift(&mut self, dx: f32, dy: f32) {
        match self {
            Op::Neume { x, y, .. } | Op::Rect { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Op::Text { x, baseline, .. } => {
                *x += dx;
                *baseline += dy;
            }
        }
    }

    /// The op's horizontal extent, roughly, for overflow checks.
    pub fn right(&self) -> f32 {
        match self {
            Op::Neume { glyph, x, scale, .. } => {
                // The outline's width is in staff spaces of 100 glyph units.
                x + neuma::glyph_outline(*glyph).map_or(0.0, |o| o.width * 100.0 * scale)
            }
            Op::Rect { x, w, .. } => x + w,
            Op::Text { x, size, run, .. } => x + run.width * size,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub ops: Vec<Op>,
}
