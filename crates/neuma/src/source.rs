//! Links between the GABC source and the engraved score, both ways, for editors: which source
//! bytes drew the note, bar or syllable under a point, and what to highlight for a caret.
//!
//! Spans are UTF-8 byte ranges in the source, as everywhere in neuma. Editors that count in
//! UTF-16 code units (JavaScript, Java and Kotlin strings, `NSString`) convert with
//! [`Utf16Index`].

use std::ops::Range;

use crate::display::LineBox;
use crate::layout::Layout;

/// What kind of thing an [`Element`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ElementKind {
    Note,
    Bar,
    Syllable,
}

/// A note, bar or syllable as drawn: its source span and its box on one line.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Element {
    pub kind: ElementKind,
    /// The note id ([`crate::NoteRef`]), or the bar's or syllable's index, counted from 0
    /// in source order.
    pub index: u32,
    /// UTF-8 byte range in the source. A syllable's runs from its text through its closing
    /// parenthesis.
    pub span: Range<usize>,
    pub line: u32,
    /// The box's left, top, width and height, in output units. A note's is its notehead,
    /// less a porrectus end's overlap with its neighbours; a bar's its ink; and a syllable's
    /// runs from the line's top to its bottom across the syllable's notation and lyric (or
    /// across the initial, for the syllable it starts).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The x of a note's notehead center, as [`crate::TimelineNote::cx`] (the box can be
    /// trimmed off center); for a bar or syllable, the box's center. A tap between notes
    /// finds the note whose `cx` is nearest.
    pub cx: f32,
}

impl Element {
    fn contains(&self, x: f32, y: f32, pad: f32) -> bool {
        x >= self.x - pad && x <= self.x + self.w + pad && y >= self.y && y <= self.y + self.h
    }

    /// Horizontal distance from `x` to the box, 0 inside it.
    fn distance(&self, x: f32) -> f32 {
        (self.x - x).max(x - (self.x + self.w)).max(0.0)
    }
}

/// Every note, bar and syllable of a layout with its source span: what [`Layout::note_at`],
/// [`Layout::source_at`] and [`Layout::elements_at`] search. Get it with
/// [`Layout::source_map`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SourceMap {
    /// By note id; notes on lines a `max_lines` layout leaves out are missing.
    pub notes: Vec<Element>,
    /// By bar index.
    pub bars: Vec<Element>,
    /// In source order. A syllable split across lines has an element per line, and the
    /// syllable the initial comes from has one more for the initial.
    pub syllables: Vec<Element>,
    /// Each line's box, staff center and lyric baseline, in output units.
    pub lines: Vec<LineBox>,
    /// One staff space in output units.
    pub staff_space: f32,
}

impl SourceMap {
    /// The note under (`x`, `y`) in output units: one whose box holds the point, else the
    /// note on the line the point falls in whose [`cx`](Element::cx) is nearest. `None`
    /// outside every line.
    #[must_use]
    pub fn note_at(&self, x: f32, y: f32) -> Option<crate::NoteRef> {
        if let Some(n) = self.notes.iter().find(|n| n.contains(x, y, 0.0)) {
            return Some(n.index);
        }
        let line = self.line_at(y)?;
        self.notes
            .iter()
            .filter(|n| n.line == line)
            .min_by(|a, b| (a.cx - x).abs().total_cmp(&(b.cx - x).abs()))
            .map(|n| n.index)
    }

    /// The element under (`x`, `y`) in output units, most specific first: a notehead whose
    /// box holds the point, else a bar within half a staff space of it, else a syllable whose
    /// box holds it, else the nearest syllable on the line the point falls in. `None` outside
    /// every line.
    #[must_use]
    pub fn source_at(&self, x: f32, y: f32) -> Option<&Element> {
        if let Some(n) = self.notes.iter().find(|n| n.contains(x, y, 0.0)) {
            return Some(n);
        }
        let pad = self.staff_space / 2.0;
        if let Some(b) = self.bars.iter().find(|b| b.contains(x, y, pad)) {
            return Some(b);
        }
        let line = self.line_at(y)?;
        // Lyrics can overhang their neighbours' boxes; the nearest center wins.
        let center = |e: &&Element| (e.x + e.w / 2.0 - x).abs();
        let on_line = || self.syllables.iter().filter(|s| s.line == line);
        if let Some(s) = on_line()
            .filter(|s| s.contains(x, y, 0.0))
            .min_by(|a, b| center(a).total_cmp(&center(b)))
        {
            return Some(s);
        }
        on_line().min_by(|a, b| a.distance(x).total_cmp(&b.distance(x)).then(center(a).total_cmp(&center(b))))
    }

    fn line_at(&self, y: f32) -> Option<u32> {
        Some(self.lines.iter().position(|l| y >= l.top && y <= l.bottom)? as u32)
    }

    /// What to highlight for a caret at byte `offset`: the note and the bar whose span holds
    /// it, then the boxes of the syllable it is in, most specific first. A caret just after
    /// a note, bar or syllable, as after typing it, counts as in it when nothing of that
    /// kind holds the caret. Notes written together (`gvv` is three) share a span, and all
    /// of them are returned.
    #[must_use]
    pub fn elements_at(&self, offset: usize) -> Vec<&Element> {
        let mut out = Vec::new();
        for list in [&self.notes, &self.bars] {
            let hit = list
                .iter()
                .find(|e| e.span.start <= offset && offset < e.span.end)
                .or_else(|| list.iter().rfind(|e| !e.span.is_empty() && e.span.end == offset));
            if let Some(hit) = hit {
                out.extend(list.iter().filter(|e| e.span == hit.span));
            }
        }
        let holds = |e: &&Element| e.span.start <= offset && offset < e.span.end;
        let index = self
            .syllables
            .iter()
            .find(holds)
            .or_else(|| self.syllables.iter().rfind(|e| !e.span.is_empty() && e.span.end == offset))
            .map(|e| e.index);
        if let Some(i) = index {
            out.extend(self.syllables.iter().filter(|e| e.index == i));
        }
        out
    }
}

impl Layout {
    /// Every note, bar and syllable on this layout's lines with its source span and box, made
    /// when first asked for and kept (clones of the layout share it).
    #[must_use]
    pub fn source_map(&self) -> &SourceMap {
        self.sources.get_or_init(|| self.make_source_map())
    }

    /// The note under (`x`, `y`): one whose box holds the point, else the nearest on the line
    /// the point falls in. `None` outside every line. See [`SourceMap::note_at`].
    #[must_use]
    pub fn note_at(&self, x: f32, y: f32) -> Option<crate::NoteRef> {
        self.source_map().note_at(x, y)
    }

    /// The note, bar or syllable under (`x`, `y`), with its source span: a notehead, else a
    /// bar within half a staff space, else a syllable's box, else the nearest syllable on
    /// that line. `None` outside every line. See [`SourceMap::source_at`].
    #[must_use]
    pub fn source_at(&self, x: f32, y: f32) -> Option<&Element> {
        self.source_map().source_at(x, y)
    }

    /// What to highlight for a caret at byte `offset` of the source: the notes and bar whose
    /// source holds it, then a box per line for its syllable, most specific first. A caret
    /// just after a note, as after typing it, counts as on it; for a [`Chant`](crate::Chant)'s
    /// layout, one past the end of the source is at its end. For a caret in UTF-16 units,
    /// use [`elements_at_utf16`](Self::elements_at_utf16). See [`SourceMap::elements_at`].
    #[must_use]
    pub fn elements_at(&self, offset: usize) -> Vec<&Element> {
        let offset = self.utf16().map_or(offset, |u| offset.min(u.len));
        self.source_map().elements_at(offset)
    }

    /// [`elements_at`](Self::elements_at) for a caret counted in UTF-16 units, as text fields
    /// on the web, Android and iOS count it. Only a [`Chant`](crate::Chant)'s layouts know
    /// their source (see [`utf16`](Self::utf16)); a layout made with
    /// [`Engraving::layout`](crate::Engraving::layout) finds nothing.
    #[must_use]
    pub fn elements_at_utf16(&self, caret: usize) -> Vec<&Element> {
        match self.utf16() {
            Some(u) => self.elements_at(u.to_utf8(caret)),
            None => Vec::new(),
        }
    }

    fn make_source_map(&self) -> SourceMap {
        let eng = &*self.eng;
        let s = self.scale;
        let mut map = SourceMap {
            staff_space: s,
            ..SourceMap::default()
        };
        for (li, line) in self.lines.iter().enumerate() {
            let li = li as u32;
            map.lines.push(LineBox {
                top: line.top * s,
                bottom: line.bottom * s,
                staff: line.staff * s,
                baseline: line.baseline * s,
            });
            // The syllable being gathered on this line, and its left and right.
            let mut open: Option<(u32, f32, f32)> = None;
            let close = |open: &mut Option<(u32, f32, f32)>, map: &mut SourceMap| {
                if let Some((syl, l, r)) = open.take()
                    && let Some(span) = eng.syllable_spans.get(syl as usize)
                {
                    map.syllables.push(Element {
                        kind: ElementKind::Syllable,
                        index: syl,
                        span: span.clone(),
                        line: li,
                        x: l * s,
                        y: line.top * s,
                        w: (r - l) * s,
                        h: (line.bottom - line.top) * s,
                        cx: (l + r) / 2.0 * s,
                    });
                }
            };
            for (i, seg) in eng.segments[line.first..=line.last].iter().enumerate() {
                let x0 = line.xs[i];
                for h in &seg.heads {
                    let note = seg.note(h.note);
                    let Some(info) = eng.notes.get(note as usize) else { continue };
                    map.notes.push(Element {
                        kind: ElementKind::Note,
                        index: note,
                        span: info.span.clone(),
                        line: li,
                        x: (x0 + h.hit[0]) * s,
                        y: (line.staff + h.hit[1]) * s,
                        w: (h.hit[2] - h.hit[0]) * s,
                        h: (h.hit[3] - h.hit[1]) * s,
                        cx: (x0 + h.x) * s,
                    });
                }
                for b in &seg.bars {
                    let bar = seg.bar(b.bar);
                    let Some(span) = eng.bar_spans.get(bar as usize) else { continue };
                    map.bars.push(Element {
                        kind: ElementKind::Bar,
                        index: bar,
                        span: span.clone(),
                        line: li,
                        x: (x0 + b.x) * s,
                        y: (line.staff + b.top) * s,
                        w: b.w * s,
                        h: (b.bottom - b.top) * s,
                        cx: (x0 + b.x + b.w / 2.0) * s,
                    });
                }
                let mut extent: Option<(f32, f32)> = seg.ink;
                if let Some(t) = &seg.lyric {
                    let (l, r) = (t.left, t.left + t.width);
                    extent = Some(extent.map_or((l, r), |(a, b)| (a.min(l), b.max(r))));
                }
                let Some((l, r)) = extent else { continue };
                let (l, r) = (x0 + l, x0 + r);
                match &mut open {
                    Some((syl, a, b)) if *syl == seg.syllable => {
                        *a = a.min(l);
                        *b = b.max(r);
                    }
                    _ => {
                        close(&mut open, &mut map);
                        open = Some((seg.syllable, l, r));
                    }
                }
            }
            close(&mut open, &mut map);
        }
        // The initial belongs to the syllable it was taken from.
        if let (Some(init), Some(placed), Some(first)) = (&eng.initial, &self.initial, self.lines.first())
            && let Some(span) = eng.syllable_spans.get(init.syllable as usize)
        {
            let top = first.staff - 3.0;
            map.syllables.push(Element {
                kind: ElementKind::Syllable,
                index: init.syllable,
                span: span.clone(),
                line: 0,
                x: placed.x * s,
                y: top * s,
                w: init.advance_em * placed.size * s,
                h: (placed.baseline - top) * s,
                cx: (placed.x + init.advance_em * placed.size / 2.0) * s,
            });
        }
        map.notes.sort_by_key(|e| e.index);
        map.bars.sort_by_key(|e| e.index);
        map.syllables.sort_by_key(|e| e.index);
        map
    }
}

/// Converts offsets between UTF-8 bytes, which neuma's spans count, and UTF-16 code units,
/// which JavaScript, Java and Kotlin strings and `NSString` count. Build it once per source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Utf16Index {
    /// Each non-ASCII character: its byte offset, its UTF-16 offset, and its length in bytes
    /// and in UTF-16 units. Between them, bytes and units advance together.
    wide: Vec<(usize, usize, u8, u8)>,
    len: usize,
    len16: usize,
}

impl Utf16Index {
    /// The index of `src`.
    #[must_use]
    pub fn new(src: &str) -> Utf16Index {
        let mut wide = Vec::new();
        let mut units = 0;
        for (i, c) in src.char_indices() {
            if !c.is_ascii() {
                wide.push((i, units, c.len_utf8() as u8, c.len_utf16() as u8));
            }
            units += c.len_utf16();
        }
        Utf16Index {
            wide,
            len: src.len(),
            len16: units,
        }
    }

    /// The source's length in UTF-16 units.
    #[must_use]
    pub fn utf16_len(&self) -> usize {
        self.len16
    }

    /// The UTF-16 offset of byte `offset`. An offset inside a character maps to the
    /// character's start; one past the end, to the end.
    #[must_use]
    pub fn to_utf16(&self, offset: usize) -> usize {
        let offset = offset.min(self.len);
        let k = self.wide.partition_point(|w| w.0 <= offset);
        match k.checked_sub(1).map(|k| self.wide[k]) {
            None => offset,
            Some((b, u, bl, _)) if offset < b + bl as usize => u,
            Some((b, u, bl, ul)) => u + ul as usize + (offset - b - bl as usize),
        }
    }

    /// The byte offset of UTF-16 offset `offset`. An offset between the two halves of a
    /// surrogate pair maps to the character's start; one past the end, to the end.
    #[must_use]
    pub fn to_utf8(&self, offset: usize) -> usize {
        let offset = offset.min(self.len16);
        let k = self.wide.partition_point(|w| w.1 <= offset);
        match k.checked_sub(1).map(|k| self.wide[k]) {
            None => offset,
            Some((b, u, _, ul)) if offset < u + ul as usize => b,
            Some((b, u, bl, ul)) => b + bl as usize + (offset - u - ul as usize),
        }
    }

    /// A byte range as a UTF-16 range.
    #[must_use]
    pub fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApproxMeasure, Initial, StyleOptions, parse};

    #[test]
    fn utf16_round_trips() {
        let src = "a\u{e9}b\u{1d11e}c\u{2020}";
        let idx = Utf16Index::new(src);
        assert_eq!(idx.utf16_len(), src.encode_utf16().count());
        for (byte, c) in src.char_indices() {
            let units = src[..byte].encode_utf16().count();
            assert_eq!(idx.to_utf16(byte), units, "{c}");
            assert_eq!(idx.to_utf8(units), byte, "{c}");
            // Inside the character: its start.
            for inner in 1..c.len_utf8() {
                assert_eq!(idx.to_utf16(byte + inner), units);
            }
        }
        assert_eq!(idx.to_utf16(src.len()), idx.utf16_len());
        assert_eq!(idx.to_utf8(idx.utf16_len() + 5), src.len());
        // Between the halves of the clef's surrogate pair.
        assert_eq!(idx.to_utf8(4), 4);
        let ascii = Utf16Index::new("(c4) a(g)");
        assert_eq!((ascii.to_utf16(5), ascii.to_utf8(5)), (5, 5));
    }

    fn map(src: &str) -> SourceMap {
        let style = StyleOptions::default().with_initial(Initial::None);
        let eng = parse(src).score.engrave(&ApproxMeasure, &style);
        eng.layout(600.0).source_map().clone()
    }

    #[test]
    fn spans_cover_notes_bars_and_syllables() {
        let src = "(c4) Ky(g)ri(hi) (,) e(h) (::)";
        let m = map(src);
        let text = |e: &Element| &src[e.span.clone()];
        assert_eq!(m.notes.iter().map(text).collect::<Vec<_>>(), ["g", "h", "i", "h"]);
        assert_eq!(m.bars.iter().map(text).collect::<Vec<_>>(), [",", "::"]);
        assert_eq!(
            m.syllables.iter().map(text).collect::<Vec<_>>(),
            ["Ky(g)", "ri(hi)", "(,)", "e(h)", "(::)"]
        );
        for e in m.notes.iter().chain(&m.bars).chain(&m.syllables) {
            assert!(e.w > 0.0 && e.h > 0.0, "{e:?}");
        }
    }

    #[test]
    fn a_point_on_a_notehead_finds_that_note() {
        // Porrectus ends stacked on other notes, whose boxes are trimmed off them.
        let src = "(c4) a(hgh) b(ihi) c(jhj) d(gfgh) e(hghi) (::)";
        let style = StyleOptions::default().with_initial(crate::Initial::None);
        let eng = parse(src).score.engrave(&ApproxMeasure, &style);
        let layout = eng.layout(600.0);
        let m = layout.source_map();
        let (w, h) = layout.size();
        let mut checked = 0;
        for i in 0..400 {
            for j in 0..200 {
                let (x, y) = (w * i as f32 / 400.0, h * j as f32 / 200.0);
                let Some(e) = m.notes.iter().find(|n| n.contains(x, y, 0.0)) else {
                    continue;
                };
                assert_eq!(layout.note_at(x, y), Some(e.index), "at ({x}, {y})");
                checked += 1;
            }
        }
        assert!(checked > 100);
    }

    #[test]
    fn a_point_finds_its_source() {
        let src = "(c4) Ky(g)ri(hi) (,) e(h) (::)";
        let m = map(src);
        for n in &m.notes {
            let hit = m.source_at(n.x + n.w / 2.0, n.y + n.h / 2.0).unwrap();
            assert_eq!((hit.kind, hit.index), (ElementKind::Note, n.index));
        }
        let bar = &m.bars[0];
        let hit = m.source_at(bar.x + bar.w / 2.0, bar.y + 1.0).unwrap();
        assert_eq!((hit.kind, hit.index), (ElementKind::Bar, 0));
        // Under a note, on its lyric: the syllable.
        let ky = &m.syllables[0];
        let (top, bottom) = (m.lines[0].top, m.lines[0].bottom);
        let hit = m.source_at(ky.x + 1.0, bottom - 1.0).unwrap();
        assert_eq!((hit.kind, &src[hit.span.clone()]), (ElementKind::Syllable, "Ky(g)"));
        // Past the end of the line: the nearest syllable.
        let hit = m.source_at(10_000.0, (top + bottom) / 2.0).unwrap();
        assert_eq!(&src[hit.span.clone()], "(::)");
        assert!(m.source_at(0.0, bottom + 100.0).is_none());
    }

    #[test]
    fn a_caret_finds_what_to_highlight() {
        let src = "(c4) Ky(g)ri(hi) (,) e(h) (::)";
        let m = map(src);
        let at = |offset: usize| {
            m.elements_at(offset)
                .iter()
                .map(|e| format!("{:?} {}", e.kind, &src[e.span.clone()]))
                .collect::<Vec<_>>()
        };
        let h = src.find("hi").unwrap();
        assert_eq!(at(h), ["Note h", "Syllable ri(hi)"]);
        // Just after a note, as after typing it.
        assert_eq!(at(h + 2), ["Note i", "Syllable ri(hi)"]);
        assert_eq!(at(src.find('K').unwrap()), ["Syllable Ky(g)"]);
        assert_eq!(at(src.find(',').unwrap()), ["Bar ,", "Syllable (,)"]);
        assert!(at(0).is_empty() || at(0).iter().all(|e| !e.starts_with("Note")));
    }

    #[test]
    fn a_split_syllable_has_a_box_per_line() {
        let melisma = format!("(c4) A({}) (::)", "g/h/".repeat(40));
        let style = StyleOptions::default().with_initial(Initial::None);
        let eng = parse(&melisma).score.engrave(&ApproxMeasure, &style);
        let layout = eng.layout(200.0);
        let m = layout.source_map();
        let a: Vec<_> = m.syllables.iter().filter(|s| s.index == 1).collect();
        assert!(a.len() > 1, "{a:?}");
        assert!(a.windows(2).all(|w| w[0].line < w[1].line));
        assert_eq!(m.elements_at(melisma.find('A').unwrap()).len(), a.len());
    }
}
