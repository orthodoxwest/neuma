//! Engraving: turns a [`Score`] into width-independent segments. Each segment is a stretch of
//! notation (and the lyric under it) with geometry relative to its own origin, plus what may
//! happen at the break after it. Line breaking places segments on lines (`layout`).

pub(crate) mod neume;

use crate::diag::{Diagnostic, Sink};
use crate::glyphs::GlyphId as G;
use crate::score::{
    AlterationKind, BarKind, Clef, ClefKind, CustosRule, Figure, LyricRun, Note, NoteShape, Score, Space, StaffPosition, TextStyle,
};
use crate::text::TextMeasure;
use crate::vowel::VowelRules;
use neume::{INTRA, STEM};

/// What a piece of ink is, so themes can color staff, notes and rubrics separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ink {
    Staff,
    Ledger,
    Note,
    Stem,
    Bar,
    Episema,
    Mora,
    Ictus,
    Accidental,
    Clef,
    Custos,
}

impl Ink {
    /// The CSS class the SVG writer uses for this role.
    pub fn class(self) -> &'static str {
        match self {
            Ink::Staff => "staff",
            Ink::Ledger => "ledger",
            Ink::Note => "note",
            Ink::Stem => "stem",
            Ink::Bar => "bar",
            Ink::Episema => "episema",
            Ink::Mora => "mora",
            Ink::Ictus => "ictus",
            Ink::Accidental => "accidental",
            Ink::Clef => "clef",
            Ink::Custos => "custos",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    /// A glyph drawn with its anchor at (x, y).
    Glyph {
        glyph: G,
        x: f32,
        y: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Piece {
    pub mark: Mark,
    pub role: Ink,
    /// Score-wide index of the note this ink belongs to.
    pub note: Option<u32>,
}

impl Piece {
    pub(crate) fn shifted(mut self, dx: f32) -> Piece {
        match &mut self.mark {
            Mark::Glyph { x, .. } | Mark::Rect { x, .. } => *x += dx,
        }
        self
    }

    /// Vertical ink extent.
    pub(crate) fn y_extent(&self) -> (f32, f32) {
        match self.mark {
            Mark::Glyph { glyph, y, .. } => {
                let (_, a, _, b) = glyph.ink();
                (y + a, y + b)
            }
            Mark::Rect { y, h, .. } => (y, y + h),
        }
    }
}

/// How long an alteration lasts (DESIGN section 6.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlterationScope {
    /// Until the next clef or written line break. Engraving doesn't know where layout will
    /// break lines, so an alteration carries past a line break that layout chose.
    Line,
    Word,
    Bar,
    #[default]
    WordOrBar,
    Note,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CustosPolicy {
    #[default]
    Auto,
    Never,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleOptions {
    /// Lyric font size, in staff spaces.
    pub lyric_size: f32,
    /// Overrides the rules the `language:` header picks.
    pub vowels: Option<VowelRules>,
    pub alterations: AlterationScope,
    pub custos: CustosPolicy,
}

impl Default for StyleOptions {
    fn default() -> StyleOptions {
        StyleOptions {
            lyric_size: 2.7,
            vowels: None,
            alterations: AlterationScope::default(),
            custos: CustosPolicy::default(),
        }
    }
}

/// What may happen at the boundary after a segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Break {
    Allowed,
    Forbidden,
    Forced { justify: bool, custos: CustosRule },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LyricBox {
    pub runs: Vec<LyricRun>,
    /// Left edge relative to the segment origin, and width, in staff spaces.
    pub left: f32,
    pub width: f32,
    /// The syllable ends its word, so no hyphen follows it.
    pub word_end: bool,
    pub syllable: u32,
}

/// A notehead in a segment, for the note map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeadBox {
    pub note: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Segment {
    pub syllable: u32,
    /// The first segment of its syllable (only it carries the lyric).
    pub first: bool,
    pub word_start: bool,
    pub pieces: Vec<Piece>,
    pub heads: Vec<HeadBox>,
    /// Horizontal extent of the notation, if there is any.
    pub ink: Option<(f32, f32)>,
    pub lyric: Option<LyricBox>,
    pub after: Break,
    /// Space written before this segment inside its syllable (for segments after the first).
    pub space_before: f32,
    /// The clef in force after this segment.
    pub clef: Clef,
    pub starts_with_clef: bool,
    /// Position of the first note, for the custos that announces this segment.
    pub first_note: Option<StaffPosition>,
    pub suppress_custos: bool,
}

impl Segment {
    pub(crate) fn right(&self) -> f32 {
        let r = self.ink.map_or(0.0, |(_, r)| r);
        match &self.lyric {
            Some(t) => r.max(t.left + t.width),
            None => r,
        }
    }
}

/// Information about each note in the score, by score-wide index.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NoteInfo {
    pub syllable: u32,
    pub position: StaffPosition,
    pub shape: NoteShape,
    pub liquescent: bool,
    pub morae: u8,
    pub episema: bool,
    pub span: std::ops::Range<usize>,
    pub clef: Clef,
    /// Semitone shift from alterations in force: −1 flat, +1 sharp.
    pub alteration: i8,
    pub vowel: Option<char>,
}

/// A score engraved independently of width. Lay it out with [`Engraving::layout`].
#[derive(Clone, Debug, PartialEq)]
pub struct Engraving {
    pub(crate) segments: Vec<Segment>,
    pub(crate) clef: Clef,
    pub(crate) notes: Vec<NoteInfo>,
    pub(crate) syllable_text: Vec<String>,
    pub(crate) syllable_word: Vec<u32>,
    pub(crate) lyric_size: f32,
    pub(crate) hyphen: f32,
    pub(crate) word_space: f32,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) alt_text: String,
    pub(crate) pauses: Vec<(u32, BarKind)>,
    pub(crate) custos_never: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// Gap between the notation of neighbouring syllables (exsurge's `interSyllabicMultiplier`).
pub(crate) const SYLLABLE_GAP: f32 = INTRA * 2.5;
/// Extra gap between words.
pub(crate) const WORD_GAP: f32 = INTRA;
const ACCIDENTAL_GAP: f32 = INTRA * 2.0;
const DEFAULT_CLEF: Clef = Clef {
    kind: ClefKind::Do,
    line: 4,
    flat: false,
    span: 0..0,
};

fn space_width(s: Space) -> f32 {
    match s {
        Space::Zero => 0.0,
        Space::Tiny => INTRA * 0.5,
        Space::Half => INTRA * 0.5,
        Space::Small => INTRA,
        Space::Medium => INTRA * 2.0,
        Space::Large | Space::LargeNoBreak => INTRA * 2.0,
        Space::Scaled(f) => INTRA * f,
    }
}

/// Places a glyph so its ink starts at `left`.
fn ink_at(glyph: G, left: f32, y: f32, role: Ink, note: Option<u32>) -> (Piece, f32) {
    let (a, _, c, _) = glyph.ink();
    (
        Piece {
            mark: Mark::Glyph { glyph, x: left - a, y },
            role,
            note,
        },
        c - a,
    )
}

fn rect(x: f32, top: StaffPosition, bottom: StaffPosition, role: Ink) -> Piece {
    let y = -(top as f32);
    Piece {
        mark: Mark::Rect {
            x,
            y,
            w: STEM,
            h: (top - bottom) as f32,
        },
        role,
        note: None,
    }
}

pub(crate) fn clef_pieces(clef: &Clef, left: f32) -> (Vec<Piece>, f32) {
    let glyph = if clef.kind == ClefKind::Do { G::DoClef } else { G::FaClef };
    let p = clef.position();
    let (piece, w) = ink_at(glyph, left, -(p as f32), Ink::Clef, None);
    let mut out = vec![piece];
    let mut right = left + w;
    if clef.flat {
        // The key flat sits on B: a step below do, or three steps above fa.
        let mut b = if clef.kind == ClefKind::Do { p - 1 } else { p + 3 };
        if b > 4 {
            b -= 7;
        }
        if b < -4 {
            b += 7;
        }
        let (piece, w) = ink_at(G::Flat, right + 0.2, -(b as f32), Ink::Clef, None);
        out.push(piece);
        right += 0.2 + w;
    }
    (out, right)
}

pub(crate) fn custos_piece(position: StaffPosition, left: f32) -> (Piece, f32) {
    let glyph = match (position <= 2, position.rem_euclid(2) == 1) {
        (true, true) => G::CustosLong,
        (true, false) => G::CustosShort,
        (false, true) => G::CustosDescLong,
        (false, false) => G::CustosDescShort,
    };
    ink_at(glyph, left, -(position as f32), Ink::Custos, None)
}

fn bar_pieces(kind: BarKind, high: bool, left: f32) -> (Vec<Piece>, f32) {
    let shift = if high { -2 } else { 0 };
    let bar = |top: StaffPosition, bottom: StaffPosition| rect(left, top + shift, bottom + shift, Ink::Bar);
    match kind {
        BarKind::Virgula => {
            let (p, w) = ink_at(G::Virgula, left, -(3 + shift) as f32, Ink::Bar, None);
            (vec![p], w)
        }
        BarKind::Minimis => (vec![bar(4, 3)], STEM),
        BarKind::Minima => (vec![bar(4, 2)], STEM),
        BarKind::Minor => (vec![bar(2, -2)], STEM),
        BarKind::Maior => (vec![bar(3, -3)], STEM),
        BarKind::DottedMaior => {
            let mut out = Vec::new();
            let mut y = -3.0;
            while y < 3.0 {
                out.push(Piece {
                    mark: Mark::Rect {
                        x: left,
                        y,
                        w: STEM,
                        h: 0.6,
                    },
                    role: Ink::Bar,
                    note: None,
                });
                y += 1.0;
            }
            (out, STEM)
        }
        BarKind::Finalis => {
            let w = INTRA * 2.0;
            (
                vec![bar(3, -3), rect(left + w - STEM * 2.0, 3, -3, Ink::Bar)]
                    .into_iter()
                    .map(|mut p| {
                        if let Mark::Rect { w: rw, x, .. } = &mut p.mark
                            && *x > left
                        {
                            *rw = STEM * 2.0;
                        }
                        p
                    })
                    .collect(),
                w,
            )
        }
        BarKind::Dominican(n) => {
            // `;1`–`;8`: a short bar through line (n+1)/2 for odd n, the space for even n.
            let base = n as StaffPosition - 5;
            (vec![bar(base + 2, base - 1)], STEM)
        }
    }
}

struct Run {
    notes: Vec<Note>,
    ids: Vec<u32>,
}

struct Engraver<'a> {
    style: &'a StyleOptions,
    rules: VowelRules,
    sink: Sink,
    segments: Vec<Segment>,
    notes: Vec<NoteInfo>,
    clef: Clef,
    initial_clef: Option<Clef>,
    pauses: Vec<(u32, BarKind)>,
    alteration: Vec<(StaffPosition, i8)>,
    /// Positions of every note in source order, for automatic custodes.
    note_positions: Vec<StaffPosition>,
}

/// The segment being built.
struct Open {
    pieces: Vec<Piece>,
    heads: Vec<HeadBox>,
    x: f32,
    gap: Option<f32>,
    starts_with_clef: bool,
    first_note: Option<StaffPosition>,
    empty: bool,
}

impl Open {
    fn new() -> Open {
        Open {
            pieces: Vec::new(),
            heads: Vec::new(),
            x: 0.0,
            gap: None,
            starts_with_clef: false,
            first_note: None,
            empty: true,
        }
    }

    /// Where the next item starts: after the gap written before it, or `default`.
    fn advance(&mut self, default: f32) -> f32 {
        if !self.empty {
            self.x += self.gap.take().unwrap_or(default);
        } else {
            self.gap = None;
        }
        self.empty = false;
        self.x
    }
}

impl Engraver<'_> {
    fn flush(&mut self, run: &mut Run, open: &mut Open, syllable: u32) {
        if run.notes.is_empty() {
            return;
        }
        let next_note = self.note_positions.get(*run.ids.last().unwrap_or(&0) as usize + 1).copied();
        let splits = neume::split(&mut run.notes);
        let mut x = open.advance(SYLLABLE_GAP);
        for (k, s) in splits.iter().enumerate() {
            let notes = &run.notes[s.start..s.end];
            let next = run.notes.get(s.end).map(|n| n.position).or(next_note);
            let base = run.ids[s.start];
            if s.kind == neume::Kind::Porrectus || s.kind == neume::Kind::PorrectusFlexus {
                let d = notes[0].position - notes[1].position;
                if d > 4 {
                    self.sink.info(
                        notes[0].span.start..notes[1].span.end,
                        "engrave::wide-porrectus",
                        "no porrectus glyph spans more than a fifth; drawn as two puncta",
                    );
                }
            }
            let mut built = neume::build(s.kind, notes, next, base);
            neume::add_markings(&mut built, s.kind, notes, base);
            for p in &built.pieces {
                open.pieces.push(p.shifted(x));
            }
            for h in &built.heads {
                let y = -(h.position as f32);
                open.heads.push(HeadBox {
                    note: base + h.index as u32,
                    x: h.center() + x,
                    y,
                    w: h.w.max(0.5),
                    h: h.bottom - h.top,
                });
            }
            if open.first_note.is_none() {
                open.first_note = notes.first().map(|n| n.position);
            }
            if k + 1 < splits.len() {
                x += built.width + s.trailing.unwrap_or(INTRA);
            } else {
                x += built.width;
            }
        }
        open.x = x;
        for (i, n) in run.notes.iter().enumerate() {
            let id = run.ids[i] as usize;
            if let Some(info) = self.notes.get_mut(id) {
                info.syllable = syllable;
            }
            let _ = n;
        }
        run.notes.clear();
        run.ids.clear();
    }

    fn close(&mut self, open: Open, syllable: u32, first: bool, word_start: bool, space_before: f32) -> Option<usize> {
        if open.pieces.is_empty() && !first {
            return None;
        }
        let ink = neume::extent(&open.pieces);
        self.segments.push(Segment {
            syllable,
            first,
            word_start,
            pieces: open.pieces,
            heads: open.heads,
            ink,
            lyric: None,
            after: Break::Allowed,
            space_before,
            clef: self.clef.clone(),
            starts_with_clef: open.starts_with_clef,
            first_note: open.first_note,
            suppress_custos: false,
        });
        Some(self.segments.len() - 1)
    }

    fn alteration_of(&self, position: StaffPosition, clef: &Clef) -> i8 {
        if let Some((_, a)) = self.alteration.iter().rev().find(|(p, _)| *p == position) {
            return *a;
        }
        if clef.flat {
            let b = if clef.kind == ClefKind::Do {
                clef.position() - 1
            } else {
                clef.position() + 3
            };
            if (position - b).rem_euclid(7) == 0 {
                return -1;
            }
        }
        0
    }

    fn reset_alterations(&mut self, word: bool, bar: bool) {
        let clear = match self.style.alterations {
            AlterationScope::Line | AlterationScope::Note => false,
            AlterationScope::Word => word,
            AlterationScope::Bar => bar,
            AlterationScope::WordOrBar => word || bar,
        };
        if clear {
            self.alteration.clear();
        }
    }
}

impl Score {
    /// Engraves the score: neumes, signs and lyric boxes, independent of width.
    pub fn engrave(&self, measure: &dyn TextMeasure, style: &StyleOptions) -> Engraving {
        let rules = style
            .vowels
            .clone()
            .or_else(|| self.header.get("language").and_then(VowelRules::builtin))
            .unwrap_or_else(VowelRules::latin);
        let mut e = Engraver {
            style,
            rules,
            sink: Sink::default(),
            segments: Vec::new(),
            notes: Vec::new(),
            clef: DEFAULT_CLEF,
            initial_clef: None,
            pauses: Vec::new(),
            alteration: Vec::new(),
            note_positions: Vec::new(),
        };
        if let Some(lang) = self.header.get("language")
            && style.vowels.is_none()
            && VowelRules::builtin(lang).is_none()
        {
            e.sink.info(
                0..0,
                "engrave::vowel-rules",
                format!("no vowel rules for `{lang}`; centering with the Latin rules"),
            );
        }
        for syl in &self.syllables {
            for f in &syl.notation {
                if let Figure::Note(n) = f {
                    e.note_positions.push(n.position);
                }
            }
        }

        let size = style.lyric_size;
        let hyphen = measure.advance("-", TextStyle::REGULAR) * size;
        let word_space = measure.advance(" ", TextStyle::REGULAR) * size;
        let (ascent, descent) = measure.vertical(TextStyle::REGULAR);
        let mut syllable_text = Vec::new();
        let mut syllable_word = Vec::new();
        let mut alt_text = String::new();
        let mut word = 0u32;
        let mut warned_face = false;
        let mut pending_break: Option<Break> = None;
        let mut nocustos = false;

        for (si, syl) in self.syllables.iter().enumerate() {
            let si = si as u32;
            if syl.word_start && si > 0 {
                word += 1;
            }
            syllable_word.push(word);
            let plain = syl.text.plain();
            if !plain.is_empty() {
                if syl.word_start && !alt_text.is_empty() {
                    alt_text.push(' ');
                }
                alt_text.push_str(&plain);
            }
            syllable_text.push(plain);
            e.reset_alterations(syl.word_start, false);

            let mut open = Open::new();
            let mut run = Run {
                notes: Vec::new(),
                ids: Vec::new(),
            };
            let mut first_seg = true;
            let mut space_before = 0.0;
            let mut seg_ids: Vec<usize> = Vec::new();
            let only_clef = syl.text.is_empty() && syl.notation.iter().all(|f| matches!(f, Figure::Clef(_) | Figure::Space(_)));

            for f in &syl.notation {
                match f {
                    Figure::Note(n) => {
                        let id = e.notes.len() as u32;
                        let alteration = if e.style.alterations == AlterationScope::Note {
                            0
                        } else {
                            e.alteration_of(n.position, &e.clef.clone())
                        };
                        e.notes.push(NoteInfo {
                            syllable: si,
                            position: n.position,
                            shape: n.shape,
                            liquescent: n.liquescent != crate::score::Liquescent::None,
                            morae: n.morae,
                            episema: n.episema.is_some(),
                            span: n.span.clone(),
                            clef: e.clef.clone(),
                            alteration,
                            vowel: None,
                        });
                        if e.style.alterations == AlterationScope::Note {
                            e.alteration.clear();
                        }
                        run.notes.push(n.clone());
                        run.ids.push(id);
                    }
                    Figure::Space(s) => {
                        e.flush(&mut run, &mut open, si);
                        if *s == Space::Large && !open.empty {
                            let o = std::mem::replace(&mut open, Open::new());
                            if let Some(k) = e.close(o, si, first_seg, syl.word_start, space_before) {
                                seg_ids.push(k);
                                first_seg = false;
                            }
                            space_before = space_width(*s);
                        } else {
                            open.gap = Some(open.gap.unwrap_or(0.0) + space_width(*s));
                        }
                    }
                    Figure::Clef(c) => {
                        e.flush(&mut run, &mut open, si);
                        e.alteration.clear();
                        e.clef = c.clone();
                        if e.initial_clef.is_none() && e.segments.is_empty() && only_clef {
                            e.initial_clef = Some(c.clone());
                            continue;
                        }
                        if open.empty {
                            open.starts_with_clef = true;
                        }
                        let x = open.advance(SYLLABLE_GAP);
                        let (pieces, right) = clef_pieces(c, x);
                        open.pieces.extend(pieces);
                        open.x = right;
                        open.gap.get_or_insert(SYLLABLE_GAP);
                    }
                    Figure::Alteration(a) => {
                        e.flush(&mut run, &mut open, si);
                        let delta = match a.kind {
                            AlterationKind::Flat => -1,
                            AlterationKind::Natural => 0,
                            AlterationKind::Sharp => 1,
                        };
                        let in_force = e.alteration_of(a.position, &e.clef.clone());
                        e.alteration.push((a.position, delta));
                        if a.soft && in_force == delta {
                            continue;
                        }
                        let glyph = match a.kind {
                            AlterationKind::Flat => G::Flat,
                            AlterationKind::Natural => G::Natural,
                            AlterationKind::Sharp => G::Sharp,
                        };
                        let x = open.advance(SYLLABLE_GAP);
                        let (piece, w) = ink_at(glyph, x, -(a.position as f32), Ink::Accidental, None);
                        open.pieces.push(piece);
                        open.x = x + w;
                        open.gap.get_or_insert(ACCIDENTAL_GAP * 0.5);
                    }
                    Figure::Bar(b) => {
                        e.flush(&mut run, &mut open, si);
                        e.reset_alterations(false, true);
                        let x = open.advance(SYLLABLE_GAP);
                        let (pieces, w) = bar_pieces(b.kind, b.high, x);
                        open.pieces.extend(pieces);
                        open.x = x + w;
                        e.pauses.push((e.notes.len() as u32, b.kind));
                    }
                    Figure::Custos { position, .. } => {
                        e.flush(&mut run, &mut open, si);
                        let pos = position.or_else(|| e.note_positions.get(e.notes.len()).copied());
                        if let Some(p) = pos {
                            let x = open.advance(SYLLABLE_GAP);
                            let (piece, w) = custos_piece(p, x);
                            open.pieces.push(piece);
                            open.x = x + w;
                        }
                    }
                    Figure::Break(b) => {
                        e.flush(&mut run, &mut open, si);
                        let brk = Break::Forced {
                            justify: b.justify,
                            custos: b.custos,
                        };
                        if open.empty {
                            pending_break = Some(brk);
                        } else {
                            let o = std::mem::replace(&mut open, Open::new());
                            if let Some(k) = e.close(o, si, first_seg, syl.word_start, space_before) {
                                e.segments[k].after = brk;
                                seg_ids.push(k);
                                first_seg = false;
                            }
                            space_before = 0.0;
                        }
                    }
                    Figure::NoCustos => nocustos = true,
                }
            }
            e.flush(&mut run, &mut open, si);
            if only_clef && e.initial_clef.as_ref().is_some_and(|c| *c == e.clef) && e.segments.is_empty() {
                continue;
            }
            if let Some(k) = e.close(open, si, first_seg, syl.word_start, space_before) {
                seg_ids.push(k);
            }
            if seg_ids.is_empty() {
                continue;
            }
            // A break written in its own syllable applies after the previous segment.
            if let Some(brk) = pending_break.take() {
                let k = seg_ids[0];
                let break_only = syl
                    .notation
                    .iter()
                    .all(|f| matches!(f, Figure::Break(_) | Figure::Space(_) | Figure::NoCustos));
                if break_only && k > 0 {
                    e.segments[k - 1].after = brk;
                    if syl.text.is_empty() {
                        e.segments.truncate(k);
                        continue;
                    }
                    // The syllable's text still needs its segment, at the start of the new line.
                } else {
                    e.segments[*seg_ids.last().unwrap_or(&k)].after = brk;
                }
            }
            if nocustos {
                if let Some(k) = seg_ids.last() {
                    e.segments[*k].suppress_custos = true;
                }
                nocustos = false;
            }
            if syl.no_break_before {
                let k = seg_ids[0];
                if k > 0 && !matches!(e.segments[k - 1].after, Break::Forced { .. }) {
                    e.segments[k - 1].after = Break::Forbidden;
                }
            }

            // The lyric goes under the first segment, its vowel over the first note.
            if !syl.text.is_empty() {
                let k = seg_ids[0];
                let runs = syl.text.runs.clone();
                let mut width = 0.0;
                for r in &runs {
                    width += measure.advance(&r.text, r.style) * size;
                    if !measure.has_face(r.style) {
                        width += measure.advance(&r.text, TextStyle::REGULAR) * size * 0.03;
                        if !warned_face {
                            e.sink.info(
                                syl.span.clone(),
                                "text::synthetic-face",
                                "no face for this style; measured with the regular face widened 3%",
                            );
                            warned_face = true;
                        }
                    }
                }
                let chars: Vec<char> = syl.text.plain().chars().collect();
                let mut masked = chars.clone();
                let mut ci = 0;
                for r in &runs {
                    for _ in r.text.chars() {
                        if r.consonant {
                            masked[ci] = '\u{0}';
                        }
                        ci += 1;
                    }
                }
                let nucleus = syl.text.center.clone().or_else(|| e.rules.nucleus(&masked));
                let seg = &e.segments[k];
                let anchor = match seg.heads.first() {
                    Some(h) => h.x,
                    None => seg.ink.map_or(0.0, |(l, r)| (l + r) / 2.0),
                };
                let center = match &nucleus {
                    Some(r) => {
                        let before = prefix_advance(&runs, r.start, measure) * size;
                        let upto = prefix_advance(&runs, r.end, measure) * size;
                        (before + upto) / 2.0
                    }
                    None => width / 2.0,
                };
                if let Some(r) = &nucleus
                    && let Some(c) = chars.get(r.start)
                {
                    for n in e.notes.iter_mut().filter(|n| n.syllable == si) {
                        n.vowel = Some(*c);
                    }
                }
                let next_word = self.syllables.get(si as usize + 1).is_none_or(|s| s.word_start);
                e.segments[k].lyric = Some(LyricBox {
                    runs,
                    left: anchor - center,
                    width,
                    word_end: next_word,
                    syllable: si,
                });
            }
        }
        if let Some(brk) = pending_break
            && let Some(last) = e.segments.last_mut()
        {
            let _ = brk;
            last.after = Break::Allowed;
            e.sink
                .info(0..0, "engrave::final-break", "a line break at the end of the score is dropped");
        }

        Engraving {
            segments: e.segments,
            clef: e.initial_clef.unwrap_or(DEFAULT_CLEF),
            notes: e.notes,
            syllable_text,
            syllable_word,
            lyric_size: size,
            hyphen,
            word_space,
            ascent,
            descent,
            alt_text,
            pauses: e.pauses,
            custos_never: style.custos == CustosPolicy::Never,
            diagnostics: e.sink.items,
        }
    }
}

/// Advance of the first `chars` characters of the runs, in ems.
fn prefix_advance(runs: &[LyricRun], chars: usize, measure: &dyn TextMeasure) -> f32 {
    let mut left = chars;
    let mut w = 0.0;
    for r in runs {
        let n = r.text.chars().count();
        if left >= n {
            w += measure.advance(&r.text, r.style);
            left -= n;
        } else {
            let part: String = r.text.chars().take(left).collect();
            w += measure.advance(&part, r.style);
            break;
        }
    }
    w
}
