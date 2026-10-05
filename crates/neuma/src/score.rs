//! The one score model. The GABC parser, the psalm-tone engine and [`ScoreBuilder`] all produce
//! a [`Score`], and [`Score::to_gabc`] writes one back out.

use std::ops::Range;

/// Header fields in source order. Names keep their spelling; lookups ignore ASCII case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub fields: Vec<(String, String)>,
}

impl Header {
    /// The first value for `name`.
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Score {
    pub header: Header,
    pub syllables: Vec<Syllable>,
}

/// One syllable of text with the notation sung on it. A syllable may have no text (a clef or
/// bar on its own) and no notation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Syllable {
    pub text: Lyric,
    /// Whether this syllable starts a new word (the source had a space before it).
    pub word_start: bool,
    pub notation: Vec<Figure>,
    /// Source span of the whole syllable, text through closing parenthesis.
    pub span: Range<usize>,
    /// The syllable lies inside `<nlba>…</nlba>`: no line break before it.
    pub no_break_before: bool,
    /// Inside `<eu>…</eu>`.
    pub euouae: bool,
}

/// Lyric text as styled runs, plus an optional forced centering range (from `{…}`), in chars of
/// the concatenated plain text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lyric {
    pub runs: Vec<LyricRun>,
    pub center: Option<Range<usize>>,
}

impl Lyric {
    pub fn plain(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.is_empty())
    }

    pub fn from_plain(text: &str) -> Lyric {
        if text.is_empty() {
            return Lyric::default();
        }
        Lyric {
            runs: vec![LyricRun {
                text: text.to_string(),
                style: TextStyle::default(),
                consonant: false,
            }],
            center: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LyricRun {
    pub text: String,
    pub style: TextStyle,
    /// Counts as consonants for vowel finding (special characters, elisions).
    pub consonant: bool,
}

/// How a run of text is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextStyle {
    pub italic: bool,
    pub bold: bool,
    pub small_caps: bool,
    pub underline: bool,
    /// Rubric color (`<c>`, special characters, annotations).
    pub rubric: bool,
}

impl TextStyle {
    pub const REGULAR: TextStyle = TextStyle {
        italic: false,
        bold: false,
        small_caps: false,
        underline: false,
        rubric: false,
    };

    /// The font face this style is drawn in (rubric and underline don't change the face).
    pub fn face(self) -> Face {
        match (self.bold, self.italic) {
            (false, false) => Face::Regular,
            (false, true) => Face::Italic,
            (true, false) => Face::Bold,
            (true, true) => Face::BoldItalic,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Face {
    Regular,
    Italic,
    Bold,
    BoldItalic,
}

/// One item of notation, in source order.
#[derive(Clone, Debug, PartialEq)]
pub enum Figure {
    Clef(Clef),
    Note(Note),
    Alteration(Alteration),
    Space(Space),
    Bar(Bar),
    /// `z`/`Z`: a forced line break.
    Break(LineBreak),
    /// `g+` (a custos at that pitch) or `z0` (pitch of the next note).
    Custos {
        position: Option<i8>,
        span: Range<usize>,
    },
    /// `[nocustos]`: no custos if the line breaks here.
    NoCustos,
}

/// Staff position: 0 is the middle space of a four-line staff, lines are at −3, −1, 1 and 3,
/// and each step is a line or a space. GABC's `a` is −6 and `m` is 6.
pub type StaffPosition = i8;

/// The staff position of a GABC pitch letter (`a`–`m`, `n`, `p`), case-insensitive.
pub fn pitch_position(letter: char) -> Option<StaffPosition> {
    let l = letter.to_ascii_lowercase();
    match l {
        'a'..='n' => Some(l as i8 - b'a' as i8 - 6),
        'p' => Some(b'o' as i8 - b'a' as i8 - 6),
        _ => None,
    }
}

/// The GABC letter for a staff position, if it has one.
pub fn position_letter(position: StaffPosition) -> Option<char> {
    let i = position + 6;
    match i {
        0..=13 => Some((b'a' + i as u8) as char),
        14 => Some('p'),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClefKind {
    Do,
    Fa,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clef {
    pub kind: ClefKind,
    /// Staff line, 1 (bottom) to 4.
    pub line: u8,
    /// `cb3`: a key flat on B.
    pub flat: bool,
    pub span: Range<usize>,
}

impl Clef {
    /// Staff position of the clef's line.
    pub fn position(&self) -> StaffPosition {
        2 * self.line as i8 - 5
    }

    /// Diatonic steps from the clef's reference note (do or fa) to `position`, counting
    /// upward.
    pub fn steps_from_reference(&self, position: StaffPosition) -> i32 {
        position as i32 - self.position() as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteShape {
    Punctum,
    Inclinatum,
    Virga,
    VirgaReversa,
    Quilisma,
    Oriscus,
    OriscusScapus,
    Stropha,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Liquescent {
    #[default]
    None,
    /// `~`
    Deminutus,
    /// `<`
    Augmented,
    /// `>`
    Diminished,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    #[default]
    Auto,
    Below,
    Above,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub position: StaffPosition,
    pub shape: NoteShape,
    pub liquescent: Liquescent,
    pub initio_debilis: bool,
    pub cavum: bool,
    /// Quadratum (`q`, `W`): a square form of the pes or quilisma.
    pub quadratum: bool,
    /// Inclinatum lean (`G0` `G1` `G2`); `None` means automatic.
    pub lean: Option<u8>,
    /// Oriscus orientation (`o0` down, `o1` up); `None` means automatic.
    pub orientation: Option<bool>,
    pub morae: u8,
    pub mora_placement: Placement,
    pub episema: Option<Episema>,
    pub ictus: Option<Placement>,
    /// Signs above the staff (`r1`–`r8`), kept for later milestones.
    pub above_sign: Option<u8>,
    pub span: Range<usize>,
}

impl Note {
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

    pub fn is_liquescent(&self) -> bool {
        self.liquescent != Liquescent::None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Episema {
    pub placement: Placement,
    /// `_2`: don't bridge to the next episema.
    pub no_bridge: bool,
    /// `_3` `_4` `_5`: a small episema aligned left, center or right.
    pub small: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlterationKind {
    Flat,
    Natural,
    Sharp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alteration {
    pub position: StaffPosition,
    pub kind: AlterationKind,
    /// Soft (`X` `##` `Y`): drawn only when it changes the pitch in effect.
    pub soft: bool,
    /// Parenthesized (`x?`).
    pub parenthesized: bool,
    pub span: Range<usize>,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BarKind {
    /// `` ` ``
    Virgula,
    /// `^`
    Minimis,
    /// `,`
    Minima,
    /// `;`
    Minor,
    /// `:`
    Maior,
    /// `:?`
    DottedMaior,
    /// `::`
    Finalis,
    /// `;1`–`;8`
    Dominican(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    pub kind: BarKind,
    /// `` `0 `` `,0` `^0`: drawn on the ledger line above the staff.
    pub high: bool,
    pub span: Range<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CustosRule {
    #[default]
    Default,
    Force,
    Suppress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineBreak {
    /// `z` justifies the line it ends; `Z` leaves it ragged.
    pub justify: bool,
    pub custos: CustosRule,
    pub span: Range<usize>,
}

/// Builds a [`Score`] directly, for tools that don't start from GABC (such as the psalm-tone
/// engine).
#[derive(Debug, Default)]
pub struct ScoreBuilder {
    score: Score,
}

impl ScoreBuilder {
    pub fn new() -> ScoreBuilder {
        ScoreBuilder::default()
    }

    pub fn header(mut self, name: &str, value: &str) -> ScoreBuilder {
        self.score.header.fields.push((name.to_string(), value.to_string()));
        self
    }

    /// Adds a syllable. `word_start` is true for the first syllable of each word.
    pub fn syllable(mut self, text: Lyric, word_start: bool, notation: Vec<Figure>) -> ScoreBuilder {
        self.score.syllables.push(Syllable {
            text,
            word_start,
            notation,
            ..Syllable::default()
        });
        self
    }

    /// Adds a syllable of plain text sung on single puncta at `positions`.
    pub fn sung(self, text: &str, word_start: bool, positions: &[StaffPosition]) -> ScoreBuilder {
        let notation = positions
            .iter()
            .map(|&p| Figure::Note(Note::new(p, NoteShape::Punctum, 0..0)))
            .collect();
        self.syllable(Lyric::from_plain(text), word_start, notation)
    }

    pub fn clef(self, kind: ClefKind, line: u8) -> ScoreBuilder {
        self.syllable(
            Lyric::default(),
            true,
            vec![Figure::Clef(Clef {
                kind,
                line,
                flat: false,
                span: 0..0,
            })],
        )
    }

    pub fn bar(self, kind: BarKind) -> ScoreBuilder {
        self.syllable(
            Lyric::default(),
            true,
            vec![Figure::Bar(Bar {
                kind,
                high: false,
                span: 0..0,
            })],
        )
    }

    pub fn build(self) -> Score {
        self.score
    }
}
