//! Line breaking and justification: the only width-dependent stage.
//!
//! Segments are packed left to right with minimum gaps (notation to notation, lyric to lyric),
//! and an optimal-fit breaker picks the breaks with the least total demerits. Arithmetic is
//! limited to add, subtract, multiply, divide and comparison (DESIGN section 13).

use crate::engrave::{Break, CAP_HEIGHT, Engraving, HYPHEN_TOP, Ink, Mark, Piece, Segment, clef_pieces, custos_piece};
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
    /// Keep only the first this many lines, as broken for the whole score (for previews such
    /// as an incipit); 0 keeps them all. An initial spanning more lines keeps its full size,
    /// and the height includes it. The timeline ends with the kept lines.
    pub max_lines: usize,
}

impl Default for LayoutOptions {
    fn default() -> LayoutOptions {
        LayoutOptions {
            scale: 6.0,
            last_line: LastLine::Ragged,
            max_lines: 0,
        }
    }
}

/// The least space between the notes of two syllables, and of two words: GregorioTeX's
/// `intersyllablespacenotes` (0.24 cm) and `interwordspacenotes` (0.29 cm) on its default
/// staff, whose interline is 0.288 cm.
const NOTES_SYLLABLE_GAP: f32 = 1.67;
const NOTES_WORD_GAP: f32 = 2.0;
/// The space either side of a bar standing in a syllable of its own, as measured from
/// GregorioTeX's output (its bar spacing, `bar@minor` and the like, is 0.18 cm plus glue).
const BAR_GAP: f32 = 1.6;
/// Gap after the line-start clef: GregorioTeX's `spaceafterlineclef` is 0.23 cm, and its
/// output shows 1.5 staff spaces from the clef's ink to the first note.
const CLEF_GAP: f32 = 1.5;
/// Gap between the first staff line and the lowest annotation's baseline.
const ANNOTATION_GAP: f32 = 1.0;
/// Gap between the top of a one-staff initial and the baseline of the annotation over it,
/// as GregorioTeX sets them.
const INITIAL_ANNOTATION_GAP: f32 = 1.6;
/// Space before the initial's column: GregorioTeX's `beforeinitialshift` (0.2 cm).
pub(crate) const INITIAL_BEFORE: f32 = 1.39;
/// Gap between the initial's column and the staff.
const INITIAL_GAP: f32 = 1.0;
/// Gap between the tail of a one-staff initial and the ink of the line under it.
const INITIAL_TAIL_GAP: f32 = 0.5;
/// Gap before an end-of-line custos: GregorioTeX's `spacebeforeeolcustos` (0.23 cm).
const CUSTOS_GAP: f32 = 1.6;
/// Two syllables of a word whose texts are closer than this touch, and no hyphen goes between
/// them; any wider gap gets one, set right after the first syllable's text (GregorioTeX's
/// `maximumspacewithoutdash` is zero).
const HYPHEN_MIN_GAP: f32 = 0.01;
/// How much a gap stretches when a line is justified, relative to a gap between words: a gap
/// inside a syllable (between note groups of a melisma) barely stretches, and syllables whose
/// texts touch stay together, as in GregorioTeX, where only those gaps lack stretchable glue.
const STRETCH_IN_SYLLABLE: f32 = 0.25;
/// How far the space between two words may shrink to fit a line: GregorioTeX's
/// interwordspacetext and interwordspacenotes give 0.05 cm.
const SHRINK: f32 = 0.35;
/// The most of the space between two words' texts that shrinking may take, so that small
/// lyrics, whose word space is narrower than [`SHRINK`] allows for, never meet.
const SHRINK_OF_WORD_SPACE: f32 = 0.3;
/// How far each gap may stretch before a line counts as loose, in staff spaces.
const STRETCH: f32 = 1.5;
/// The widest gap between two notes' ledger lines that is drawn through, in staff spaces, as
/// the notes are spaced before a line is justified. GregorioTeX's ledger lines overhang their
/// notes by about 0.95 staff spaces, so those of notes 1.9 apart meet, and those of
/// neighbouring words' notes ([`NOTES_WORD_GAP`]) all but meet; a wider gap, as where the
/// text holds the notes apart, parts them. These overhang by 0.25 (see `add_markings`).
const LEDGER_JOIN: f32 = NOTES_WORD_GAP + 0.1 - 2.0 * 0.25;
/// The widest column laid out, in output units and in staff spaces; wider requests are
/// clamped to it.
const MAX_WIDTH: f32 = 1.0e6;
/// The lyric baseline's drop below the staff's bottom line: GregorioTeX's `spacelinestext`
/// (3.48 ex of its lyrics), as measured from its output.
const TEXT_DROP: f32 = 3.3;
/// GregorioTeX lowers the lyrics by `noteadditionalspacelinestext` (0.144 cm, about a staff
/// space) for each step the score's lowest note lies below this position (`c` on a four-line
/// staff).
const BELOW_STAFF: i8 = -4;
const LOW_NOTE_DROP: f32 = 1.0;
/// The least gap between ink hanging below the staff and the tops of the letters under it
/// (an accent, a capital, an ascender), in staff spaces.
const LYRIC_CLEARANCE: f32 = 0.25;
/// How far to either side of ink a letter still counts as under it.
const LYRIC_SIDE: f32 = 0.2;
/// Distance between the lyric baselines of consecutive lines: GregorioTeX's `baselineskip`
/// (55 pt on its default staff).
pub(crate) const BASELINE_PITCH: f32 = 13.43;
/// The extra space above a line of text with no staff.
const TEXT_LINE_GAP: f32 = 1.5;
/// Space between stacked lines, in staff spaces.
const LINE_GAP: f32 = 1.0;
/// Demerits for each line, which outweigh any line's own within the tolerance, so a score
/// takes as few lines as it can, as GregorioTeX's `\looseness=-1` asks.
const LINE_PENALTY: f64 = 1.0e6;
/// The loosest line taken to save a line: GregorioTeX's tolerance of 9000 lets its word
/// gaps stretch about 4.5 times their 0.05 cm glue, 1.6 staff spaces, which is badness 115
/// here.
const TOLERANCE: f32 = 115.0;
/// Demerits for each point of badness past the tolerance: a line that loose is taken only
/// to avoid one looser still, so lines before a written break share the slack.
const TOO_LOOSE: f64 = 1.0e4;
/// Extra demerits for a break inside a melisma: about a moderately loose line's worth.
const MELISMA_DEMERITS: f64 = 2500.0;
/// Demerits for a break at a word's end and after a bar, against one between syllables of a
/// word: GregorioTeX's break penalties (`endofwordpenalty` -100, `endafterbarpenalty` -200,
/// `endofsyllablepenalty` -50), each counted as TeX counts a negative penalty, minus its
/// square, and scaled by [`BREAK_SCALE`].
const WORD_END_DEMERITS: f64 = BREAK_SCALE * (50.0 * 50.0 - 100.0 * 100.0);
const AFTER_BAR_DEMERITS: f64 = BREAK_SCALE * (50.0 * 50.0 - 200.0 * 200.0);
/// TeX weighs those penalties against badness in the thousands; badness here is about a
/// 79th of TeX's for the same line (see [`TOLERANCE`]), and the penalties weigh as much
/// against it at about a 300th. Across GregoBase this matches GregorioTeX's breaks best:
/// more often a word's end than a cut inside one, but never at the price of a loose line.
const BREAK_SCALE: f64 = 0.003;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacedLine {
    pub first: usize,
    pub last: usize,
    /// x of each segment origin, in staff spaces from the line's left edge.
    pub xs: Vec<f32>,
    pub clef: Option<Clef>,
    /// Where the staff starts: past the initial's column on the first lines, else 0.
    pub indent: f32,
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
    /// The line has text and no ink, so it draws no staff (and `staff` is its top).
    pub staffless: bool,
    /// How far a gap between words stretched to fill the column, in staff spaces.
    pub stretch: f32,
    /// Ledger lines drawn across the gap between neighbouring notes' own, as `(y, left,
    /// right)` from the staff's center and the line's left edge.
    pub bridges: Vec<(f32, f32, f32)>,
}

/// Where the initial's capital goes, in staff spaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacedInitial {
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
    /// Width of the column the initial and annotations share.
    pub column: f32,
    /// Width the capital would take to span its staves, before narrowing to the column.
    pub natural_width: f32,
    /// Baseline of the lowest annotation line.
    pub annotation_baseline: f32,
}

/// A layout at one width.
#[derive(Clone, Debug)]
pub struct Layout<'e> {
    pub(crate) eng: &'e Engraving,
    pub(crate) lines: Vec<PlacedLine>,
    pub(crate) initial: Option<PlacedInitial>,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) scale: f32,
}

impl Layout<'_> {
    /// Width and height in output units. The width is the one asked for, unless a line
    /// couldn't fit it (see [`Engraving::layout`]).
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
    /// The right end of the last ink's [`Segment::spacing`], if it has one.
    spacing_right: Option<f32>,
    lyric_right: Option<f32>,
    /// How far the last text's ink runs past `lyric_right`.
    lyric_tail: f32,
    word_continues: bool,
    /// The last text ends with a hyphen of its own.
    own_hyphen: bool,
    /// The last segment is a bar standing alone.
    after_bar: bool,
    x: f32,
}

/// Where a segment goes after the cursor.
#[derive(Clone, Copy)]
struct Spot {
    x: f32,
    /// How much the gap before this segment stretches (see [`STRETCH_IN_SYLLABLE`]).
    weight: f32,
    /// How far the gap before this segment may shrink, in staff spaces.
    shrink: f32,
    /// This segment's text touches the previous syllable's, in the same word. Neither this
    /// gap nor any since that syllable's text may stretch, or the word would come apart
    /// with no hyphen.
    touching: bool,
}

fn place(cur: &Cursor, seg: &Space, hyphen: f32, word_space: f32, line_start: f32) -> Spot {
    let mut x = cur.x;
    match (cur.ink_right, seg.ink) {
        (Some(r), Some((l, _))) => {
            // Notes are spaced by their heads, their ledger lines left to reach toward each
            // other; a bar or a clef keeps clear of those too.
            let (r, l) = match (cur.spacing_right, seg.spacing) {
                (Some(r), Some((l, _))) => (r, l),
                _ => (r, l),
            };
            let gap = if seg.first {
                if seg.is_bar || cur.after_bar {
                    BAR_GAP
                } else if seg.word_start {
                    NOTES_WORD_GAP
                } else {
                    NOTES_SYLLABLE_GAP
                }
            } else {
                seg.space_before
            };
            x = x.max(r + gap - l);
        }
        (None, Some((l, _))) => x = x.max(line_start - l),
        _ => {}
    }
    let mut touching = false;
    if let Some(t) = &seg.lyric {
        // A word ending in a syllable with no text (`quam(e)(/) *()`) leaves the text before
        // it unended; the next word's text still keeps a word space from it.
        let word_continues = cur.word_continues && !seg.word_start;
        match cur.lyric_right {
            // A text with its own hyphen may touch the next one, and needs no other.
            Some(r) if word_continues && cur.own_hyphen => {
                x = x.max(r - t.left);
                touching = x + t.left - r <= HYPHEN_MIN_GAP;
            }
            Some(r) if word_continues => {
                // Within a word the texts may touch. If the notes hold them apart, a hyphen
                // follows the first text's ink, and the second's ink must clear it.
                x = x.max(r - t.left);
                if x + t.left - r > HYPHEN_MIN_GAP {
                    x = x.max(r + cur.lyric_tail + hyphen - t.ink_left());
                } else {
                    touching = true;
                }
            }
            // A word space, and the two words' inks apart.
            Some(r) => x = x.max(r + word_space - t.left).max(r + cur.lyric_tail - t.ink_left()),
            None => x = x.max(-t.ink_left()),
        }
    }
    let weight = if !seg.first {
        STRETCH_IN_SYLLABLE
    } else if touching {
        0.0
    } else {
        1.0
    };
    // Nothing shrinks before a line's first text, which may sit right at the line's start.
    let shrink = if seg.first && seg.word_start && !seg.is_bar && !cur.after_bar && cur.lyric_right.is_some() {
        SHRINK.min(word_space * SHRINK_OF_WORD_SPACE)
    } else {
        0.0
    };
    Spot {
        x,
        touching,
        weight,
        shrink,
    }
}

fn advance(cur: &Cursor, seg: &Space, x: f32) -> Cursor {
    let mut next = *cur;
    next.x = x;
    if seg.ink.is_some() {
        next.after_bar = seg.is_bar;
    }
    if let Some((_, r)) = seg.ink {
        next.ink_right = Some(x + r);
        next.spacing_right = seg.spacing.map(|(_, r)| x + r);
    }
    if let Some(t) = &seg.lyric {
        next.lyric_right = Some(x + t.left + t.width);
        next.lyric_tail = t.tail;
        next.word_continues = !t.word_end;
        next.own_hyphen = t.hyphenated;
    }
    next
}

struct Trial {
    xs: Vec<f32>,
    /// Per segment: how much the gap before it stretches.
    weights: Vec<f32>,
    /// Per segment: its text touches the previous syllable's, so no hyphen goes between them
    /// unless the whole line is spread evenly (see [`Spot::touching`]).
    touching: Vec<bool>,
    shrinks: Vec<f32>,
    natural: f32,
    /// The width with every gap shrunk all it may. It is less than `natural` by less than the
    /// shrinks' sum when the line's right end isn't its last segment's (a text running on
    /// past an empty syllable's).
    shrunk: f32,
}

/// What a line adds at its end beyond its segments: the custos's width, if it has one, and
/// whether its last word goes on to the next line (so it may end with a hyphen).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Closing {
    custos: Option<f32>,
    word_goes_on: bool,
}

/// What spacing reads of a segment, gathered so the breaker's inner loop needn't reach into
/// the segment for it.
#[derive(Clone, Copy, Debug)]
struct Space {
    first: bool,
    word_start: bool,
    is_bar: bool,
    ink: Option<(f32, f32)>,
    spacing: Option<(f32, f32)>,
    space_before: f32,
    lyric: Option<Text>,
    right: f32,
}

/// What spacing reads of a segment's text (see [`LyricBox`](crate::engrave::LyricBox)).
#[derive(Clone, Copy, Debug)]
struct Text {
    left: f32,
    width: f32,
    lead: f32,
    tail: f32,
    word_end: bool,
    hyphenated: bool,
}

impl Text {
    fn ink_left(&self) -> f32 {
        self.left - self.lead
    }
}

impl Space {
    fn of(seg: &Segment) -> Space {
        Space {
            first: seg.first,
            word_start: seg.word_start,
            is_bar: seg.is_bar(),
            ink: seg.ink,
            spacing: seg.spacing,
            space_before: seg.space_before,
            lyric: seg.lyric.as_ref().map(|t| Text {
                left: t.left,
                width: t.width,
                lead: t.lead,
                tail: t.tail,
                word_end: t.word_end,
                hyphenated: t.hyphenated,
            }),
            right: seg.right(),
        }
    }
}

impl PartialEq for Space {
    fn eq(&self, o: &Space) -> bool {
        let b = f32::to_bits;
        let pair = |p: Option<(f32, f32)>| p.map(|(x, y)| (b(x), b(y)));
        let text = |t: Option<Text>| t.map(|t| (b(t.left), b(t.width), b(t.lead), b(t.tail), t.word_end, t.hyphenated));
        self.first == o.first
            && self.word_start == o.word_start
            && self.is_bar == o.is_bar
            && pair(self.ink) == pair(o.ink)
            && pair(self.spacing) == pair(o.spacing)
            && b(self.space_before) == b(o.space_before)
            && text(self.lyric) == text(o.lyric)
            && b(self.right) == b(o.right)
    }
}

/// What the line breaker reads of a segment, so a line whose segments all read the same
/// breaks the same way. Floats compare by their bits.
#[derive(Clone, Copy, Debug)]
struct Fit {
    space: Space,
    end_of_score: bool,
    after: Break,
    /// Where a line starting here starts, after its clef.
    start: f32,
    closing: Closing,
    /// The extra demerits for a break after the segment, which hang on the next one too.
    cost: f64,
    /// The segment starts the initial's syllable.
    initial: bool,
}

impl PartialEq for Fit {
    fn eq(&self, o: &Fit) -> bool {
        let b = f32::to_bits;
        self.space == o.space
            && self.end_of_score == o.end_of_score
            && self.after == o.after
            && b(self.start) == b(o.start)
            && self.closing.custos.map(b) == o.closing.custos.map(b)
            && self.closing.word_goes_on == o.closing.word_goes_on
            && self.cost.to_bits() == o.cost.to_bits()
            && self.initial == o.initial
    }
}

/// What every line's candidates depend on beyond their own segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BreakKey {
    target: u32,
    indent: u32,
    hyphen: u32,
    word_space: u32,
    last_line: LastLine,
}

/// A break a line may take (see `Engraving::line_candidates`).
#[derive(Clone, Copy, Debug)]
struct Candidate {
    /// The segment the line ends after, counted from the line's first, so a line moved by
    /// an edit before it keeps its candidates as they are.
    end: u32,
    demerits: f64,
    break_cost: f64,
}

/// One line start's candidates in `BreakTable::cands`, and the last segment they read.
#[derive(Clone, Copy, Debug)]
struct Span {
    from: u32,
    to: u32,
    reach: u32,
}

/// The candidates of every line start the breaker has needed, for one column and indent,
/// with the fits they were worked out from.
#[derive(Clone, Debug)]
struct BreakTable {
    key: Option<BreakKey>,
    fits: Vec<Fit>,
    /// By line start, then whether the line is indented for the initial.
    lines: Vec<[Option<Span>; 2]>,
    cands: Vec<Candidate>,
    /// The previous layout's table, while this one takes what it can from it.
    old: Option<Box<BreakTable>>,
    /// How many leading fits the old table shares, and how many trailing ones.
    same_head: usize,
    same_tail: usize,
}

impl BreakTable {
    fn empty() -> BreakTable {
        BreakTable {
            key: None,
            fits: Vec::new(),
            lines: Vec::new(),
            cands: Vec::new(),
            old: None,
            same_head: 0,
            same_tail: 0,
        }
    }

    /// Starts a table for `eng`, keeping this one as the old table to reuse from when it was
    /// made for the same key.
    fn refit(&mut self, eng: &Engraving, key: BreakKey) {
        let fits = eng.fits();
        let mut old = std::mem::replace(self, BreakTable::empty());
        old.old = None;
        let n = fits.len();
        self.lines = vec![[None, None]; n];
        if old.key == Some(key) {
            let m = old.fits.len();
            let same_head = fits.iter().zip(&old.fits).take_while(|(a, b)| a == b).count();
            let same_tail = fits.iter().rev().zip(old.fits.iter().rev()).take_while(|(a, b)| a == b).count();
            self.same_head = same_head;
            self.same_tail = same_tail.min(n).min(m);
            self.cands.reserve(old.cands.len());
            self.old = Some(Box::new(old));
        }
        self.key = Some(key);
        self.fits = fits;
    }

    /// The candidates of a line starting at `first`, from the old table if its segments are
    /// unchanged there, else worked out now.
    fn candidates(&mut self, eng: &Engraving, first: usize, indented: bool, target: f32, opts: &LayoutOptions) -> (usize, usize) {
        let c = usize::from(indented);
        if let Some(sp) = self.lines[first][c] {
            return (sp.from as usize, sp.to as usize);
        }
        let n = self.fits.len();
        let from = self.cands.len();
        let mut reach = None;
        if let Some(old) = &self.old {
            let m = old.fits.len();
            // Unchanged before the edit: the same segments at the same place.
            let head = (first < m)
                .then(|| old.lines[first][c])
                .flatten()
                .filter(|sp| (sp.reach as usize) < self.same_head)
                .map(|sp| (sp, 0isize));
            // Unchanged after it: the same segments, moved by what the edit added or removed.
            let tail = || {
                let moved = first + self.same_tail >= n && first + m >= n;
                moved
                    .then(|| old.lines[first + m - n][c])
                    .flatten()
                    .map(|sp| (sp, n as isize - m as isize))
            };
            if let Some((sp, shift)) = head.or_else(tail) {
                self.cands.extend_from_slice(&old.cands[sp.from as usize..sp.to as usize]);
                reach = Some((sp.reach as isize + shift) as usize);
            }
        }
        let reach = match reach {
            Some(r) => r,
            None => eng.line_candidates(first, target, opts, &self.fits, &mut self.cands),
        };
        let to = self.cands.len();
        self.lines[first][c] = Some(Span {
            from: from as u32,
            to: to as u32,
            reach: reach as u32,
        });
        (from, to)
    }
}

/// What [`Engraving::layout_cached`] keeps between layouts: the line breaker's work, so a
/// layout after a small edit redoes only the lines the edit touched.
#[derive(Clone, Debug, Default)]
pub struct LayoutCache {
    /// Most recent last; one per column and indent the breaker has run with lately.
    tables: Vec<BreakTable>,
}

impl LayoutCache {
    /// Takes the table for `key`, or the most recent one to start over in.
    fn take(&mut self, key: &BreakKey) -> BreakTable {
        match self.tables.iter().rposition(|t| t.key.as_ref() == Some(key)) {
            Some(i) => self.tables.remove(i),
            None => BreakTable::empty(),
        }
    }

    fn put(&mut self, mut table: BreakTable) {
        table.old = None;
        // A layout with a wide initial breaks up to three times, at different indents.
        if self.tables.len() >= 3 {
            self.tables.remove(0);
        }
        self.tables.push(table);
    }
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
        if self.segments[first].starts_with_clef || first >= self.inkless_from {
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
        self.next_note.get(last).copied().flatten()
    }

    fn trial(&self, first: usize, last: usize, start: f32) -> Trial {
        let mut cur = Cursor {
            ink_right: None,
            spacing_right: None,
            lyric_right: None,
            lyric_tail: 0.0,
            word_continues: false,
            own_hyphen: false,
            after_bar: false,
            x: start,
        };
        let mut xs = Vec::with_capacity(last - first + 1);
        let mut weights = Vec::with_capacity(last - first + 1);
        let mut shrinks = Vec::with_capacity(last - first + 1);
        let mut touching = Vec::with_capacity(last - first + 1);
        let mut right = 0.0f32;
        let mut ink_end = start;
        let mut last_lyric = 0;
        // The same with every gap shrunk all it may: the segments shift left by the shrink
        // of all the gaps before them.
        let mut gone = 0.0f32;
        let mut cur_shrunk = cur;
        let mut right_shrunk = 0.0f32;
        let mut ink_end_shrunk = start;
        for seg in &self.segments[first..=last] {
            let seg = &Space::of(seg);
            let spot = place(&cur, seg, self.hyphen, self.word_space, start);
            let x = spot.x;
            xs.push(x);
            touching.push(spot.touching);
            if spot.touching {
                for w in &mut weights[last_lyric + 1..] {
                    *w = 0.0;
                }
            }
            weights.push(if xs.len() == 1 { 0.0 } else { spot.weight });
            if seg.lyric.is_some() {
                last_lyric = xs.len() - 1;
            }
            shrinks.push(if xs.len() == 1 { 0.0 } else { spot.shrink });
            gone += shrinks[shrinks.len() - 1];
            cur = advance(&cur, seg, x);
            cur_shrunk = advance(&cur_shrunk, seg, x - gone);
            right = right.max(x + seg.right);
            right_shrunk = right_shrunk.max(x - gone + seg.right);
            if let Some((_, r)) = seg.ink {
                ink_end = ink_end.max(x + r);
                ink_end_shrunk = ink_end_shrunk.max(x - gone + r);
            }
        }
        Trial {
            xs,
            weights,
            touching,
            shrinks,
            natural: self.natural(&cur, right, ink_end, self.closing(last)),
            shrunk: self.natural(&cur_shrunk, right_shrunk, ink_end_shrunk, self.closing(last)),
        }
    }

    /// The width of the line `first..=last` set at `xs`, and its ink's right end.
    fn extent(&self, first: usize, last: usize, xs: &[f32], start: f32) -> (f32, f32) {
        let mut cur = Cursor {
            ink_right: None,
            spacing_right: None,
            lyric_right: None,
            lyric_tail: 0.0,
            word_continues: false,
            own_hyphen: false,
            after_bar: false,
            x: start,
        };
        let mut right = 0.0f32;
        let mut ink_end = start;
        for (seg, &x) in self.segments[first..=last].iter().zip(xs) {
            let seg = &Space::of(seg);
            cur = advance(&cur, seg, x);
            right = right.max(x + seg.right);
            if let Some((_, r)) = seg.ink {
                ink_end = ink_end.max(x + r);
            }
        }
        (self.natural(&cur, right, ink_end, self.closing(last)), ink_end)
    }

    /// Whether the next text after segment `last` is in the same word, so a line ending there
    /// ends with a hyphen when its last text's word goes on.
    fn continues_past(&self, last: usize) -> bool {
        self.segments[last + 1..]
            .iter()
            .find(|s| s.lyric.is_some())
            .is_some_and(|s| !s.word_start)
    }

    /// The width a line ending at `last` needs: its segments, plus a trailing hyphen and the
    /// custos.
    fn natural(&self, cur: &Cursor, mut right: f32, ink_end: f32, close: Closing) -> f32 {
        if cur.word_continues
            && !cur.own_hyphen
            && close.word_goes_on
            && let Some(r) = cur.lyric_right
        {
            right = right.max(r + cur.lyric_tail + self.hyphen);
        }
        if let Some(w) = close.custos {
            right = right.max(ink_end + CUSTOS_GAP + w);
        }
        right
    }

    /// How a line ending after segment `last` closes, for its width.
    fn closing(&self, last: usize) -> Closing {
        Closing {
            custos: self.custos_width(last),
            word_goes_on: self.continues_past(last),
        }
    }

    /// The width of the custos at the end of a line ending at `last`, if it has one.
    fn custos_width(&self, last: usize) -> Option<f32> {
        if last + 1 < self.segments.len() {
            self.custos_for(last).map(|p| custos_piece(p, 0.0).1)
        } else {
            None
        }
    }

    /// The breaker's view of every segment.
    fn fits(&self) -> Vec<Fit> {
        let n = self.segments.len();
        self.closings()
            .into_iter()
            .zip(&self.segments)
            .enumerate()
            .map(|(k, (closing, seg))| Fit {
                space: Space::of(seg),
                end_of_score: k + 1 == n,
                after: seg.after,
                start: self.line_start(k).1,
                closing,
                cost: self.break_cost(k),
                initial: self.initial.as_ref().is_some_and(|i| i.syllable == seg.syllable && seg.first),
            })
            .collect()
    }

    /// `closing` for every segment at once, in one pass from the end, so the line breaker
    /// neither measures a custos nor scans ahead for the next text at each step.
    fn closings(&self) -> Vec<Closing> {
        let mut out = vec![Closing::default(); self.segments.len()];
        // Whether the next text after the segment being looked at starts a word.
        let mut next_word_start = None;
        for k in (0..self.segments.len()).rev() {
            out[k] = Closing {
                custos: self.custos_width(k),
                word_goes_on: next_word_start == Some(false),
            };
            if self.segments[k].lyric.is_some() {
                next_word_start = Some(self.segments[k].word_start);
            }
        }
        out
    }

    /// How far below the staff's center a line's lyric baseline must lie for every letter
    /// (and hyphen) to clear the ink over it by [`LYRIC_CLEARANCE`], where that is lower than
    /// its usual drop; 0 if no ink hangs low enough to matter. `xs` and the hyphens are the
    /// line's, as set; `others` is the ink not in its segments (the clef and the custos),
    /// placed on the line.
    fn text_clearance(&self, first: usize, xs: &[f32], hyphens: &[f32], hyphen: Option<f32>, others: &[Piece]) -> f32 {
        let segments = &self.segments[first..first + xs.len()];
        let hyphen_top = HYPHEN_TOP * self.lyric_size;
        // Ink no lower than this can't reach past the usual drop even over the tallest
        // letter, so need not be looked at; most lines have none lower.
        let tallest = segments
            .iter()
            .filter_map(|s| s.lyric.as_ref())
            .flat_map(|t| &t.tops)
            .map(|e| e.2)
            .chain((!hyphens.is_empty() || hyphen.is_some()).then_some(hyphen_top))
            .fold(0.0f32, f32::max);
        let low = 3.0 + self.text_drop() - LYRIC_CLEARANCE - tallest;
        let segment_ink = segments.iter().zip(xs).flat_map(|(seg, &x)| seg.pieces.iter().map(move |p| (p, x)));
        let inks: Vec<(f32, f32, f32)> = segment_ink
            .chain(others.iter().map(|p| (p, 0.0)))
            .filter_map(|(p, x)| {
                let [l, _, r, bottom] = p.ink_box();
                (bottom > low).then_some((l + x - LYRIC_SIDE, r + x + LYRIC_SIDE, bottom))
            })
            .collect();
        if inks.is_empty() {
            return 0.0;
        }
        let mut boxes: Vec<(f32, f32, f32)> = Vec::new();
        for (seg, &x) in segments.iter().zip(xs) {
            if let Some(t) = &seg.lyric {
                let left = x + t.left;
                boxes.extend(t.tops.iter().map(|&(l, r, top)| (left + l, left + r, top)));
            }
        }
        let h = self.hyphen / 2.0;
        boxes.extend(hyphens.iter().chain(hyphen.as_ref()).map(|&c| (c - h, c + h, hyphen_top)));
        // In order of left edge, so the letters under a piece of ink are a short run of them.
        boxes.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let widest = boxes.iter().fold(0.0f32, |a, e| a.max(e.1 - e.0));
        let mut need = 0.0f32;
        for (l, r, bottom) in inks {
            let from = boxes.partition_point(|e| e.0 < l - widest - 1.0);
            for &(_, _, top) in boxes[from..].iter().take_while(|e| e.0 < r).filter(|e| e.1 > l) {
                need = need.max(bottom + LYRIC_CLEARANCE + top);
            }
        }
        need
    }

    /// The ledger lines that join those of neighbouring notes on the line `first..=last`, set
    /// at `xs`: where the notes, as spaced at `natural` before the line was justified, lie
    /// close enough for GregorioTeX's to meet, with no bar or clef between them. So
    /// the same notes join alike on every line, however loose.
    fn ledger_bridges(&self, first: usize, last: usize, natural: &[f32], xs: &[f32]) -> Vec<(f32, f32, f32)> {
        // (y, left and right as spaced, left and right as set)
        let mut ledgers: Vec<(f32, f32, f32, f32, f32)> = Vec::new();
        let mut walls: Vec<f32> = Vec::new();
        for (i, seg) in self.segments[first..=last].iter().enumerate() {
            for p in &seg.pieces {
                match (p.role, p.mark) {
                    (Ink::Ledger, Mark::Rect { x: l, y, w, .. }) => {
                        ledgers.push((y, natural[i] + l, natural[i] + l + w, xs[i] + l, xs[i] + l + w));
                    }
                    (Ink::Bar | Ink::Clef | Ink::Custos, _) => {
                        let [l, _, r, _] = p.ink_box();
                        walls.push(natural[i] + (l + r) / 2.0);
                    }
                    _ => {}
                }
            }
        }
        if ledgers.len() < 2 {
            return Vec::new();
        }
        ledgers.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        ledgers
            .windows(2)
            .filter(|w| {
                let ((y, _, r, _, _), (y2, l, _, _, _)) = (w[0], w[1]);
                y == y2 && l > r && l - r < LEDGER_JOIN && !walls.iter().any(|&x| x > r && x < l)
            })
            .map(|w| (w[0].0, w[0].4, w[1].3))
            .collect()
    }

    /// How far the lyric baseline lies below the staff's bottom line.
    fn text_drop(&self) -> f32 {
        TEXT_DROP + LOW_NOTE_DROP * (BELOW_STAFF - self.lowest).max(0) as f32
    }

    /// How a line ending after segment `end` closes: whether it is set ragged, and the extra
    /// demerits for the break it takes.
    #[cfg(test)]
    fn line_end(&self, end: usize, opts: &LayoutOptions) -> (bool, f64) {
        (self.ragged(end, opts), self.break_cost(end))
    }

    /// Whether a line ending after segment `end` is set ragged.
    fn ragged(&self, end: usize, opts: &LayoutOptions) -> bool {
        end + 1 == self.segments.len() && opts.last_line == LastLine::Ragged
            || matches!(self.segments[end].after, Break::Forced { justify: false, .. })
    }

    /// The extra demerits for a line break after segment `end`.
    fn break_cost(&self, end: usize) -> f64 {
        let after = self.segments[end].after;
        // A syllable's end is a better break than a cut inside its melisma, a word's end
        // better still, and a bar best. A ℣ or ℟ under a bar (`<sp>V/</sp>.(::)`) leads into
        // the verse after it, so a line ending there would leave it behind: no better than
        // any break.
        let seg = &self.segments[end];
        let leads = seg
            .lyric
            .as_ref()
            .is_some_and(|t| t.runs.first().is_some_and(|r| r.text.starts_with(['℣', '℟'])));
        match after {
            Break::InMelisma => MELISMA_DEMERITS,
            Break::Allowed if seg.is_bar() && leads => 0.0,
            Break::Allowed if seg.is_bar() => AFTER_BAR_DEMERITS,
            Break::Allowed if self.segments.get(end + 1).is_none_or(|s| s.word_start) => WORD_END_DEMERITS,
            _ => 0.0,
        }
    }

    /// The breaks a line starting at segment `first` may take in a column `target` wide: for
    /// each, the segment it ends after, the line's demerits and the break's own, in the order
    /// the breaker tries them. Pushed onto `out`; returns the last segment it looked at.
    fn line_candidates(&self, first: usize, target: f32, opts: &LayoutOptions, fits: &[Fit], out: &mut Vec<Candidate>) -> usize {
        let n = self.segments.len();
        let start = fits[first].start;
        // Packs the line one segment at a time, as `trial` does, so each candidate costs
        // one step instead of a repack.
        let mut cur = Cursor {
            ink_right: None,
            spacing_right: None,
            lyric_right: None,
            lyric_tail: 0.0,
            word_continues: false,
            own_hyphen: false,
            after_bar: false,
            x: start,
        };
        let mut right = 0.0f32;
        let mut ink_end = start;
        // How far the line's gaps stretch together, relative to a gap between words.
        let mut stretch_weight = 0.0f32;
        // The part of it since the last text, which a touching text takes back.
        let mut since_text = 0.0f32;
        // How far its word gaps may shrink together, and the line as packed with them
        // all shrunk (see `Trial::shrunk`).
        let mut gone = 0.0f32;
        let mut cur_shrunk = cur;
        let mut right_shrunk = 0.0f32;
        let mut ink_end_shrunk = start;
        // Whether this line has passed a boundary where it may end.
        let mut breakable_seen = false;
        for (last, fit) in fits.iter().enumerate().skip(first) {
            let seg = &fit.space;
            let end_of_score = last + 1 == n;
            let forced = matches!(fit.after, Break::Forced { .. });
            let breakable = end_of_score || forced || matches!(fit.after, Break::Allowed | Break::InMelisma);
            let spot = place(&cur, seg, self.hyphen, self.word_space, start);
            let x = spot.x;
            if spot.touching {
                stretch_weight -= since_text;
                since_text = 0.0;
            }
            if last > first {
                stretch_weight += spot.weight;
                since_text += spot.weight;
                gone += spot.shrink;
            }
            if seg.lyric.is_some() {
                since_text = 0.0;
            }
            cur = advance(&cur, seg, x);
            cur_shrunk = advance(&cur_shrunk, seg, x - gone);
            right = right.max(x + seg.right);
            right_shrunk = right_shrunk.max(x - gone + seg.right);
            if let Some((_, r)) = seg.ink {
                ink_end = ink_end.max(x + r);
                ink_end_shrunk = ink_end_shrunk.max(x - gone + r);
            }
            let close = fit.closing;
            let natural = self.natural(&cur, right, ink_end, close);
            let shrink = natural - self.natural(&cur_shrunk, right_shrunk, ink_end_shrunk, close);
            // A line may be a little wider than the column: its word gaps shrink, as
            // GregorioTeX's glue does.
            let over = natural > target + shrink;
            let squeezed = natural > target && !over;
            // Past the width with only forbidden breaks behind it, as in an unclosed
            // `<nlba>`: the line ends at the last of them, or after this segment when it
            // alone is too wide, rather than nowhere, which left the walk back to set every
            // segment on a line of its own.
            let stuck = over && !breakable_seen;
            if breakable || stuck {
                // The initial's syllable keeps to the first line even when too wide for it:
                // ending before it would part it from the bar or clef written before it.
                let end = if stuck && last > first && !fit.initial { last - 1 } else { last };
                let gaps = (last - first) as f32;
                // A stuck line ends before this segment, so the break it takes is `end`'s.
                let (ragged, break_cost) = (self.ragged(end, opts), fits[end].cost);
                let badness = if over {
                    if last == first || stuck { 10000.0 } else { f32::INFINITY }
                } else if squeezed {
                    let r = (natural - target) / shrink;
                    (100.0 * r * r * r).min(10000.0)
                } else if ragged {
                    0.0
                } else if gaps == 0.0 {
                    // As bad as the loosest line with gaps, so splitting a loose line
                    // into one-segment lines never looks cheaper.
                    if target - natural > 0.5 { 10000.0 } else { 0.0 }
                } else {
                    // A line of one word whose syllables touch can only stretch evenly, if at all.
                    let capacity = if stretch_weight > 0.0 { stretch_weight } else { gaps };
                    let r = (target - natural) / (capacity * STRETCH);
                    (100.0 * r * r * r).min(10000.0)
                };
                if badness.is_finite() {
                    // As GregorioTeX (looseness -1, tolerance 9000): as few lines as can be
                    // set no looser than it allows, the best of those by demerits.
                    // In f64: a long score's sum of line penalties would swamp f32.
                    let b = f64::from(badness);
                    let d = (10.0 + b) * (10.0 + b) + LINE_PENALTY + TOO_LOOSE * (b - f64::from(TOLERANCE)).max(0.0);
                    // A syllable's end is a better break than a cut inside its melisma.
                    out.push(Candidate {
                        end: (end - first) as u32,
                        demerits: d,
                        break_cost,
                    });
                }
            }
            if forced || over {
                return last;
            }
            breakable_seen |= breakable;
        }
        n - 1
    }

    /// Lays the engraving out at `width` output units.
    ///
    /// A stretch of notes too wide for the column with no break allowed in it (one long
    /// neume, or a column narrower than a clef and a syllable) still gets its line, which
    /// runs past `width`; the layout, and every staff, widen with it. That is the only
    /// way the layout comes out wider than asked: compare [`Layout::size`] with `width` to
    /// tell.
    pub fn layout(&self, width: f32, opts: &LayoutOptions) -> Layout<'_> {
        self.layout_cached(width, opts, &mut LayoutCache::default())
    }

    /// Lays the engraving out as [`layout`](Self::layout) does, reusing the line breaker's
    /// work from the last layout made with `cache` wherever the engraving is unchanged. The
    /// layout is the same as without the cache; a small edit costs a few lines' breaking
    /// rather than the score's.
    pub fn layout_cached(&self, width: f32, opts: &LayoutOptions, cache: &mut LayoutCache) -> Layout<'_> {
        let mut layout = self.layout_with(width, opts, None, cache);
        // A capital spanning staves farther apart than the nominal pitch is wider than the
        // column the breaker left for it; break again with room for it. A new break can
        // change the span, so allow one more try before narrowing the capital to fit.
        for _ in 0..2 {
            match layout.initial {
                Some(placed) if placed.natural_width > placed.column + 0.01 => {
                    layout = self.layout_with(width, opts, Some(placed.natural_width), cache);
                }
                _ => break,
            }
        }
        layout
    }

    fn layout_with(&self, width: f32, opts: &LayoutOptions, column: Option<f32>, cache: &mut LayoutCache) -> Layout<'_> {
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
                initial: None,
                width: target,
                height: 0.0,
                scale,
            };
        }
        // The first `indented` lines make room for the initial, so the breaker tracks how many
        // lines came before, up to that count.
        let indented = self.initial.as_ref().map_or(0, |i| i.lines);
        let column = self.initial.as_ref().map_or(0.0, |i| column.unwrap_or(0.0).max(i.column()));
        let indent = if indented > 0 { INITIAL_BEFORE + column + INITIAL_GAP } else { 0.0 };
        // best[k][j]: least demerits for lines ending just before segment k, with j lines so
        // far (capped at `indented`), and where the last line started and its own j.
        let mut best: Vec<Vec<Option<(f64, usize, usize)>>> = vec![vec![None; indented + 1]; n + 1];
        // Each line's candidate breaks depend only on its own segments, so they come from the
        // cache when those are unchanged (see `BreakTable`).
        let key = BreakKey {
            target: target.to_bits(),
            indent: indent.to_bits(),
            hyphen: self.hyphen.to_bits(),
            word_space: self.word_space.to_bits(),
            last_line: opts.last_line,
        };
        let mut table = cache.take(&key);
        table.refit(self, key);
        best[0][0] = Some((0.0, 0, 0));
        for first in 0..n {
            for j in 0..=indented {
                let Some((base, _, _)) = best[first][j] else { continue };
                let next = (j + 1).min(indented);
                let indented_line = j < indented;
                let line_target = if indented_line { target - indent } else { target };
                let (from, to) = table.candidates(self, first, indented_line, line_target, opts);
                for c in &table.cands[from..to] {
                    let total = base + c.demerits + c.break_cost;
                    let end = first + c.end as usize;
                    let better = best[end + 1][next].is_none_or(|(b, _, _)| total < b);
                    if better {
                        best[end + 1][next] = Some((total, first, j));
                    }
                }
            }
        }
        cache.put(table);
        // Walk back from the end, from the cheapest final state.
        let mut ranges = Vec::new();
        let mut k = n;
        let mut j = (0..=indented)
            .filter(|&j| best[n][j].is_some())
            .min_by(|&a, &b| best[n][a].unwrap().0.total_cmp(&best[n][b].unwrap().0))
            .unwrap_or(indented);
        while k > 0 {
            let (first, pj) = best[k][j].map_or((k - 1, j), |(_, f, pj)| (f, pj));
            ranges.push((first, k - 1));
            k = first;
            j = pj;
        }
        ranges.reverse();

        let size = self.lyric_size;
        let mut lines: Vec<PlacedLine> = Vec::new();
        let mut y = 0.0f32;
        let kept = if opts.max_lines == 0 {
            ranges.len()
        } else {
            opts.max_lines.min(ranges.len())
        };
        // Lines past `kept` that the initial spans are placed too, so it is sized as in the
        // whole score, then dropped.
        let placed = kept.max(indented).min(ranges.len());
        let mut rights = Vec::with_capacity(placed);
        let mut prev_baseline: Option<f32> = None;
        for (li, &(first, last)) in ranges.iter().take(placed).enumerate() {
            let line_indent = if li < indented { indent } else { 0.0 };
            let target = target - line_indent;
            let (clef, start) = self.line_start(first);
            let trial = self.trial(first, last, start);
            let ragged = self.ragged(last, opts);
            let mut xs = trial.xs.clone();
            let gaps = last - first;
            let mut stretch = 0.0;
            let mut word_gap_stretch = 0.0;
            // Whether the slack went evenly into every gap, touching ones included.
            let mut spread = false;
            if !ragged && gaps > 0 && trial.natural < target {
                // Each gap takes its share of the slack. A line whose gaps can't stretch (one
                // word, its syllables touching) spreads it evenly when that leaves room for a
                // hyphen in every gap; less slack stays at the line's end, as in GregorioTeX,
                // whose touching syllables have no glue.
                let total: f32 = trial.weights.iter().sum();
                let even = total <= 0.0;
                let per = (target - trial.natural) / if even { gaps as f32 } else { total };
                if !even || per >= self.hyphen + HYPHEN_MIN_GAP {
                    spread = even;
                    word_gap_stretch = per;
                    for (i, x) in xs.iter_mut().enumerate().skip(1) {
                        stretch += per * if even { 1.0 } else { trial.weights[i] };
                        *x += stretch;
                    }
                }
            }
            let capacity = trial.natural - trial.shrunk;
            if gaps > 0 && trial.natural > target && capacity > 0.0 {
                // Too wide by no more than its word gaps can give: they shrink alike. The
                // line's width is the most of linear functions of how far they shrink, so
                // shrinking them by this part of their all narrows it at least as much.
                let part = ((trial.natural - target) / capacity).min(1.0);
                for (i, x) in xs.iter_mut().enumerate().skip(1) {
                    stretch -= part * trial.shrinks[i];
                    *x += stretch;
                }
            }
            // The line's extent as set.
            let (natural, ink_end) = self.extent(first, last, &xs, start);
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
                    // Wherever a word's syllables are apart, right after the first text, as
                    // GregorioTeX sets it. Placing them decided that, unless an even spread
                    // parted touching ones, which leaves room for it between their advances
                    // (so it keeps off the next text's ink); the final positions' floats
                    // aren't tested again.
                    if let Some((r, true)) = prev_lyric
                        && !s.word_start
                        && (!trial.touching[i] || spread)
                    {
                        hyphens.push((r + self.hyphen / 2.0).min(xs[i] + t.ink_left() - self.hyphen / 2.0));
                    }
                    prev_lyric = Some((xs[i] + t.ink_right(), !t.word_end && !t.hyphenated));
                }
            }
            let hyphen = match prev_lyric {
                Some((r, true)) if self.continues_past(last) => Some(r + self.hyphen / 2.0),
                _ => None,
            };
            for x in xs.iter_mut().chain(hyphens.iter_mut()) {
                *x += line_indent;
            }
            let hyphen = hyphen.map(|h| h + line_indent);
            let custos = custos.map(|(p, x)| (p, x + line_indent));
            let bridges = self.ledger_bridges(first, last, &trial.xs, &xs);
            // Vertical extent.
            let custos_ink = custos.map(|(p, x)| custos_piece(p, x).0);
            let mut ink_top = -3.0f32;
            let mut ink_bottom = 3.0f32;
            // A clef rises above the staff: on the top line it must not be clipped, and on a
            // later one it must clear the descenders of the lyrics above.
            let clef_ink = clef.as_ref().map(|c| clef_pieces(c, 0.0).0).unwrap_or_default();
            let line_ink = self.segments[first..=last].iter().flat_map(|s| &s.pieces);
            // The custos counts too: one announcing a high note rises above the staff.
            for p in clef_ink.iter().chain(line_ink).chain(&custos_ink) {
                let (a, b) = p.y_extent();
                ink_top = ink_top.min(a);
                ink_bottom = ink_bottom.max(b);
            }
            let has_lyrics = self.segments[first..=last].iter().any(|s| s.lyric.is_some());
            let staffless = has_lyrics && first >= self.inkless_from;
            // The annotations sit above the first staff, over the initial and any accent on it.
            // Above a one-staff initial, which stands on the lyric line, they sit over the
            // capital, beside the staff.
            if li == 0
                && let Some(init) = &self.initial
                && init.lines == 1
            {
                let cap_top = 3.0 + self.text_drop() - CAP_HEIGHT * init.size - init.accent_room;
                if !init.annotations.is_empty() {
                    let lines = init.annotations.len() as f32;
                    ink_top =
                        ink_top.min(cap_top - INITIAL_ANNOTATION_GAP - init.annotation_ascent - (lines - 1.0) * init.annotation_size * 1.1);
                }
            } else if li == 0
                && let Some(init) = &self.initial
            {
                ink_top = ink_top.min(-3.0 - init.accent_room);
                if !init.annotations.is_empty() {
                    let lines = init.annotations.len() as f32;
                    ink_top = ink_top.min(
                        -3.0 - init.accent_room - ANNOTATION_GAP - init.annotation_ascent - (lines - 1.0) * init.annotation_size * 1.1,
                    );
                }
            }
            let mut top = y;
            let mut staff = top + (-ink_top) + 0.5;
            let mut baseline = if staffless {
                // Text alone, as a rubric after the final bar: a line of text under the last
                // staff's lyrics, set off from them by a little more than a line of text.
                let tops = self.segments[first..=last]
                    .iter()
                    .filter_map(|s| s.lyric.as_ref())
                    .flat_map(|t| &t.tops)
                    .fold(0.0f32, |a, e| a.max(e.2));
                staff = top;
                top + TEXT_LINE_GAP + tops
            } else if has_lyrics {
                // GregorioTeX's lyric line: a fixed drop below the staff, more for a score that
                // goes below the staff, the same on every line. Ink hanging lower still over a
                // letter (a low note over a capital or an accent, a sign under a note) pushes
                // it down rather than into the text.
                let others: Vec<Piece> = clef_ink.iter().map(|p| p.shifted(line_indent)).chain(custos_ink).collect();
                let clear = self.text_clearance(first, &xs, &hyphens, hyphen, &others);
                (staff + 3.0 + self.text_drop()).max(staff + clear)
            } else {
                staff + ink_bottom + 0.4
            };
            // Lines of lyrics are as far apart as GregorioTeX's baselines, or farther if the
            // notes need the room.
            if has_lyrics
                && !staffless
                && let Some(prev) = prev_baseline
                && baseline < prev + BASELINE_PITCH
            {
                let shift = prev + BASELINE_PITCH - baseline;
                top += shift;
                staff += shift;
                baseline += shift;
            }
            // Under a one-staff initial, which stands on the first line's lyric baseline, the
            // second line's ink keeps clear of its tail (a Q's).
            if li == 1
                && let Some(init) = self.initial.as_ref().filter(|i| i.lines == 1)
                && let Some(line0) = lines.first()
            {
                let reach = INITIAL_BEFORE + column + INITIAL_GAP;
                let clef_ink = clef.as_ref().map(|c| clef_pieces(c, 0.0).0).unwrap_or_default();
                let seg_ink = self.segments[first..=last]
                    .iter()
                    .zip(&xs)
                    .flat_map(|(s, &x)| s.pieces.iter().map(move |p| p.shifted(x)));
                let under = clef_ink
                    .into_iter()
                    .chain(seg_ink)
                    .filter(|p| p.ink_box()[0] < reach)
                    .map(|p| p.ink_box()[1])
                    .fold(f32::INFINITY, f32::min);
                let tail = line0.baseline + init.depth_em * init.size + INITIAL_TAIL_GAP;
                let shift = tail - (staff + under);
                if shift > 0.0 {
                    top += shift;
                    staff += shift;
                    baseline += shift;
                }
            }
            prev_baseline = has_lyrics.then_some(baseline);
            let bottom = if has_lyrics {
                (baseline + self.descent * size).max(staff + ink_bottom + 0.5)
            } else {
                baseline + 0.5
            };
            // Shrinking to the column can land a rounding error past it; that is the column.
            let natural = if natural > target && natural < target + 1e-3 {
                target
            } else {
                natural
            };
            let right = line_indent + if ragged { natural } else { target.max(natural) };
            rights.push(right);
            lines.push(PlacedLine {
                first,
                last,
                xs,
                clef,
                indent: line_indent,
                custos,
                hyphen,
                hyphens,
                top,
                staff,
                baseline,
                bottom,
                staffless,
                stretch: word_gap_stretch,
                bridges,
            });
            y = bottom + LINE_GAP;
        }
        let mut height = lines.get(kept.wrapping_sub(1)).map_or(0.0, |l| l.bottom);
        // The capital runs from the first staff's top line to the bottom line of the last
        // staff it spans, narrowed if need be to fit the column the breaker left for it.
        let initial = self.initial.as_ref().and_then(|init| {
            let first = lines.first()?;
            if init.lines == 1 {
                // GregorioTeX's default initial: a fixed size, standing on the first line's
                // lyric baseline, the lyrics running on beside it.
                let has_lyrics = self.segments[first.first..=first.last].iter().any(|s| s.lyric.is_some());
                let baseline = if has_lyrics {
                    first.baseline
                } else {
                    first.staff + 3.0 + self.text_drop()
                };
                let size = init.size;
                let width = init.advance_em * size;
                height = height.max(baseline + init.descent * size);
                return Some(PlacedInitial {
                    x: INITIAL_BEFORE + (column - width) / 2.0,
                    baseline,
                    size,
                    column,
                    natural_width: width,
                    annotation_baseline: baseline - CAP_HEIGHT * size - init.accent_room - INITIAL_ANNOTATION_GAP,
                });
            }
            let last = &lines[init.lines.min(lines.len()) - 1];
            let cap = (last.staff + 3.0) - (first.staff - 3.0);
            let natural = cap / CAP_HEIGHT;
            let mut size = natural;
            if init.advance_em > 0.0 {
                size = size.min(column / init.advance_em);
            }
            let width = init.advance_em * size;
            let baseline = last.staff + 3.0;
            height = height.max(baseline + init.descent * size);
            Some(PlacedInitial {
                x: INITIAL_BEFORE + (column - width) / 2.0,
                baseline,
                size,
                column,
                natural_width: init.advance_em * natural,
                annotation_baseline: first.staff - 3.0 - init.accent_room - ANNOTATION_GAP,
            })
        });
        lines.truncate(kept);
        // A line too wide for the column widens the layout, and every staff with it. Lines left
        // out of a preview count too, so its staves are drawn as in the whole score.
        for &(first, last) in ranges.iter().skip(placed) {
            let (_, start) = self.line_start(first);
            let trial = self.trial(first, last, start);
            let natural = if trial.natural > target {
                target.max(trial.shrunk)
            } else {
                trial.natural
            };
            let ragged = self.ragged(last, opts);
            rights.push(if ragged { natural } else { target.max(natural) });
        }
        let max_width = rights.iter().fold(0.0f32, |a, &b| a.max(b));
        Layout {
            eng: self,
            lines,
            initial,
            width: target.max(max_width),
            height,
            scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApproxMeasure, Initial, StyleOptions, parse};

    #[test]
    fn the_breakers_closings_match_each_lines_own() {
        let src = "(c4) Al(g)le(h)lu(ij)ia.(h) (,) Al(g)le(h) (;) lu(gh) (::) ia(hi) e(j)ius(j) (z) cu(g)-(h)jus(i) <i>a(g)men()";
        let eng = parse(src).score.engrave(&ApproxMeasure, &StyleOptions::default());
        let all = eng.closings();
        for (k, c) in all.iter().enumerate() {
            assert_eq!(*c, eng.closing(k), "segment {k}");
        }
        assert!(all.iter().any(|c| c.word_goes_on) && all.iter().any(|c| c.custos.is_some()));
    }

    #[test]
    fn the_first_note_keeps_gregorios_distance_from_the_clef() {
        let style = StyleOptions {
            initial: Initial::None,
            ..StyleOptions::default()
        };
        let eng = parse("(c4) a(g) b(h)").score.engrave(&ApproxMeasure, &style);
        let (clef, start) = eng.line_start(0);
        let (_, clef_right) = clef_pieces(&clef.unwrap(), 0.0);
        assert!((start - clef_right - 1.5).abs() < 1e-6);
        // The first note's ink starts there.
        let x = eng.trial(0, 0, start).xs[0];
        let (left, _) = eng.segments[0].ink.unwrap();
        assert!((x + left - start).abs() < 1e-6);
    }

    #[test]
    fn a_bar_with_text_is_spaced_as_a_bar() {
        // Gregorio sets a bar's syllable at the bar spacing whether or not it carries text,
        // and doesn't shrink around it.
        let style = StyleOptions {
            initial: Initial::None,
            ..StyleOptions::default()
        };
        for src in [
            "(c4) a(ghg) (;) b(ghg)",
            "(c4) a(ghg) *(;) b(ghg)",
            "(c4) a(ghg) <sp>V/</sp>.(::) b(ghg)",
        ] {
            let eng = parse(src).score.engrave(&ApproxMeasure, &style);
            assert_eq!(eng.segments.len(), 3, "{src}");
            let t = eng.trial(0, 2, 0.0);
            let ink = |i: usize| {
                let (l, r) = eng.segments[i].ink.unwrap();
                (t.xs[i] + l, t.xs[i] + r)
            };
            assert!((ink(1).0 - ink(0).1 - BAR_GAP).abs() < 1e-4, "{src}");
            // After the bar, the texts may hold the next syllable further off.
            assert!(ink(2).0 - ink(1).1 >= BAR_GAP - 1e-4, "{src}");
            assert_eq!(t.shrinks[1..], [0.0, 0.0], "{src}");
        }
    }

    #[test]
    fn a_hyphen_goes_only_where_placing_left_room_for_it() {
        // Syllables whose texts are a hair apart, within HYPHEN_MIN_GAP, touch: placing them
        // leaves no room for a hyphen, and justifying them by sums that round differently
        // mustn't then draw one over the next text.
        let style = StyleOptions {
            initial: Initial::None,
            ..StyleOptions::default()
        };
        let src = format!("(c4) {} (::)", ["Mag(ghg)da(g) le(g)na(h)"; 8].join(" "));
        let base = parse(&src).score.engrave(&ApproxMeasure, &style);
        let n = base.segments.len();
        let natural = base.trial(0, n - 1, 0.0);
        let opts = LayoutOptions {
            last_line: LastLine::Justified,
            ..LayoutOptions::default()
        };
        let mut steps = 0;
        for k in 1..n {
            let Some(t) = base.segments[k].lyric.as_ref().filter(|t| t.runs[0].text == "da") else {
                continue;
            };
            let prev = base.segments[..k]
                .iter()
                .zip(&natural.xs)
                .rev()
                .find_map(|(s, x)| s.lyric.as_ref().map(|p| x + p.left + p.width))
                .unwrap();
            let gap = natural.xs[k] + t.left - prev;
            for j in 0..8 {
                let mut eng = base.clone();
                // A gap just under or at the threshold, in steps finer than f32's rounding.
                eng.segments[k].lyric.as_mut().unwrap().left += HYPHEN_MIN_GAP * (1.0 - j as f32 * 1.0e-4) - gap;
                for width in (60..400).step_by(5) {
                    let layout = eng.layout(width as f32, &opts);
                    for line in &layout.lines {
                        let lefts: Vec<f32> = (line.first..=line.last)
                            .filter_map(|i| eng.segments[i].lyric.as_ref().map(|t| line.xs[i - line.first] + t.left))
                            .collect();
                        for h in &line.hyphens {
                            steps += 1;
                            // The text after it: the first to start past the one before it ends.
                            let next = lefts
                                .iter()
                                .copied()
                                .filter(|l| *l > h - eng.hyphen / 2.0 - 1e-3)
                                .fold(f32::INFINITY, f32::min);
                            assert!(next >= h + eng.hyphen / 2.0 - 1e-4, "{width} {j}: {h} {next}");
                        }
                    }
                }
            }
        }
        assert!(steps > 0);
    }

    #[test]
    fn breaks_prefer_bars_then_words() {
        let style = StyleOptions {
            initial: Initial::None,
            ..StyleOptions::default()
        };
        let eng = parse("(c4) Dó(g)mi(h)ne(g) (;) Lord(g) <sp>V/</sp>.(:) Ple(g)ni(h) (::)")
            .score
            .engrave(&ApproxMeasure, &style);
        let opts = LayoutOptions::default();
        let cost = |text: &str| {
            let k = eng
                .segments
                .iter()
                .rposition(|s| s.lyric.as_ref().is_some_and(|t| t.runs[0].text == text))
                .unwrap();
            eng.line_end(k, &opts).1
        };
        assert_eq!(cost("Dó"), 0.0);
        assert_eq!(cost("ne"), WORD_END_DEMERITS);
        assert_eq!(eng.line_end(3, &opts).1, AFTER_BAR_DEMERITS);
        // A versicle sign under a bar leads into its verse.
        assert_eq!(cost("℣"), 0.0);
    }

    /// How often lines break inside a word, and how far justified lines stretch, over the
    /// reference scores at two widths: (mid-word breaks, breaks, mean stretch of a word gap,
    /// lines stretched more than a staff space a gap), counting the breaks the breaker chose.
    #[cfg(feature = "fonts")]
    fn break_stats() -> (usize, usize, f32, usize) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut paths: Vec<_> = ["tests/golden", "tests/corpus", "../../examples/compline"]
            .iter()
            .flat_map(|d| std::fs::read_dir(dir.join(d)).expect("reference scores"))
            .map(|e| e.expect("entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "gabc"))
            .collect();
        paths.sort();
        let (mut mid, mut breaks, mut stretch, mut loose) = (0, 0, 0.0f32, 0);
        for p in paths {
            let src = std::fs::read_to_string(&p).expect("score");
            let eng = parse(&src).score.engrave(crate::Font::Google.table(), &StyleOptions::default());
            for width in [500.0, 800.0] {
                let layout = eng.layout(width, &LayoutOptions::default());
                let (_, justified) = layout.lines.split_last().expect("a line");
                // A line before a break the score asks for was not the breaker's choice.
                let chosen = justified
                    .iter()
                    .filter(|l| !matches!(eng.segments[l.last].after, Break::Forced { .. }));
                for line in chosen {
                    breaks += 1;
                    mid += usize::from(line.hyphen.is_some());
                    stretch += line.stretch;
                    loose += usize::from(line.stretch > 1.0);
                }
            }
        }
        (mid, breaks, stretch / breaks as f32, loose)
    }

    #[test]
    #[cfg(feature = "fonts")]
    fn lines_break_between_words_where_that_costs_little() {
        // GregorioTeX breaks about a quarter of its lines inside a word across GregoBase. A
        // breaker blind to words breaks some 40% of these inside one, and one that holds out
        // for words and bars at any cost hardly ever does, and leaves loose lines.
        let (mid, breaks, stretch, loose) = break_stats();
        let mid = mid as f32 / breaks as f32;
        let loose = loose as f32 / breaks as f32;
        assert!((0.15..0.35).contains(&mid), "{mid} of breaks inside a word");
        assert!(stretch < 0.45, "word gaps stretch {stretch} on average");
        assert!(loose < 0.1, "{loose} of lines stretch more than a staff space a gap");
    }

    #[test]
    fn the_cache_compares_what_a_break_costs() {
        // A break after the syllable with no text `(hg)` ends a word or not as the next one,
        // also with no text, starts one; nothing else the breaker reads of `(hg)` tells.
        let style = StyleOptions {
            initial: Initial::None,
            ..StyleOptions::default()
        };
        let fits = |src: &str| parse(src).score.engrave(&ApproxMeasure, &style).fits();
        let (apart, joined) = (fits("(c4) a(g) (hg) (fg) c(g) (::)"), fits("(c4) a(g) (hg)(fg) c(g) (::)"));
        assert_eq!(apart.len(), joined.len());
        let k = 1;
        assert_eq!((apart[k].cost, joined[k].cost), (WORD_END_DEMERITS, 0.0));
        assert!(apart[k] != joined[k]);
        // The cost is all that tells them apart.
        let same_cost = Fit {
            cost: apart[k].cost,
            ..joined[k]
        };
        assert!(apart[k] == same_cost);
    }

    #[test]
    fn a_stuck_line_takes_the_break_it_ends_at() {
        // A line stuck behind forbidden breaks ends before the segment that overflowed, so its
        // cost and raggedness are those of the segment it ends after, not the overflowing one.
        let style = StyleOptions {
            initial: Initial::None,
            ..StyleOptions::default()
        };
        let mut eng = parse("(c4) a(g) b(h) c(g)").score.engrave(&ApproxMeasure, &style);
        let opts = LayoutOptions::default();
        let n = eng.segments.len();
        eng.segments[n - 3].after = Break::Forbidden;
        eng.segments[n - 2].after = Break::InMelisma;
        assert_eq!(eng.line_end(n - 3, &opts), (false, 0.0));
        assert_eq!(eng.line_end(n - 2, &opts), (false, MELISMA_DEMERITS));
        assert_eq!(eng.line_end(n - 1, &opts), (true, WORD_END_DEMERITS));
        // Laid out narrower than the first two syllables, the line is stuck at `b` and ends
        // after `a`; the breaker still sets every syllable.
        let layout = eng.layout(1.0, &opts);
        assert_eq!(layout.lines.last().map(|l| l.last), Some(n - 1));
    }
}
