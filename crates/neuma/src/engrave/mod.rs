//! Engraving: turns a [`Score`] into width-independent segments. Each segment is a stretch of
//! notation (and the lyric under it) with geometry relative to its own origin, plus what may
//! happen at the break after it. Line breaking places segments on lines (`layout`).

mod initial;
pub(crate) mod neume;

pub use initial::Initial;
pub(crate) use initial::{CAP_HEIGHT, strip_tex};

use crate::diag::{Diagnostic, Sink};
use crate::glyphs::GlyphId as G;
use crate::notes::PauseKind;
use crate::score::{
    AlterationKind, BarKind, Clef, ClefKind, CustosRule, Figure, Lyric, LyricRun, Note, NoteShape, Score, Space, StaffPosition, TextStyle,
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
    /// For ink that draws several notes at once (a porrectus swash), the last of them.
    pub through: Option<u32>,
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
    /// Lyric font size, in staff spaces. The default, 2.45, is GregorioTeX's: 10 pt lyrics on
    /// its default staff.
    pub lyric_size: f32,
    /// The drop-cap initial.
    pub initial: Initial,
    /// Show the `annotation` headers (or the mode) above the initial.
    pub annotation: bool,
    /// Overrides the rules the `language:` header picks.
    pub vowels: Option<VowelRules>,
    pub alterations: AlterationScope,
    pub custos: CustosPolicy,
}

impl Default for StyleOptions {
    fn default() -> StyleOptions {
        StyleOptions {
            lyric_size: 2.45,
            initial: Initial::default(),
            annotation: true,
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
    /// Between two note groups of a long melisma, as Gregorio allows.
    InMelisma,
    Forbidden,
    Forced {
        justify: bool,
        custos: CustosRule,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LyricBox {
    pub runs: Vec<LyricRun>,
    /// Left edge relative to the segment origin, and width, in staff spaces.
    pub left: f32,
    pub width: f32,
    /// The syllable ends its word, so no hyphen follows it.
    pub word_end: bool,
    /// The text ends with a hyphen of its own (`Giê-(f)su(g)`), so none is added after it.
    pub hyphenated: bool,
    /// Not the syllable's text but the hyphen GregorioTeX sets under a first syllable the
    /// initial took whole.
    pub lead_hyphen: bool,
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

/// The drop cap and its annotations, sized at engrave time; layout indents the first
/// `lines` staves to make room.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InitialBox {
    pub text: String,
    /// The syllable the letter came from.
    pub syllable: u32,
    /// Font size and width of the initial at its nominal height, in staff spaces. They size
    /// the column; layout scales the capital to the staves it spans.
    pub size: f32,
    pub width: f32,
    /// The initial's advance in ems.
    pub advance_em: f32,
    /// Room above the first staff for an accent on the capital, in staff spaces.
    pub accent_room: f32,
    /// The face's descent in ems, for a capital with a tail.
    pub descent: f32,
    pub lines: usize,
    /// Annotation lines, top first, with their widths.
    pub annotations: Vec<(String, f32)>,
    pub annotation_size: f32,
    pub annotation_ascent: f32,
}

impl InitialBox {
    /// Width of the column the initial and its annotations share.
    pub fn column(&self) -> f32 {
        self.annotations.iter().map(|a| a.1).fold(self.width, f32::max)
    }
}

/// A score engraved independently of width. Lay it out with [`Engraving::layout`].
#[derive(Clone, Debug, PartialEq)]
pub struct Engraving {
    pub(crate) segments: Vec<Segment>,
    pub(crate) initial: Option<InitialBox>,
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
    pub(crate) pauses: Vec<(u32, PauseKind)>,
    /// The segment each pause is drawn in, parallel to `pauses`.
    pub(crate) pause_segments: Vec<usize>,
    pub(crate) custos_never: bool,
    /// The lowest note's staff position, or 0 for a score without notes.
    pub(crate) lowest: StaffPosition,
    pub diagnostics: Vec<Diagnostic>,
}

/// Gap between a clef, bar or accidental and the notes beside it in one syllable (exsurge's
/// `interSyllabicMultiplier`). Layout sets the gaps between syllables.
pub(crate) const SYLLABLE_GAP: f32 = INTRA * 2.5;
const ACCIDENTAL_GAP: f32 = INTRA * 2.0;
/// GregorioTeX's default `\gresetunbreakablesyllablenotes{10}{4}{4}`: a syllable of at least
/// this many notes may break between its note groups, but not within this many of either end.
const MELISMA_NOTES: usize = 10;
const MELISMA_END_NOTES: usize = 4;
/// The least space between the texts of two words, in ems of the lyrics: GregorioTeX's
/// `interwordspacetext` (0.17 cm against 10 pt lyrics). The font's own space is narrower, and
/// set words ran together.
const WORD_SPACE: f32 = 0.48;
/// A one-staff initial's size relative to the lyrics (GregorioTeX's 40 pt and 10 pt).
const INITIAL_SCALE: f32 = 4.0;
/// Annotation size relative to the lyrics.
const ANNOTATION_RATIO: f32 = 0.75;
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
            through: None,
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
        through: None,
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

/// The distance between the two bars of a `::`, measured from GregorioTeX's output.
const FINALIS_SEP: f32 = 0.94;

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
                    through: None,
                });
                y += 1.0;
            }
            (out, STEM)
        }
        BarKind::Finalis => {
            // Two thin bars, as GregorioTeX draws `::`, their centres 0.94 staff spaces apart.
            let second = left + FINALIS_SEP;
            (vec![bar(3, -3), rect(second, 3 + shift, -3 + shift, Ink::Bar)], FINALIS_SEP + STEM)
        }
        BarKind::Dominican(n) => {
            // `;1`–`;8`: a bar an interline and a half long, as GregorioTeX draws them. An odd
            // one rises from line (n+1)/2; an even one hangs from line n/2+1. Lines count from
            // the bottom, so `;7` and `;8` reach above the staff.
            let n = n as StaffPosition;
            let (top, bottom) = if n % 2 == 1 { (n - 1, n - 4) } else { (n - 3, n - 6) };
            (vec![bar(top, bottom)], STEM)
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
    pauses: Vec<(u32, PauseKind)>,
    pause_segments: Vec<usize>,
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
        // A double mora after a neume ending on two pitches (`hg..`) is one mora for each of
        // its last two notes, as Gregorio reads it: a dot after the neume at each note's height.
        for s in &splits {
            if s.end - s.start >= 2 {
                let (a, b) = (s.end - 2, s.end - 1);
                if run.notes[b].morae == 2 && run.notes[a].morae == 0 && run.notes[a].position != run.notes[b].position {
                    run.notes[a].morae = 1;
                    run.notes[b].morae = 1;
                    for k in [a, b] {
                        if let Some(info) = self.notes.get_mut(run.ids[k] as usize) {
                            info.morae = 1;
                        }
                    }
                }
            }
        }
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
                let (w, height) = h.size();
                open.heads.push(HeadBox {
                    note: base + h.index as u32,
                    x: h.center() + x,
                    y,
                    w,
                    h: height,
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
            // The score's opening clef is drawn at the start of the first line even when a clef
            // change follows it at once, as in `(c4) (c3)`: Gregorio shows both.
            starts_with_clef: open.starts_with_clef && !(self.segments.is_empty() && self.initial_clef.is_some()),
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
            pause_segments: Vec::new(),
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
        let word_space = measure.advance(" ", TextStyle::REGULAR).max(WORD_SPACE) * size;
        let (ascent, descent) = measure.vertical(TextStyle::REGULAR);

        // The drop cap comes off the first syllable with text; the rest of it is the lyric.
        let mut initial = None;
        let mut first_lyric: Option<(usize, Lyric)> = None;
        // Only a syllable with no notes before it: the initial is drawn on the first line.
        let first_texted = self.syllables.iter().position(|s| !s.text.is_empty()).filter(|&si| {
            self.syllables[..si]
                .iter()
                .all(|s| !s.notation.iter().any(|f| matches!(f, Figure::Note(_))))
        });
        if let Initial::Lines(n) = style.initial
            && n > 0
            && let Some(si) = first_texted
            && let Some((text, rest)) = initial::split_initial(&self.syllables[si].text)
        {
            // A nominal staff-to-staff distance (the staff, lyrics below it, and the gaps)
            // sizes the column the breaker indents for; layout sizes the capital itself to
            // the staves it actually spans.
            let line_pitch = crate::layout::BASELINE_PITCH;
            let lines = n.min(initial::MAX_LINES) as usize;
            let cap = 6.0 + line_pitch * (lines - 1) as f32;
            // A one-staff initial is GregorioTeX's default: 40 pt against 10 pt lyrics, set on
            // the lyric line rather than spanning the staff.
            let initial_size = if lines == 1 { INITIAL_SCALE * size } else { cap / CAP_HEIGHT };
            let annotation_size = size * ANNOTATION_RATIO;
            let annotations = if style.annotation {
                initial::annotations(&self.header)
                    .into_iter()
                    .map(|a| {
                        let w = measure.advance(&a, TextStyle::REGULAR) * annotation_size;
                        (a, w)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let advance_em = measure.advance(&text, TextStyle::REGULAR);
            // An accent on the capital rises above its cap height; leave room for it.
            let accented = !text.is_ascii();
            initial = Some(InitialBox {
                width: advance_em * initial_size,
                advance_em,
                // Standing on the lyric line, a one-staff initial's accent stays below the staff's top.
                accent_room: if accented && lines > 1 { 0.25 * initial_size } else { 0.0 },
                descent,
                text,
                syllable: si as u32,
                size: initial_size,
                lines,
                annotations,
                annotation_size,
                annotation_ascent: ascent * annotation_size,
            });
            first_lyric = Some((si, rest));
        }
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
            let pauses_before = e.pauses.len();

            let mut open = Open::new();
            let mut run = Run {
                notes: Vec::new(),
                ids: Vec::new(),
            };
            let mut first_seg = true;
            let mut space_before = 0.0;
            let mut seg_ids: Vec<usize> = Vec::new();
            let only_clef = syl.text.is_empty() && syl.notation.iter().all(|f| matches!(f, Figure::Clef(_) | Figure::Space(_)));
            let total_notes = syl.notation.iter().filter(|f| matches!(f, Figure::Note(_))).count();
            let glued =
                syl.no_break_within || syl.no_break_before || self.syllables.get(si as usize + 1).is_some_and(|s| s.no_break_before);
            let mut notes_before = 0usize;
            // A break between note groups is allowed only inside a long melisma, away from its ends.
            let melisma_break = |notes_before: usize| {
                !glued
                    && total_notes >= MELISMA_NOTES
                    && notes_before >= MELISMA_END_NOTES
                    && total_notes - notes_before >= MELISMA_END_NOTES
            };

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
                        notes_before += 1;
                    }
                    Figure::Space(s) => {
                        e.flush(&mut run, &mut open, si);
                        let cut = match *s {
                            Space::Small | Space::Medium | Space::Half => true,
                            Space::Scaled(f) => f > 0.0,
                            _ => false,
                        };
                        if !open.empty && (*s == Space::Large || cut && melisma_break(notes_before)) {
                            let o = std::mem::replace(&mut open, Open::new());
                            if let Some(k) = e.close(o, si, first_seg, syl.word_start, space_before) {
                                if *s != Space::Large {
                                    e.segments[k].after = Break::InMelisma;
                                }
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
                        e.pauses.push((e.notes.len() as u32, PauseKind::Bar(b.kind)));
                        // The bar's ink is in the open segment, which closes next.
                        e.pause_segments.push(e.segments.len());
                        if melisma_break(notes_before) {
                            let o = std::mem::replace(&mut open, Open::new());
                            if let Some(k) = e.close(o, si, first_seg, syl.word_start, space_before) {
                                e.segments[k].after = Break::InMelisma;
                                seg_ids.push(k);
                                first_seg = false;
                            }
                            space_before = SYLLABLE_GAP;
                        }
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
            // The psalm marks pause after this syllable's notes, ahead of a bar written after
            // them, since the text marks the end of the half-verse.
            let end = e.notes.len() as u32;
            let mut at = e.pauses.iter().rposition(|p| p.0 < end).map_or(0, |i| i + 1).max(pauses_before);
            for c in syllable_text.last().map_or("", String::as_str).chars() {
                let kind = match c {
                    '*' => PauseKind::Mediant,
                    '†' => PauseKind::Flex,
                    _ => continue,
                };
                e.pauses.insert(at, (end, kind));
                e.pause_segments.insert(at, usize::MAX);
                at += 1;
            }
            e.flush(&mut run, &mut open, si);
            if only_clef && e.initial_clef.as_ref().is_some_and(|c| *c == e.clef) && e.segments.is_empty() {
                continue;
            }
            if let Some(k) = e.close(open, si, first_seg, syl.word_start, space_before) {
                seg_ids.push(k);
            }
            // A psalm mark belongs to its syllable's last segment.
            let last_seg = e.segments.len().saturating_sub(1);
            for s in e.pause_segments.iter_mut().skip(pauses_before).filter(|s| **s == usize::MAX) {
                *s = last_seg;
            }
            if seg_ids.is_empty() {
                continue;
            }
            // A break written in its own syllable applies after the previous segment.
            if let Some(brk) = pending_break.take() {
                let k = seg_ids[0];
                // A syllable with text keeps its segment, and the break follows it.
                let break_only = syl.text.is_empty()
                    && syl
                        .notation
                        .iter()
                        .all(|f| matches!(f, Figure::Break(_) | Figure::Space(_) | Figure::NoCustos));
                if break_only && k > 0 {
                    e.segments[k - 1].after = brk;
                    e.segments.truncate(k);
                    continue;
                }
                e.segments[*seg_ids.last().unwrap_or(&k)].after = brk;
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
            let text = match &first_lyric {
                Some((i, rest)) if *i == si as usize => rest,
                _ => &syl.text,
            };
            if !text.is_empty() {
                let k = seg_ids[0];
                let runs = text.runs.clone();
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
                let chars: Vec<char> = text.plain().chars().collect();
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
                let nucleus = text.center.clone().or_else(|| e.rules.nucleus(&masked));
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
                    hyphenated: text.plain().ends_with(['-', '\u{2010}']),
                    lead_hyphen: false,
                    syllable: si,
                });
            } else if first_lyric.as_ref().is_some_and(|(i, _)| *i == si as usize)
                && self.syllables.get(si as usize + 1).is_some_and(|s| !s.word_start)
            {
                // The initial took the whole first syllable of a longer word (`E(f)o(g)dem`):
                // GregorioTeX sets a hyphen under its notes, so the line doesn't seem to start
                // a new word.
                let k = seg_ids[0];
                let width = hyphen;
                let seg = &e.segments[k];
                let anchor = match seg.heads.first() {
                    Some(h) => h.x,
                    None => seg.ink.map_or(0.0, |(l, r)| (l + r) / 2.0),
                };
                e.segments[k].lyric = Some(LyricBox {
                    runs: vec![LyricRun {
                        text: "-".into(),
                        style: TextStyle::REGULAR,
                        consonant: true,
                    }],
                    left: anchor - width / 2.0,
                    width,
                    word_end: false,
                    hyphenated: true,
                    lead_hyphen: true,
                    syllable: si,
                });
            }
        }
        // A score of only a clef still draws its staff and clef, as Gregorio does.
        if e.segments.is_empty() && e.initial_clef.is_some() {
            let last = self.syllables.len().saturating_sub(1) as u32;
            e.close(Open::new(), last, true, true, 0.0);
        }
        if let Some(brk) = pending_break
            && let Some(last) = e.segments.last_mut()
        {
            let _ = brk;
            last.after = Break::Allowed;
            e.sink
                .info(0..0, "engrave::final-break", "a line break at the end of the score is dropped");
        }

        let lowest = e.notes.iter().map(|n| n.position).min().unwrap_or(0);
        Engraving {
            segments: e.segments,
            initial,
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
            pause_segments: e.pause_segments,
            custos_never: style.custos == CustosPolicy::Never,
            lowest,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The staff positions a bar spans, top first.
    fn span(kind: BarKind) -> (f32, f32) {
        let (pieces, _) = bar_pieces(kind, false, 0.0);
        let (top, bottom) = pieces[0].y_extent();
        (-top, -bottom)
    }

    #[test]
    fn a_double_bar_is_two_thin_bars() {
        let (pieces, w) = bar_pieces(BarKind::Finalis, false, 0.0);
        let rects: Vec<(f32, f32)> = pieces
            .iter()
            .filter_map(|p| match p.mark {
                Mark::Rect { x, w, .. } => Some((x, w)),
                _ => None,
            })
            .collect();
        assert_eq!(rects, [(0.0, STEM), (FINALIS_SEP, STEM)]);
        assert_eq!(w, FINALIS_SEP + STEM);
        for p in &pieces {
            assert_eq!(p.y_extent(), (-3.0, 3.0));
        }
    }

    #[test]
    fn dominican_bars_match_gregoriotex() {
        // Measured from GregorioTeX's output on a four-line staff (lines at -3, -1, 1, 3).
        assert_eq!(span(BarKind::Dominican(1)), (0.0, -3.0));
        assert_eq!(span(BarKind::Dominican(2)), (-1.0, -4.0));
        assert_eq!(span(BarKind::Dominican(3)), (2.0, -1.0));
        assert_eq!(span(BarKind::Dominican(4)), (1.0, -2.0));
        assert_eq!(span(BarKind::Dominican(5)), (4.0, 1.0));
        assert_eq!(span(BarKind::Dominican(6)), (3.0, 0.0));
    }
}
