//! Engraving: turns a [`Score`] into width-independent segments. Each segment is a stretch of
//! notation (and the lyric under it) with geometry relative to its own origin, plus what may
//! happen at the break after it. Line breaking places segments on lines (`layout`).

mod cache;
mod initial;
mod lyric_top;
pub(crate) mod neume;

pub(crate) use cache::EngraveCache;
pub use initial::Initial;
pub(crate) use initial::{CAP_HEIGHT, strip_tex};
pub(crate) use lyric_top::HYPHEN_TOP;

use crate::diag::{Diagnostic, Sink};
use crate::glyphs::GlyphId as G;
use crate::notes::PauseKind;
use crate::score::{
    AlterationKind, BarKind, Clef, ClefKind, CustosRule, Figure, Lyric, LyricRun, Note, NoteShape, Score, Space, StaffPosition, TextStyle,
};
use crate::text::TextMeasure;
use crate::vowel::VowelRules;
use neume::INTRA;
pub(crate) use neume::{LEDGER_GAP, STEM};
use std::sync::Arc;

/// What a piece of ink is, so themes can color staff, notes and rubrics separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ink {
    /// A staff line.
    Staff,
    /// A ledger line above or below the staff.
    Ledger,
    /// A notehead, or a neume glyph that draws several notes.
    Note,
    /// A stem or a line joining two notes.
    Stem,
    /// A bar (divisio).
    Bar,
    /// A horizontal or vertical episema.
    Episema,
    /// A mora (dot).
    Mora,
    /// An ictus mark.
    Ictus,
    /// A flat, natural or sharp.
    Accidental,
    /// A clef, and a `cb` clef's flat.
    Clef,
    /// A custos at the end of a line.
    Custos,
}

impl Ink {
    /// The CSS class the SVG writer uses for this role.
    #[must_use]
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

    /// The ink's box: left, top, right, bottom.
    pub(crate) fn ink_box(&self) -> [f32; 4] {
        match self.mark {
            Mark::Glyph { glyph, x, y } => {
                let (a, b, c, d) = glyph.ink();
                [x + a, y + b, x + c, y + d]
            }
            Mark::Rect { x, y, w, h } => [x, y, x + w, y + h],
        }
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

/// How long an alteration lasts (docs/DESIGN.md, "Alteration scope").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AlterationScope {
    /// Until the next clef or written line break. Engraving doesn't know where layout will
    /// break lines, so an alteration carries past a line break that layout chose.
    Line,
    /// Until the end of the word.
    Word,
    /// Until the next bar.
    Bar,
    /// Until the end of the word or the next bar, whichever comes first, as in the Solesmes
    /// books; the default.
    #[default]
    WordOrBar,
    /// For its own note only.
    Note,
}

/// Whether lines end with a custos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CustosPolicy {
    /// Where the score and GregorioTeX's defaults put one.
    #[default]
    Auto,
    /// Never, whatever the score asks for.
    Never,
}

/// How to engrave a score: everything that doesn't depend on the width. Build it with the
/// `with_*` setters: `StyleOptions::default().with_initial(Initial::Lines(2))`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
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
    /// How long a flat or natural lasts.
    pub alterations: AlterationScope,
    /// Whether lines end with a custos.
    pub custos: CustosPolicy,
}

crate::setters!(StyleOptions {
    lyric_size: f32 => with_lyric_size,
    initial: Initial => with_initial,
    annotation: bool => with_annotation,
    vowels: Option<VowelRules> => with_vowels,
    alterations: AlterationScope => with_alterations,
    custos: CustosPolicy => with_custos,
});

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
    /// How far the text's ink runs past its advance on the left and on the right (the hook
    /// of an `f`): syllables of a word may touch by their advances, but a hyphen, a word
    /// space or the line's end keeps clear of the ink.
    pub lead: f32,
    pub tail: f32,
    /// The syllable ends its word, so no hyphen follows it.
    pub word_end: bool,
    /// The text ends with a hyphen of its own (`Giê-(f)su(g)`), so none is added after it.
    pub hyphenated: bool,
    /// Not the syllable's text but the hyphen GregorioTeX sets under a first syllable the
    /// initial took whole.
    pub lead_hyphen: bool,
    /// How high the letters reach: `(left, right, top)` in staff spaces, from the text's left
    /// edge and above its baseline (see `lyric_top::profile`).
    pub tops: Vec<(f32, f32, f32)>,
}

impl LyricBox {
    /// The left edge of the text's ink.
    pub fn ink_left(&self) -> f32 {
        self.left - self.lead
    }

    /// The right edge of the text's ink.
    pub fn ink_right(&self) -> f32 {
        self.left + self.width + self.tail
    }
}

/// A notehead in a segment, for the note map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeadBox {
    pub note: u32,
    /// The notehead's center and size.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The box hit testing uses (left, top, right, bottom), in the same coordinates.
    pub hit: [f32; 4],
}

/// A bar in a segment, for the source map: its score-wide index and ink box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BarBox {
    pub bar: u32,
    pub x: f32,
    pub w: f32,
    pub top: f32,
    pub bottom: f32,
}

/// A stretch of notation and the lyric under it, as the line breaker places it. What doesn't
/// move with the segment's place in the score (its ink, measured from its own origin, and its
/// text) is shared behind an [`Arc`], so engravings that differ only elsewhere share it: an
/// edit copies the segments it touches, and moves the rest by their numbers here.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Segment {
    pub syllable: u32,
    /// What the note numbers in the ink count from: a piece's or head's note is this plus
    /// its own (see [`Segment::note`]).
    pub note_base: u32,
    /// What the bar numbers in the ink count from.
    pub bar_base: u32,
    pub after: Break,
    /// The clef in force after this segment.
    pub clef: Clef,
    pub suppress_custos: bool,
    pub body: Arc<SegmentInk>,
}

/// The part of a [`Segment`] that is the same wherever it stands in the score: its ink and
/// text. Note and bar
/// numbers count from the segment's `note_base` and `bar_base`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SegmentInk {
    /// The first segment of its syllable (only it carries the lyric).
    pub first: bool,
    pub word_start: bool,
    pub pieces: Vec<Piece>,
    pub heads: Vec<HeadBox>,
    pub bars: Vec<BarBox>,
    /// Horizontal extent of the notation, if there is any.
    pub ink: Option<(f32, f32)>,
    /// The extent notes are spaced by: the ink without its ledger lines, which may reach
    /// toward the next notes' as GregorioTeX's do. None for a segment with a bar, a clef or a
    /// custos, which keeps clear of ledger lines too.
    pub spacing: Option<(f32, f32)>,
    pub lyric: Option<LyricBox>,
    /// Space written before this segment inside its syllable (for segments after the first).
    pub space_before: f32,
    pub starts_with_clef: bool,
    /// Position of the first note, for the custos that announces this segment.
    pub first_note: Option<StaffPosition>,
}

impl std::ops::Deref for Segment {
    type Target = SegmentInk;

    fn deref(&self) -> &SegmentInk {
        &self.body
    }
}

impl Segment {
    /// The body, to change: copied first if another engraving shares it.
    pub(crate) fn body_mut(&mut self) -> &mut SegmentInk {
        Arc::make_mut(&mut self.body)
    }

    /// A note number of the ink, score-wide.
    pub(crate) fn note(&self, n: u32) -> u32 {
        self.note_base + n
    }

    /// A bar number of the ink, score-wide.
    pub(crate) fn bar(&self, b: u32) -> u32 {
        self.bar_base + b
    }
}

impl SegmentInk {
    /// A bar standing in a syllable of its own, its text (`*(;)`) or none.
    pub(crate) fn is_bar(&self) -> bool {
        !self.pieces.is_empty() && self.pieces.iter().all(|p| p.role == Ink::Bar)
    }

    pub(crate) fn right(&self) -> f32 {
        let r = self.ink.map_or(0.0, |(_, r)| r);
        match &self.lyric {
            Some(t) => r.max(t.ink_right()),
            None => r,
        }
    }

    /// Whether any piece or head belongs to a note.
    pub(crate) fn has_notes(&self) -> bool {
        !self.heads.is_empty() || self.pieces.iter().any(|p| p.note.is_some())
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
    /// The initial's width in ems: its advance, and its ink beyond it (see `initial::overhang`).
    pub advance_em: f32,
    /// How far its ink reaches left of where it is set, and below its baseline, in ems.
    pub lead_em: f32,
    pub depth_em: f32,
    /// How far the capital's ink rises above its cap height, and so above the first staff's
    /// top line (an accent, an ascender, the apex of an A), in staff spaces.
    pub above: f32,
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
    pub(crate) syllable_text: Vec<Arc<str>>,
    pub(crate) syllable_word: Vec<u32>,
    /// Each syllable's source span, and each bar's, in source order.
    pub(crate) syllable_spans: Vec<std::ops::Range<usize>>,
    pub(crate) bar_spans: Vec<std::ops::Range<usize>>,
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
    /// The first note after each segment, for its custos (so layout needn't scan ahead).
    pub(crate) next_note: Vec<Option<StaffPosition>>,
    /// The segments from here on have no ink, only text, so a line of them alone draws no
    /// staff; the segments' count when every segment has ink or none has.
    pub(crate) inkless_from: usize,
    /// What engraving found: the `engrave::` and `text::` codes in docs/diagnostics.md.
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
/// A staff's height from its top line to its bottom line, in staff spaces.
const STAFF_HEIGHT: f32 = 6.0;
/// Annotation size relative to the lyrics.
const ANNOTATION_RATIO: f32 = 0.75;
const DEFAULT_CLEF: Clef = Clef {
    kind: ClefKind::Do,
    line: 4,
    flat: false,
    span: 0..0,
};

/// `/` in the notes: GregorioTeX's `interelementspace` (0.069 cm) on its default staff, whose
/// interline is 0.288 cm.
const SMALL_SPACE: f32 = 0.48;
/// `//` in the notes: GregorioTeX's `largerspace` (0.109 cm).
const MEDIUM_SPACE: f32 = 0.76;
/// A space in the notes: GregorioTeX's `glyphspace` (0.219 cm).
const LARGE_SPACE: f32 = 1.52;

/// The spaces written inside notes.
fn space_width(s: Space) -> f32 {
    match s {
        Space::Zero => 0.0,
        Space::Tiny => INTRA * 0.5,
        Space::Half => INTRA * 0.5,
        Space::Small => SMALL_SPACE,
        Space::Medium => MEDIUM_SPACE,
        Space::Large | Space::LargeNoBreak => LARGE_SPACE,
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

/// How much wider than the regular face a style without a face of its own is measured: a
/// synthesized bold is about this much wider.
const SYNTHETIC_WIDENING: f32 = 0.03;

/// The width to add to run `r` measured with the regular face, where the measure has no face
/// for its style; warns of it once.
#[cold]
fn synthetic_face(
    measure: &dyn TextMeasure,
    r: &LyricRun,
    size: f32,
    sink: &mut Sink,
    warned: &mut bool,
    syl: &crate::score::Syllable,
) -> f32 {
    if !*warned {
        sink.info(
            syl.span.clone(),
            "text::synthetic-face",
            "no face for this style; measured with the regular face widened 3%",
        );
        *warned = true;
    }
    measure.advance(&r.text, TextStyle::REGULAR) * size * SYNTHETIC_WIDENING
}

/// The gap between a `cb` clef and its key flat, in staff spaces.
const KEY_FLAT_GAP: f32 = 0.2;

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
        let (piece, w) = ink_at(G::Flat, right + KEY_FLAT_GAP, -(b as f32), Ink::Clef, None);
        out.push(piece);
        right += KEY_FLAT_GAP + w;
    }
    (out, right)
}

/// How wide a clef is drawn, its key flat included: [`clef_pieces`]'s width, without the
/// pieces.
pub(crate) fn clef_width(clef: &Clef) -> f32 {
    let ink = |g: G| {
        let (a, _, c, _) = g.ink();
        c - a
    };
    let w = ink(if clef.kind == ClefKind::Do { G::DoClef } else { G::FaClef });
    if clef.flat { w + (KEY_FLAT_GAP + ink(G::Flat)) } else { w }
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

/// The distance between the centres of the two bars of a `::`: GregorioTeX's
/// `divisiofinalissep` (0.109 cm, 0.76 staff spaces) between them, plus a bar's width.
const FINALIS_SEP: f32 = 0.76 + STEM;

/// The dotted full bar `:?`: a dash this long at every staff position from the top line down.
const DOTTED_BAR_DASH: f32 = 0.6;

fn bar_pieces(kind: BarKind, high: bool, left: f32) -> (Vec<Piece>, f32) {
    let shift = if high { -2 } else { 0 };
    let bar = |top: StaffPosition, bottom: StaffPosition| rect(left, top + shift, bottom + shift, Ink::Bar);
    match kind {
        BarKind::Virgula => {
            let (p, w) = ink_at(G::Virgula, left, -(3 + shift) as f32, Ink::Bar, None);
            (vec![p], w)
        }
        BarKind::Minimis => (vec![bar(4, 3)], STEM),
        BarKind::Quarter => (vec![bar(4, 2)], STEM),
        BarKind::Half => (vec![bar(2, -2)], STEM),
        BarKind::Full => (vec![bar(3, -3)], STEM),
        BarKind::DottedFull => {
            let mut out = Vec::new();
            let mut y = -3.0;
            while y < 3.0 {
                out.push(Piece {
                    mark: Mark::Rect {
                        x: left,
                        y,
                        w: STEM,
                        h: DOTTED_BAR_DASH,
                    },
                    role: Ink::Bar,
                    note: None,
                    through: None,
                });
                y += 1.0;
            }
            (out, STEM)
        }
        BarKind::Double => {
            // Two thin bars, as GregorioTeX draws `::`.
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
    bars: Vec<BarBox>,
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
            bars: Vec::new(),
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
                let (cx, y, w, height) = h.hit_box();
                let [l, t, r, b] = h.hit_rect();
                open.heads.push(HeadBox {
                    note: base + h.index as u32,
                    x: cx + x,
                    y,
                    w,
                    h: height,
                    hit: [l + x, t, r + x, b],
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
        let mut open = open;
        neume::clear_ledgers(&mut open.pieces);
        let ink = neume::extent(&open.pieces);
        let walled = open.pieces.iter().any(|p| matches!(p.role, Ink::Bar | Ink::Clef | Ink::Custos));
        let spacing = if walled {
            None
        } else {
            neume::extent(open.pieces.iter().filter(|p| p.role != Ink::Ledger))
        };
        // Numbers count from the segment's first note and bar, so the body reads the same
        // wherever the segment stands.
        let note_base = open
            .pieces
            .iter()
            .flat_map(|p| p.note)
            .chain(open.heads.iter().map(|h| h.note))
            .min()
            .unwrap_or(0);
        let bar_base = open.bars.iter().map(|b| b.bar).min().unwrap_or(0);
        for p in &mut open.pieces {
            p.note = p.note.map(|n| n - note_base);
            p.through = p.through.map(|n| n - note_base);
        }
        for h in &mut open.heads {
            h.note -= note_base;
        }
        for b in &mut open.bars {
            b.bar -= bar_base;
        }
        // The score's opening clef is drawn at the start of the first line even when a clef
        // change follows it at once, as in `(c4) (c3)`: Gregorio shows both.
        let starts_with_clef = open.starts_with_clef && !(self.segments.is_empty() && self.initial_clef.is_some());
        self.segments.push(Segment {
            syllable,
            note_base,
            bar_base,
            after: Break::Allowed,
            clef: self.clef.clone(),
            suppress_custos: false,
            body: Arc::new(SegmentInk {
                first,
                word_start,
                pieces: open.pieces,
                heads: open.heads,
                bars: open.bars,
                ink,
                spacing,
                lyric: None,
                space_before,
                starts_with_clef,
                first_note: open.first_note,
            }),
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
    /// Engraves the score: neumes, signs and lyric boxes, independent of width. The
    /// engraving comes shared, as the layouts made from it share it.
    #[must_use]
    pub fn engrave(&self, measure: &dyn TextMeasure, style: &StyleOptions) -> Arc<Engraving> {
        let mut pass = self.pass(measure, style, false);
        for (si, syl) in self.syllables.iter().enumerate() {
            pass.syllable(self, si, syl);
        }
        Arc::new(pass.finish(self).0)
    }

    /// Sets up engraving: the vowel rules, the text metrics and the initial.
    fn pass<'a>(&self, measure: &'a dyn TextMeasure, style: &'a StyleOptions, marks: bool) -> Pass<'a> {
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
                self.header.span("language").unwrap_or(0..0),
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
            // the staves it actually spans. A one-staff initial runs from the staff's top line
            // down to the lyric baseline, as in Solesmes books, so the lyrics' drop (more for
            // a score that goes below the staff) counts too.
            let line_pitch = crate::layout::BASELINE_PITCH;
            let lines = n.min(initial::MAX_LINES) as usize;
            let cap = if lines == 1 {
                let lowest = e.note_positions.iter().copied().min().unwrap_or(0);
                STAFF_HEIGHT + crate::layout::text_drop(lowest)
            } else {
                STAFF_HEIGHT + line_pitch * (lines - 1) as f32
            };
            let initial_size = cap / CAP_HEIGHT;
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
            // The column holds the capital's ink, which for a few letters runs past its advance.
            let (lead, tail) = initial::overhang(&text);
            let advance_em = measure.advance(&text, TextStyle::REGULAR) + lead + tail;
            initial = Some(InitialBox {
                width: advance_em * initial_size,
                advance_em,
                lead_em: lead,
                depth_em: initial::depth(&text),
                above: initial::rise(&text) * initial_size,
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
        Pass {
            e,
            measure,
            size,
            hyphen,
            word_space,
            ascent,
            descent,
            initial,
            first_lyric,
            syllable_text: Vec::new(),
            syllable_spans: Vec::with_capacity(self.syllables.len()),
            bar_spans: Vec::new(),
            syllable_word: Vec::new(),
            alt_text: String::new(),
            word: 0,
            warned_face: false,
            pending_break: None,
            nocustos: false,
            marks: marks.then(|| Vec::with_capacity(self.syllables.len() + 1)),
        }
    }
}

/// Engraving under way: the engraver and what it has carried from one syllable to the next.
struct Pass<'a> {
    e: Engraver<'a>,
    measure: &'a dyn TextMeasure,
    size: f32,
    hyphen: f32,
    word_space: f32,
    ascent: f32,
    descent: f32,
    initial: Option<InitialBox>,
    /// The syllable the initial came from, and the rest of its text.
    first_lyric: Option<(usize, Lyric)>,
    syllable_text: Vec<Arc<str>>,
    syllable_spans: Vec<std::ops::Range<usize>>,
    bar_spans: Vec<std::ops::Range<usize>>,
    syllable_word: Vec<u32>,
    alt_text: String,
    word: u32,
    warned_face: bool,
    /// A break written in a syllable of its own, for the next segment.
    pending_break: Option<Break>,
    nocustos: bool,
    /// The state before each syllable and after the last, when kept for an [`EngraveCache`].
    marks: Option<Vec<Resume>>,
}

/// What engraving carries into a syllable, and how much it has made by then. A syllable
/// engraves the same from the same mark.
#[derive(Clone, Debug, PartialEq)]
struct Resume {
    segments: usize,
    notes: usize,
    pauses: usize,
    bars: usize,
    diagnostics: usize,
    alt_text: usize,
    word: u32,
    alt_empty: bool,
    clef: Clef,
    initial_clef: Option<Clef>,
    alteration: Vec<(StaffPosition, i8)>,
    pending_break: Option<Break>,
    nocustos: bool,
    warned_face: bool,
    /// What may happen after the last segment so far, which a later syllable may change.
    last_after: Option<Break>,
}

impl Pass<'_> {
    fn mark(&self) -> Resume {
        Resume {
            segments: self.e.segments.len(),
            notes: self.e.notes.len(),
            pauses: self.e.pauses.len(),
            bars: self.bar_spans.len(),
            diagnostics: self.e.sink.items.len(),
            alt_text: self.alt_text.len(),
            word: self.word,
            alt_empty: self.alt_text.is_empty(),
            clef: self.e.clef.clone(),
            initial_clef: self.e.initial_clef.clone(),
            alteration: self.e.alteration.clone(),
            pending_break: self.pending_break,
            nocustos: self.nocustos,
            warned_face: self.warned_face,
            last_after: self.e.segments.last().map(|s| s.after),
        }
    }

    /// Engraves syllable `si` of `score`.
    fn syllable(&mut self, score: &Score, si: usize, syl: &crate::score::Syllable) {
        let mark = self.marks.is_some().then(|| self.mark());
        if let (Some(marks), Some(m)) = (&mut self.marks, mark) {
            marks.push(m);
        }
        let si = si as u32;
        if syl.word_start && si > 0 {
            self.word += 1;
        }
        self.syllable_word.push(self.word);
        let plain = syl.text.plain();
        if !plain.is_empty() {
            if syl.word_start && !self.alt_text.is_empty() {
                self.alt_text.push(' ');
            }
            self.alt_text.push_str(&plain);
        }
        self.syllable_text.push(plain.into());
        self.syllable_spans.push(syl.span.clone());
        self.e.reset_alterations(syl.word_start, false);
        let pauses_before = self.e.pauses.len();
        // This syllable's notes are the ones pushed from here on.
        let notes_from = self.e.notes.len();

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
        let glued = syl.no_break_within || syl.no_break_before || score.syllables.get(si as usize + 1).is_some_and(|s| s.no_break_before);
        let mut notes_before = 0usize;
        // A break between note groups is allowed only inside a long melisma, away from its ends.
        let melisma_break = |notes_before: usize| {
            !glued && total_notes >= MELISMA_NOTES && notes_before >= MELISMA_END_NOTES && total_notes - notes_before >= MELISMA_END_NOTES
        };

        for f in &syl.notation {
            match f {
                Figure::Note(n) => {
                    let id = self.e.notes.len() as u32;
                    let alteration = if self.e.style.alterations == AlterationScope::Note {
                        0
                    } else {
                        self.e.alteration_of(n.position, &self.e.clef.clone())
                    };
                    self.e.notes.push(NoteInfo {
                        syllable: si,
                        position: n.position,
                        shape: n.shape,
                        liquescent: n.liquescent != crate::score::Liquescent::None,
                        morae: n.morae,
                        episema: n.episema.is_some(),
                        span: n.span.clone(),
                        clef: self.e.clef.clone(),
                        alteration,
                        vowel: None,
                    });
                    if self.e.style.alterations == AlterationScope::Note {
                        self.e.alteration.clear();
                    }
                    run.notes.push(n.clone());
                    run.ids.push(id);
                    notes_before += 1;
                }
                Figure::Space(s) => {
                    self.e.flush(&mut run, &mut open, si);
                    let cut = match *s {
                        Space::Small | Space::Medium | Space::Half => true,
                        Space::Scaled(f) => f > 0.0,
                        _ => false,
                    };
                    if !open.empty && (*s == Space::Large || cut && melisma_break(notes_before)) {
                        let o = std::mem::replace(&mut open, Open::new());
                        if let Some(k) = self.e.close(o, si, first_seg, syl.word_start, space_before) {
                            if *s != Space::Large {
                                self.e.segments[k].after = Break::InMelisma;
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
                    self.e.flush(&mut run, &mut open, si);
                    self.e.alteration.clear();
                    self.e.clef = c.clone();
                    if self.e.initial_clef.is_none() && self.e.segments.is_empty() && only_clef {
                        self.e.initial_clef = Some(c.clone());
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
                    self.e.flush(&mut run, &mut open, si);
                    let delta = match a.kind {
                        AlterationKind::Flat => -1,
                        AlterationKind::Natural => 0,
                        AlterationKind::Sharp => 1,
                    };
                    let in_force = self.e.alteration_of(a.position, &self.e.clef.clone());
                    self.e.alteration.push((a.position, delta));
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
                    self.e.flush(&mut run, &mut open, si);
                    self.e.reset_alterations(false, true);
                    let x = open.advance(SYLLABLE_GAP);
                    let (pieces, w) = bar_pieces(b.kind, b.high, x);
                    let (top, bottom) = pieces
                        .iter()
                        .map(Piece::y_extent)
                        .fold((f32::MAX, f32::MIN), |(t, b), (pt, pb)| (t.min(pt), b.max(pb)));
                    open.bars.push(BarBox {
                        bar: self.bar_spans.len() as u32,
                        x,
                        w,
                        top,
                        bottom,
                    });
                    self.bar_spans.push(b.span.clone());
                    open.pieces.extend(pieces);
                    open.x = x + w;
                    self.e.pauses.push((self.e.notes.len() as u32, PauseKind::Bar(b.kind)));
                    // The bar's ink is in the open segment, which closes next.
                    self.e.pause_segments.push(self.e.segments.len());
                    if melisma_break(notes_before) {
                        let o = std::mem::replace(&mut open, Open::new());
                        if let Some(k) = self.e.close(o, si, first_seg, syl.word_start, space_before) {
                            self.e.segments[k].after = Break::InMelisma;
                            seg_ids.push(k);
                            first_seg = false;
                        }
                        space_before = SYLLABLE_GAP;
                    }
                }
                Figure::Custos { position, .. } => {
                    self.e.flush(&mut run, &mut open, si);
                    let pos = position.or_else(|| self.e.note_positions.get(self.e.notes.len()).copied());
                    if let Some(p) = pos {
                        let x = open.advance(SYLLABLE_GAP);
                        let (piece, w) = custos_piece(p, x);
                        open.pieces.push(piece);
                        open.x = x + w;
                    }
                }
                Figure::Break(b) => {
                    self.e.flush(&mut run, &mut open, si);
                    let brk = Break::Forced {
                        justify: b.justify,
                        custos: b.custos,
                    };
                    if open.empty {
                        self.pending_break = Some(brk);
                    } else {
                        let o = std::mem::replace(&mut open, Open::new());
                        if let Some(k) = self.e.close(o, si, first_seg, syl.word_start, space_before) {
                            self.e.segments[k].after = brk;
                            seg_ids.push(k);
                            first_seg = false;
                        }
                        space_before = 0.0;
                    }
                }
                Figure::NoCustos => self.nocustos = true,
            }
        }
        // The psalm marks pause after this syllable's notes, ahead of a bar written after
        // them, since the text marks the end of the half-verse.
        let end = self.e.notes.len() as u32;
        let mut at = self
            .e
            .pauses
            .iter()
            .rposition(|p| p.0 < end)
            .map_or(0, |i| i + 1)
            .max(pauses_before);
        for c in self.syllable_text.last().map_or("", |t| &**t).chars() {
            let kind = match c {
                '*' => PauseKind::Mediant,
                '†' => PauseKind::Flex,
                _ => continue,
            };
            self.e.pauses.insert(at, (end, kind));
            self.e.pause_segments.insert(at, usize::MAX);
            at += 1;
        }
        self.e.flush(&mut run, &mut open, si);
        if only_clef && self.e.initial_clef.as_ref().is_some_and(|c| *c == self.e.clef) && self.e.segments.is_empty() {
            return;
        }
        if let Some(k) = self.e.close(open, si, first_seg, syl.word_start, space_before) {
            seg_ids.push(k);
        }
        // A psalm mark belongs to its syllable's last segment.
        let last_seg = self.e.segments.len().saturating_sub(1);
        for s in self.e.pause_segments.iter_mut().skip(pauses_before).filter(|s| **s == usize::MAX) {
            *s = last_seg;
        }
        if seg_ids.is_empty() {
            return;
        }
        // A break written in its own syllable applies after the previous segment.
        if let Some(brk) = self.pending_break.take() {
            let k = seg_ids[0];
            // A syllable with text keeps its segment, and the break follows it.
            let break_only = syl.text.is_empty()
                && syl
                    .notation
                    .iter()
                    .all(|f| matches!(f, Figure::Break(_) | Figure::Space(_) | Figure::NoCustos));
            if break_only && k > 0 {
                self.e.segments[k - 1].after = brk;
                self.e.segments.truncate(k);
                return;
            }
            self.e.segments[*seg_ids.last().unwrap_or(&k)].after = brk;
        }
        if self.nocustos {
            if let Some(k) = seg_ids.last() {
                self.e.segments[*k].suppress_custos = true;
            }
            self.nocustos = false;
        }
        if syl.no_break_before {
            let k = seg_ids[0];
            if k > 0 && !matches!(self.e.segments[k - 1].after, Break::Forced { .. }) {
                self.e.segments[k - 1].after = Break::Forbidden;
            }
        }

        // The lyric goes under the first segment, its vowel over the first note.
        self.lyric(score, si, syl, seg_ids[0], notes_from);
    }

    /// Sets a syllable's lyric under segment `k`, its first, and gives its vowel to its notes
    /// (`notes_from` on). Kept apart from [`Self::syllable`], as is what it calls, so that the
    /// loop over the notation stays small.
    #[inline(never)]
    fn lyric(&mut self, score: &Score, si: u32, syl: &crate::score::Syllable, k: usize, notes_from: usize) {
        let text = match &self.first_lyric {
            Some((i, rest)) if *i == si as usize => rest,
            _ => &syl.text,
        };
        if !text.is_empty() {
            let runs = text.runs.clone();
            // Each run's advance up to the end of each of its characters, measured once for
            // the width, the vowel's place and the letters' tops.
            let mut advances = Vec::with_capacity(runs.iter().map(|r| r.text.len()).sum());
            let mut starts = Vec::with_capacity(runs.len() + 1);
            for r in &runs {
                starts.push(advances.len());
                self.measure.prefix_advances(&r.text, r.style, &mut advances);
            }
            starts.push(advances.len());
            // A run's advance: the last of its prefixes'.
            let full = |i: usize| match advances[starts[i]..starts[i + 1]].last() {
                Some(&a) => a,
                None => self.measure.advance(&runs[i].text, runs[i].style),
            };
            // The advance of the first `chars` characters of the runs, in ems.
            let prefix = |chars: usize| {
                let mut left = chars;
                let mut w = 0.0;
                for (i, r) in runs.iter().enumerate() {
                    let n = starts[i + 1] - starts[i];
                    if left >= n {
                        w += full(i);
                        left -= n;
                    } else {
                        w += match left {
                            0 => self.measure.advance("", r.style),
                            _ => advances[starts[i] + left - 1],
                        };
                        break;
                    }
                }
                w
            };
            let mut width = 0.0;
            for (i, r) in runs.iter().enumerate() {
                width += full(i) * self.size;
                if !self.measure.has_face(r.style) {
                    width += synthetic_face(self.measure, r, self.size, &mut self.e.sink, &mut self.warned_face, syl);
                }
            }
            // The text's characters, those of runs marked consonantal masked out for the
            // vowel's search.
            let mut masked: Vec<char> = Vec::with_capacity(advances.len());
            for r in &runs {
                masked.extend(r.text.chars().map(|c| if r.consonant { '\u{0}' } else { c }));
            }
            let nucleus = text.center.clone().or_else(|| self.e.rules.nucleus(&masked));
            let seg = &self.e.segments[k];
            let anchor = match seg.heads.first() {
                Some(h) => h.x,
                None => seg.ink.map_or(0.0, |(l, r)| (l + r) / 2.0),
            };
            let center = match &nucleus {
                Some(r) => {
                    let before = prefix(r.start) * self.size;
                    let upto = prefix(r.end) * self.size;
                    (before + upto) / 2.0
                }
                None => width / 2.0,
            };
            if let Some(r) = &nucleus
                && let Some(c) = runs.iter().flat_map(|r| r.text.chars()).nth(r.start)
            {
                for n in &mut self.e.notes[notes_from..] {
                    n.vowel = Some(c);
                }
            }
            let next_word = score.syllables.get(si as usize + 1).is_none_or(|s| s.word_start);
            let tops = lyric_top::profile(&runs, &advances, &starts, self.size);
            // The box holds the text's ink, which a letter at either end (the hook of an `f`)
            // can carry past its advance.
            let (lead, tail) = lyric_top::overhang(&runs, self.size);
            self.e.segments[k].body_mut().lyric = Some(LyricBox {
                tops,
                runs,
                lead,
                tail,
                left: anchor - center,
                width,
                word_end: next_word,
                hyphenated: text
                    .runs
                    .iter()
                    .rev()
                    .find_map(|r| r.text.chars().next_back())
                    .is_some_and(|c| matches!(c, '-' | '\u{2010}')),
                lead_hyphen: false,
            });
        } else if self.first_lyric.as_ref().is_some_and(|(i, _)| *i == si as usize)
            && score.syllables.get(si as usize + 1).is_some_and(|s| !s.word_start)
        {
            self.initial_hyphen(k);
        }
    }

    /// The initial took the whole first syllable of a longer word (`E(f)o(g)dem`): GregorioTeX
    /// sets a hyphen under its notes, so the line doesn't seem to start a new word.
    #[cold]
    fn initial_hyphen(&mut self, k: usize) {
        let width = self.hyphen;
        let seg = &self.e.segments[k];
        let anchor = match seg.heads.first() {
            Some(h) => h.x,
            None => seg.ink.map_or(0.0, |(l, r)| (l + r) / 2.0),
        };
        self.e.segments[k].body_mut().lyric = Some(LyricBox {
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
            lead: 0.0,
            tail: 0.0,
            tops: vec![(0.0, width, lyric_top::HYPHEN_TOP * self.size)],
        });
    }

    /// Ends the score, and puts the engraving together.
    fn finish(mut self, score: &Score) -> (Engraving, Option<Vec<Resume>>) {
        let mark = self.marks.is_some().then(|| self.mark());
        if let (Some(marks), Some(m)) = (&mut self.marks, mark) {
            marks.push(m);
        }
        let e = &mut self.e;
        // A score of only a clef still draws its staff and clef, as Gregorio does.
        if e.segments.is_empty() && e.initial_clef.is_some() {
            let last = score.syllables.len().saturating_sub(1) as u32;
            e.close(Open::new(), last, true, true, 0.0);
        }
        // A break pending here would have come from a syllable of its own at the end; but
        // that syllable's empty segment takes it (and is dropped), so none is left.
        debug_assert!(self.pending_break.is_none());

        // The initial is drawn on the first line, so its syllable must start there: a bar or
        // clef written before it (`(c4) (::) A(g)`) can't take a line of its own.
        if let Some(init) = &self.initial {
            for seg in e.segments.iter_mut().filter(|s| s.syllable < init.syllable) {
                if !matches!(seg.after, Break::Forced { .. }) {
                    seg.after = Break::Forbidden;
                }
            }
        }
        let e = self.e;
        let lowest = e.notes.iter().map(|n| n.position).min().unwrap_or(0);
        // Text after the last ink (a rubric after the final bar) draws no staff of its own.
        let inkless_from = match e.segments.iter().rposition(|s| !s.pieces.is_empty()) {
            Some(k) => k + 1,
            None => e.segments.len(),
        };
        let mut next_note = vec![None; e.segments.len()];
        for k in (0..e.segments.len().saturating_sub(1)).rev() {
            next_note[k] = e.segments[k + 1].first_note.or(next_note[k + 1]);
        }
        let engraving = Engraving {
            inkless_from,
            next_note,
            segments: e.segments,
            initial: self.initial,
            clef: e.initial_clef.unwrap_or(DEFAULT_CLEF),
            notes: e.notes,
            syllable_text: self.syllable_text,
            syllable_spans: self.syllable_spans,
            bar_spans: self.bar_spans,
            syllable_word: self.syllable_word,
            lyric_size: self.size,
            hyphen: self.hyphen,
            word_space: self.word_space,
            ascent: self.ascent,
            descent: self.descent,
            alt_text: self.alt_text,
            pauses: e.pauses,
            pause_segments: e.pause_segments,
            custos_never: e.style.custos == CustosPolicy::Never,
            lowest,
            diagnostics: e.sink.items,
        };
        (engraving, self.marks)
    }
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
        let (pieces, w) = bar_pieces(BarKind::Double, false, 0.0);
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
    fn next_note_looks_past_segments_without_notes() {
        let src = "(c4) A(g) b() c() (,) d(hi) e(j) (::) f() g()";
        let eng = crate::parse(src).score.engrave(&crate::ApproxMeasure, &StyleOptions::default());
        for k in 0..eng.segments.len() {
            let scanned = eng.segments[k + 1..].iter().find_map(|s| s.first_note);
            assert_eq!(eng.next_note[k], scanned, "segment {k}");
        }
        assert!(eng.next_note.iter().any(Option::is_some) && eng.next_note.last() == Some(&None));
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
