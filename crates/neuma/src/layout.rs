//! Line breaking and justification: the only width-dependent stage.
//!
//! Segments are packed left to right with minimum gaps (notation to notation, lyric to lyric),
//! and an optimal-fit breaker picks the breaks with the least total demerits. Arithmetic is
//! limited to add, subtract, multiply, divide and comparison (DESIGN section 13).

use crate::engrave::neume::INTRA;
use crate::engrave::{Break, Engraving, SYLLABLE_GAP, Segment, WORD_GAP, clef_pieces, custos_piece};
use crate::score::{Clef, CustosRule};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LastLine {
    #[default]
    Ragged,
    Justified,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutOptions {
    /// Output units (px or pt) per staff space.
    pub scale: f32,
    pub last_line: LastLine,
}

impl Default for LayoutOptions {
    fn default() -> LayoutOptions {
        LayoutOptions {
            scale: 6.0,
            last_line: LastLine::Ragged,
        }
    }
}

/// Gap after the line-start clef.
const CLEF_GAP: f32 = INTRA * 2.0;
/// Gap before an end-of-line custos.
const CUSTOS_GAP: f32 = INTRA;
/// Room either side of a hyphen between syllables.
const HYPHEN_PAD: f32 = 0.25;
/// How far each gap may stretch before a line counts as loose, in staff spaces.
const STRETCH: f32 = 1.5;
/// The widest column laid out, in output units and in staff spaces; wider requests are
/// clamped to it.
const MAX_WIDTH: f32 = 1.0e6;
/// Space between stacked lines, in staff spaces.
const LINE_GAP: f32 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacedLine {
    pub first: usize,
    pub last: usize,
    /// x of each segment origin, in staff spaces from the line's left edge.
    pub xs: Vec<f32>,
    pub clef: Option<Clef>,
    pub custos: Option<(i8, f32)>,
    /// x of a hyphen after the last syllable, when its word continues on the next line.
    pub hyphen: Option<f32>,
    /// Hyphens between syllables of a word on this line: (x of the hyphen's center).
    pub hyphens: Vec<f32>,
    /// Top of the line box and the staff's center, in staff spaces from the layout's top.
    pub top: f32,
    pub staff: f32,
    pub baseline: f32,
    pub bottom: f32,
}

/// A layout at one width.
#[derive(Clone, Debug)]
pub struct Layout<'e> {
    pub(crate) eng: &'e Engraving,
    pub(crate) lines: Vec<PlacedLine>,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) scale: f32,
}

impl Layout<'_> {
    /// Width and height in output units.
    pub fn size(&self) -> (f32, f32) {
        (self.width * self.scale, self.height * self.scale)
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// Packing state after placing a segment.
#[derive(Clone, Copy)]
struct Cursor {
    ink_right: Option<f32>,
    lyric_right: Option<f32>,
    word_continues: bool,
    x: f32,
}

fn place(cur: &Cursor, seg: &Segment, hyphen: f32, word_space: f32, line_start: f32) -> f32 {
    let mut x = cur.x;
    match (cur.ink_right, seg.ink) {
        (Some(r), Some((l, _))) => {
            let gap = if seg.first {
                SYLLABLE_GAP + if seg.word_start { WORD_GAP } else { 0.0 }
            } else {
                seg.space_before
            };
            x = x.max(r + gap - l);
        }
        (None, Some((l, _))) => x = x.max(line_start - l),
        _ => {}
    }
    if let Some(t) = &seg.lyric {
        let need = match cur.lyric_right {
            Some(r) if cur.word_continues => r + hyphen + 2.0 * HYPHEN_PAD,
            Some(r) => r + word_space,
            None => 0.0,
        };
        x = x.max(need - t.left);
    }
    x
}

fn advance(cur: &Cursor, seg: &Segment, x: f32) -> Cursor {
    let mut next = *cur;
    next.x = x;
    if let Some((_, r)) = seg.ink {
        next.ink_right = Some(x + r);
    }
    if let Some(t) = &seg.lyric {
        next.lyric_right = Some(x + t.left + t.width);
        next.word_continues = !t.word_end;
    }
    next
}

struct Trial {
    xs: Vec<f32>,
    natural: f32,
    /// Ink right and lyric right ends, for justification and the custos.
    ink_end: f32,
}

impl Engraving {
    fn clef_before(&self, first: usize) -> Clef {
        if first == 0 {
            self.clef.clone()
        } else {
            self.segments[first - 1].clef.clone()
        }
    }

    fn line_start(&self, first: usize) -> (Option<Clef>, f32) {
        if self.segments[first].starts_with_clef {
            return (None, 0.0);
        }
        let clef = self.clef_before(first);
        let (_, right) = clef_pieces(&clef, 0.0);
        (Some(clef), right + CLEF_GAP)
    }

    fn custos_for(&self, last: usize) -> Option<i8> {
        let seg = &self.segments[last];
        let rule = match seg.after {
            Break::Forced { custos, .. } => custos,
            _ => CustosRule::Default,
        };
        if seg.suppress_custos || rule == CustosRule::Suppress {
            return None;
        }
        if self.custos_never {
            return None;
        }
        self.segments[last + 1..].iter().find_map(|s| s.first_note)
    }

    fn trial(&self, first: usize, last: usize, start: f32) -> Trial {
        let mut cur = Cursor {
            ink_right: None,
            lyric_right: None,
            word_continues: false,
            x: start,
        };
        let mut xs = Vec::with_capacity(last - first + 1);
        let mut right = 0.0f32;
        let mut ink_end = start;
        for seg in &self.segments[first..=last] {
            let x = place(&cur, seg, self.hyphen, self.word_space, start);
            xs.push(x);
            cur = advance(&cur, seg, x);
            right = right.max(x + seg.right());
            if let Some((_, r)) = seg.ink {
                ink_end = ink_end.max(x + r);
            }
        }
        Trial {
            xs,
            natural: self.natural(&cur, right, ink_end, last),
            ink_end,
        }
    }

    /// The width a line ending at `last` needs: its segments, plus a trailing hyphen and the
    /// custos.
    fn natural(&self, cur: &Cursor, mut right: f32, ink_end: f32, last: usize) -> f32 {
        if cur.word_continues
            && let Some(r) = cur.lyric_right
        {
            right = right.max(r + HYPHEN_PAD + self.hyphen);
        }
        if last + 1 < self.segments.len()
            && let Some(p) = self.custos_for(last)
        {
            let (_, w) = custos_piece(p, 0.0);
            right = right.max(ink_end + CUSTOS_GAP + w);
        }
        right
    }

    /// Lays the engraving out at `width` output units.
    pub fn layout(&self, width: f32, opts: &LayoutOptions) -> Layout<'_> {
        let scale = if opts.scale > 0.0 && opts.scale.is_finite() {
            opts.scale
        } else {
            1.0
        };
        // A non-finite or negative width can't be laid out; treat it as the narrowest or the
        // widest column so the output stays finite.
        let width = if width.is_nan() { 0.0 } else { width.clamp(0.0, MAX_WIDTH) };
        let target = (width / scale).min(MAX_WIDTH);
        let n = self.segments.len();
        if n == 0 {
            return Layout {
                eng: self,
                lines: Vec::new(),
                width: target,
                height: 0.0,
                scale,
            };
        }
        // best[k]: least demerits for lines ending just before segment k.
        let mut best: Vec<Option<(f32, usize)>> = vec![None; n + 1];
        best[0] = Some((0.0, 0));
        for first in 0..n {
            let Some((base, _)) = best[first] else { continue };
            let (_, start) = self.line_start(first);
            // Packs the line one segment at a time, as `trial` does, so each candidate costs
            // one step instead of a repack.
            let mut cur = Cursor {
                ink_right: None,
                lyric_right: None,
                word_continues: false,
                x: start,
            };
            let mut right = 0.0f32;
            let mut ink_end = start;
            for last in first..n {
                let seg = &self.segments[last];
                let end_of_score = last + 1 == n;
                let forced = matches!(seg.after, Break::Forced { .. });
                let breakable = end_of_score || forced || seg.after == Break::Allowed;
                let x = place(&cur, seg, self.hyphen, self.word_space, start);
                cur = advance(&cur, seg, x);
                right = right.max(x + seg.right());
                if let Some((_, r)) = seg.ink {
                    ink_end = ink_end.max(x + r);
                }
                let natural = self.natural(&cur, right, ink_end, last);
                let over = natural > target;
                if breakable {
                    let gaps = (last - first) as f32;
                    let ragged =
                        end_of_score && opts.last_line == LastLine::Ragged || matches!(seg.after, Break::Forced { justify: false, .. });
                    let badness = if over {
                        if last == first { 10000.0 } else { f32::INFINITY }
                    } else if ragged || forced {
                        // A written break fixes where this line ends, so how loose it is says
                        // nothing about the breaks before it.
                        0.0
                    } else if gaps == 0.0 {
                        if target - natural > 0.5 { 5000.0 } else { 0.0 }
                    } else {
                        let r = (target - natural) / (gaps * STRETCH);
                        (100.0 * r * r * r).min(10000.0)
                    };
                    if badness.is_finite() {
                        let d = (10.0 + badness) * (10.0 + badness);
                        let total = base + d;
                        let better = best[last + 1].is_none_or(|(b, _)| total < b);
                        if better {
                            best[last + 1] = Some((total, first));
                        }
                    }
                }
                if forced || over {
                    break;
                }
            }
        }
        // Walk back from the end.
        let mut ranges = Vec::new();
        let mut k = n;
        while k > 0 {
            let first = best[k].map_or(k - 1, |(_, f)| f);
            ranges.push((first, k - 1));
            k = first;
        }
        ranges.reverse();

        let size = self.lyric_size;
        let mut lines = Vec::new();
        let mut y = 0.0f32;
        let mut max_width = 0.0f32;
        for (li, &(first, last)) in ranges.iter().enumerate() {
            let (clef, start) = self.line_start(first);
            let trial = self.trial(first, last, start);
            let seg = &self.segments[last];
            let end_of_score = li + 1 == ranges.len();
            let ragged = end_of_score && opts.last_line == LastLine::Ragged || matches!(seg.after, Break::Forced { justify: false, .. });
            let mut xs = trial.xs.clone();
            let gaps = last - first;
            let mut stretch = 0.0;
            if !ragged && gaps > 0 && trial.natural < target {
                stretch = (target - trial.natural) / gaps as f32;
                for (i, x) in xs.iter_mut().enumerate() {
                    *x += stretch * i as f32;
                }
            }
            let ink_end = trial.ink_end + stretch * gaps as f32;
            let custos = if last + 1 < self.segments.len() {
                self.custos_for(last).map(|p| (p, ink_end + CUSTOS_GAP))
            } else {
                None
            };
            // Hyphens.
            let mut hyphens = Vec::new();
            let mut prev_lyric: Option<(f32, bool)> = None;
            for (i, s) in self.segments[first..=last].iter().enumerate() {
                if let Some(t) = &s.lyric {
                    let l = xs[i] + t.left;
                    if let Some((r, cont)) = prev_lyric
                        && cont
                    {
                        hyphens.push((r + l) / 2.0);
                    }
                    prev_lyric = Some((l + t.width, !t.word_end));
                }
            }
            let hyphen = match prev_lyric {
                Some((r, true)) if last + 1 < self.segments.len() => Some(r + HYPHEN_PAD + self.hyphen / 2.0),
                _ => None,
            };
            // Vertical extent.
            let mut ink_top = -3.0f32;
            let mut ink_bottom = 3.0f32;
            for s in &self.segments[first..=last] {
                for p in &s.pieces {
                    let (a, b) = p.y_extent();
                    ink_top = ink_top.min(a);
                    ink_bottom = ink_bottom.max(b);
                }
            }
            let has_lyrics = self.segments[first..=last].iter().any(|s| s.lyric.is_some());
            let top = y;
            let staff = top + (-ink_top) + 0.5;
            let baseline = staff + ink_bottom + 0.4 + if has_lyrics { self.ascent * size * 0.85 } else { 0.0 };
            let bottom = baseline + if has_lyrics { self.descent * size } else { 0.5 };
            let right = if ragged { trial.natural } else { target.max(trial.natural) };
            max_width = max_width.max(right);
            lines.push(PlacedLine {
                first,
                last,
                xs,
                clef,
                custos,
                hyphen,
                hyphens,
                top,
                staff,
                baseline,
                bottom,
            });
            y = bottom + LINE_GAP;
        }
        let height = lines.last().map_or(0.0, |l| l.bottom);
        Layout {
            eng: self,
            lines,
            width: target.max(max_width),
            height,
            scale,
        }
    }
}
