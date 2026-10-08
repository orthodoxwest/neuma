//! The one score model. The GABC parser, the psalm-tone engine and [`ScoreBuilder`] all produce
//! a [`Score`], and [`Score::to_gabc`] writes one back out.

use std::ops::Range;

/// Header fields in source order. Names keep their spelling; lookups ignore ASCII case.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Header {
    /// Each field's name and value, in source order. A name may appear more than once.
    pub fields: Vec<(String, String)>,
    /// Each parsed field's source span, parallel to `fields`; not part of equality.
    pub(crate) spans: Vec<Range<usize>>,
}

impl PartialEq for Header {
    fn eq(&self, other: &Header) -> bool {
        self.fields == other.fields
    }
}

impl Eq for Header {}

impl Header {
    /// An empty header.
    #[must_use]
    pub fn new() -> Header {
        Header::default()
    }

    /// Adds a field after the others.
    pub fn push(&mut self, name: &str, value: &str) {
        self.fields.push((name.to_string(), value.to_string()));
    }

    /// The source span of the first field named `name`, from its name through its closing
    /// `;`, for a header read from GABC.
    #[must_use]
    pub fn span(&self, name: &str) -> Option<Range<usize>> {
        let i = self.fields.iter().position(|(n, _)| n.eq_ignore_ascii_case(name))?;
        self.spans.get(i).cloned()
    }

    /// The first value for `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Every value for `name`, in order (`annotation` may appear twice).
    pub fn get_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.fields
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// The model's structs are `#[non_exhaustive]`: build them with their constructors (or
/// [`ScoreBuilder`]) and set the public fields after.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Score {
    /// The header fields above the `%%` line.
    pub header: Header,
    /// The syllables in order, each with its text and the notation sung on it. The clef at
    /// the start is a syllable without text.
    pub syllables: Vec<Syllable>,
}

impl Score {
    /// A score of these header fields and syllables.
    #[must_use]
    pub fn new(header: Header, syllables: Vec<Syllable>) -> Score {
        Score { header, syllables }
    }
}

/// One syllable of text with the notation sung on it. A syllable may have no text (a clef or
/// bar on its own) and no notation.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Syllable {
    /// The syllable's text, empty for a clef or bar on its own.
    pub text: Lyric,
    /// Whether this syllable starts a new word (the source had a space before it).
    pub word_start: bool,
    /// What is in the syllable's parentheses: notes, clefs, bars, spaces and the rest.
    pub notation: Vec<Figure>,
    /// Source span of the whole syllable, text through closing parenthesis.
    pub span: Range<usize>,
    /// The syllable lies inside `<nlba>…</nlba>`: no line break before it.
    pub no_break_before: bool,
    /// The syllable's notes lie inside `<nlba>…</nlba>`: no line break among them, even when
    /// the region holds only this syllable.
    pub no_break_within: bool,
    /// Inside `<eu>…</eu>`.
    pub euouae: bool,
}

impl Syllable {
    /// A syllable with no source span and no `<nlba>` or `<eu>` marks.
    #[must_use]
    pub fn new(text: Lyric, word_start: bool, notation: Vec<Figure>) -> Syllable {
        Syllable {
            text,
            word_start,
            notation,
            ..Syllable::default()
        }
    }
}

/// Lyric text as styled runs, plus an optional forced centering range (from `{…}`), in chars of
/// the concatenated plain text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Lyric {
    /// The text, in runs of one style each.
    pub runs: Vec<LyricRun>,
    /// The characters the notes are centered over, from `{…}`, in chars of the plain text;
    /// `None` centers on the vowel.
    pub center: Option<Range<usize>>,
}

impl Lyric {
    /// A lyric of these runs, centered on `center` if given.
    #[must_use]
    pub fn new(runs: Vec<LyricRun>, center: Option<Range<usize>>) -> Lyric {
        Lyric { runs, center }
    }

    /// The text without its styles.
    #[must_use]
    pub fn plain(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Whether there is no text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.is_empty())
    }

    /// Unstyled text, as one run (or none, for empty text).
    #[must_use]
    pub fn from_plain(text: &str) -> Lyric {
        if text.is_empty() {
            return Lyric::default();
        }
        Lyric {
            runs: vec![LyricRun::new(text, TextStyle::default())],
            center: None,
        }
    }
}

/// A run of lyric text in one style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LyricRun {
    /// The text.
    pub text: String,
    /// How it is set.
    pub style: TextStyle,
    /// Counts as consonants for vowel finding (special characters, elisions).
    pub consonant: bool,
}

impl LyricRun {
    /// A run of `text` in `style` whose letters count for vowel finding.
    #[must_use]
    pub fn new(text: &str, style: TextStyle) -> LyricRun {
        LyricRun {
            text: text.to_string(),
            style,
            consonant: false,
        }
    }
}

/// How a run of text is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextStyle {
    /// Italic (`<i>`).
    pub italic: bool,
    /// Bold (`<b>`).
    pub bold: bool,
    /// Small capitals (`<sc>`), from the face's `smcp` feature.
    pub small_caps: bool,
    /// Underlined (`<ul>`).
    pub underline: bool,
    /// Rubric color (`<c>`, special characters, annotations).
    pub rubric: bool,
}

impl TextStyle {
    /// Upright, regular weight, no decoration.
    pub const REGULAR: TextStyle = TextStyle {
        italic: false,
        bold: false,
        small_caps: false,
        underline: false,
        rubric: false,
    };

    /// The font face this style is drawn in (rubric and underline don't change the face).
    #[must_use]
    pub fn face(self) -> Face {
        match (self.bold, self.italic) {
            (false, false) => Face::Regular,
            (false, true) => Face::Italic,
            (true, false) => Face::Bold,
            (true, true) => Face::BoldItalic,
        }
    }
}

/// One of the four faces of a lyric font.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Face {
    /// Upright, regular weight.
    Regular,
    /// Italic, regular weight.
    Italic,
    /// Upright, bold.
    Bold,
    /// Italic, bold.
    BoldItalic,
}

/// One item of notation, in source order.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Figure {
    /// A clef, at the start or changing mid-line.
    Clef(Clef),
    /// A note.
    Note(Note),
    /// A flat, natural or sharp sign.
    Alteration(Alteration),
    /// A space between neumes, or a cut that joins nothing.
    Space(Space),
    /// A bar (divisio).
    Bar(Bar),
    /// `z`/`Z`: a forced line break.
    Break(LineBreak),
    /// `g+` (a custos at that pitch) or `z0` (pitch of the next note).
    Custos {
        /// The custos's staff position, or `None` for that of the next note.
        position: Option<i8>,
        /// Where it is in the source.
        span: Range<usize>,
    },
    /// `[nocustos]`: no custos if the line breaks here.
    NoCustos,
}

/// Staff position: 0 is the middle space of a four-line staff, lines are at −3, −1, 1 and 3,
/// and each step is a line or a space. GABC's `a` is −6 and `m` is 6.
pub type StaffPosition = i8;

/// The staff position of a GABC pitch letter (`a`–`m`, `n`, `p`), case-insensitive.
#[must_use]
pub fn pitch_position(letter: char) -> Option<StaffPosition> {
    let l = letter.to_ascii_lowercase();
    match l {
        'a'..='n' => Some(l as i8 - b'a' as i8 - 6),
        'p' => Some(b'o' as i8 - b'a' as i8 - 6),
        _ => None,
    }
}

/// The GABC letter for a staff position, if it has one.
#[must_use]
pub fn position_letter(position: StaffPosition) -> Option<char> {
    let i = position + 6;
    match i {
        0..=13 => Some((b'a' + i as u8) as char),
        14 => Some('p'),
        _ => None,
    }
}

/// Which note a clef marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClefKind {
    /// A do clef (`c`).
    Do,
    /// A fa clef (`f`).
    Fa,
}

/// A clef: `c1`–`c4`, `f1`–`f4`, and `cb1`–`cb4` with a key flat.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Clef {
    /// Do or fa.
    pub kind: ClefKind,
    /// Staff line, 1 (bottom) to 4.
    pub line: u8,
    /// `cb3`: a key flat on B.
    pub flat: bool,
    /// Where it is in the source.
    pub span: Range<usize>,
}

impl Clef {
    /// A clef on staff line `line` (1, the bottom, to 4), with no key flat.
    #[must_use]
    pub fn new(kind: ClefKind, line: u8, span: Range<usize>) -> Clef {
        Clef {
            kind,
            line,
            flat: false,
            span,
        }
    }

    /// Staff position of the clef's line.
    #[must_use]
    pub fn position(&self) -> StaffPosition {
        2 * self.line as i8 - 5
    }

    /// Diatonic steps from the clef's reference note (do or fa) to `position`, counting
    /// upward.
    #[must_use]
    pub fn steps_from_reference(&self, position: StaffPosition) -> i32 {
        position as i32 - self.position() as i32
    }
}

/// A note's basic shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NoteShape {
    /// The square punctum (a pitch letter alone).
    Punctum,
    /// The diamond punctum inclinatum (an upper-case letter).
    Inclinatum,
    /// The virga, with its stem on the right (`v`).
    Virga,
    /// The virga reversa, with its stem on the left (`V`).
    VirgaReversa,
    /// The quilisma (`w`).
    Quilisma,
    /// The oriscus (`o`).
    Oriscus,
    /// The oriscus with a stem, the scapus (`O`).
    OriscusScapus,
    /// The stropha (`s`).
    Stropha,
}

/// A note's liquescence, as GABC writes it after the pitch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Liquescent {
    /// Not liquescent.
    #[default]
    None,
    /// `~`
    Deminutus,
    /// `<`
    Augmented,
    /// `>`
    Diminished,
}

/// Where a sign goes relative to its note: a mora, an episema, an ictus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    /// Where the engine puts it by default.
    #[default]
    Auto,
    /// Below the note (a `0` suffix).
    Below,
    /// Above the note (a `1` suffix).
    Above,
}

/// One note, with the signs on it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Note {
    /// Its staff position, from its pitch letter.
    pub position: StaffPosition,
    /// Its shape.
    pub shape: NoteShape,
    /// Its liquescence.
    pub liquescent: Liquescent,
    /// An initio debilis (`-` before the pitch): the first note of a neume, made small.
    pub initio_debilis: bool,
    /// A hollow notehead (`r`).
    pub cavum: bool,
    /// Quadratum (`q`, `W`): a square form of the pes or quilisma.
    pub quadratum: bool,
    /// Inclinatum lean (`G0` `G1` `G2`); `None` means automatic.
    pub lean: Option<u8>,
    /// Oriscus orientation (`o0` down, `o1` up); `None` means automatic.
    pub orientation: Option<bool>,
    /// The number of morae (dots) after it: `.` one, `..` two.
    pub morae: u8,
    /// Where its morae go.
    pub mora_placement: Placement,
    /// A horizontal episema (`_`).
    pub episema: Option<Episema>,
    /// A vertical episema, the ictus (`'`), and where it goes.
    pub ictus: Option<Placement>,
    /// Signs above the staff (`r1`–`r8`): read, and reported with `gabc::above-sign`, but not
    /// drawn.
    pub above_sign: Option<u8>,
    /// Where it is in the source.
    pub span: Range<usize>,
}

impl Note {
    /// A note of `shape` at `position`, with no signs on it.
    #[must_use]
    pub fn new(position: StaffPosition, shape: NoteShape, span: Range<usize>) -> Note {
        Note {
            position,
            shape,
            liquescent: Liquescent::None,
            initio_debilis: false,
            cavum: false,
            quadratum: false,
            lean: None,
            orientation: None,
            morae: 0,
            mora_placement: Placement::Auto,
            episema: None,
            ictus: None,
            above_sign: None,
            span,
        }
    }

    /// Whether the note is liquescent in any way.
    #[must_use]
    pub fn is_liquescent(&self) -> bool {
        self.liquescent != Liquescent::None
    }
}

/// A horizontal episema over or under a note.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Episema {
    /// Where it goes.
    pub placement: Placement,
    /// `_2`: don't bridge to the next episema.
    pub no_bridge: bool,
    /// `_3` `_4` `_5`: a small episema aligned left, center or right.
    pub small: Option<u8>,
}

impl Episema {
    /// A full-width episema that bridges to the next.
    #[must_use]
    pub fn new(placement: Placement) -> Episema {
        Episema {
            placement,
            ..Episema::default()
        }
    }
}

/// Which accidental an alteration is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlterationKind {
    /// A flat (`x`).
    Flat,
    /// A natural (`y`).
    Natural,
    /// A sharp (`#`).
    Sharp,
}

/// An accidental before a note, at a staff position (`ix`, `iy`, `i#`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alteration {
    /// The staff position it alters.
    pub position: StaffPosition,
    /// Flat, natural or sharp.
    pub kind: AlterationKind,
    /// Soft (`X` `##` `Y`): drawn only when it changes the pitch in effect.
    pub soft: bool,
    /// Parenthesized (`x?`).
    pub parenthesized: bool,
    /// Where it is in the source.
    pub span: Range<usize>,
}

/// A space between neumes, as GABC writes it inside the notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Space {
    /// `!` alone: split neumes with no space.
    Zero,
    /// `/!`
    Tiny,
    /// `/0`
    Half,
    /// `/`
    Small,
    /// `//`
    Medium,
    /// A space inside the notes: the large separation, and a break opportunity.
    Large,
    /// `!` followed by a space: the large separation without a break.
    LargeNoBreak,
    /// `/[f]`: the large separation scaled by `f` (negative is a backspace).
    Scaled(f32),
}

/// A bar (divisio), named as GABC and Gregorio name it. The timeline's weights, the JSON and
/// the bindings use the English names given here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BarKind {
    /// `` ` ``
    Virgula,
    /// `^`
    Minimis,
    /// `,`: the quarter bar (divisio minima).
    Quarter,
    /// `;`: the half bar (divisio minor).
    Half,
    /// `:`: the full bar (divisio maior).
    Full,
    /// `:?`: the dotted full bar.
    DottedFull,
    /// `::`: the double bar (divisio finalis).
    Double,
    /// `;1`–`;8`
    Dominican(u8),
}

/// A bar in the notes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Bar {
    /// Which bar it is.
    pub kind: BarKind,
    /// `` `0 `` `,0` `^0`: drawn on the ledger line above the staff.
    pub high: bool,
    /// Where it is in the source.
    pub span: Range<usize>,
}

impl Bar {
    /// A bar at its usual height.
    #[must_use]
    pub fn new(kind: BarKind, span: Range<usize>) -> Bar {
        Bar { kind, high: false, span }
    }
}

/// Whether a written line break ends its line with a custos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CustosRule {
    /// As the layout's custos policy says (`z`, `Z`).
    #[default]
    Default,
    /// Always (`z+`, `Z+`).
    Force,
    /// Never (`z-`, `Z-`).
    Suppress,
}

/// A line break written in the notes: `z`, `Z` and their custos suffixes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineBreak {
    /// `z` justifies the line it ends; `Z` leaves it ragged.
    pub justify: bool,
    /// Whether the line ends with a custos.
    pub custos: CustosRule,
    /// Where it is in the source.
    pub span: Range<usize>,
}

/// Builds a [`Score`] directly, for tools that don't start from GABC (such as the psalm-tone
/// engine).
#[derive(Debug, Default)]
pub struct ScoreBuilder {
    score: Score,
}

impl ScoreBuilder {
    /// An empty score.
    #[must_use]
    pub fn new() -> ScoreBuilder {
        ScoreBuilder::default()
    }

    /// Adds a header field after the others.
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> ScoreBuilder {
        self.score.header.push(name, value);
        self
    }

    /// Adds a syllable. `word_start` is true for the first syllable of each word.
    #[must_use]
    pub fn syllable(mut self, text: Lyric, word_start: bool, notation: Vec<Figure>) -> ScoreBuilder {
        self.score.syllables.push(Syllable::new(text, word_start, notation));
        self
    }

    /// Adds a syllable of plain text sung on single puncta at `positions`.
    #[must_use]
    pub fn sung(self, text: &str, word_start: bool, positions: &[StaffPosition]) -> ScoreBuilder {
        let notation = positions
            .iter()
            .map(|&p| Figure::Note(Note::new(p, NoteShape::Punctum, 0..0)))
            .collect();
        self.syllable(Lyric::from_plain(text), word_start, notation)
    }

    /// Adds a syllable holding only a clef on staff line `line` (1, the bottom, to 4).
    #[must_use]
    pub fn clef(self, kind: ClefKind, line: u8) -> ScoreBuilder {
        self.syllable(Lyric::default(), true, vec![Figure::Clef(Clef::new(kind, line, 0..0))])
    }

    /// Adds a syllable holding only a bar.
    #[must_use]
    pub fn bar(self, kind: BarKind) -> ScoreBuilder {
        self.syllable(Lyric::default(), true, vec![Figure::Bar(Bar::new(kind, 0..0))])
    }

    /// The score built.
    #[must_use]
    pub fn build(self) -> Score {
        self.score
    }
}
