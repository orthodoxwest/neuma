//! neuma for iOS and Android: a [`Chant`] engraves a score once and lays it out at any
//! width, returning a display list to draw natively and the playback timeline.
//!
//! The bindings are UniFFI (namespace `neuma`). An app links this crate on its own, or
//! depends on it from its own UniFFI crate and generates bindings for both in library mode.
//! Glyphs cross as `u16` ids: fetch each outline once with [`glyph_outline`] and draw it at
//! an item's position and scale. Lyrics are drawn with the app's EB Garamond, ligatures off.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use neuma::{Engraving, Font as EngineFont, Initial, StyleOptions};

uniffi::setup_scaffolding!("neuma");

/// Which EB Garamond the app draws lyrics with, so they are measured as drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum Font {
    /// The EB Garamond 12 release (the OTFs most apps bundle).
    #[default]
    Garamond12,
    /// The version Google Fonts serves.
    Google,
}

impl From<Font> for EngineFont {
    fn from(f: Font) -> EngineFont {
        match f {
            Font::Garamond12 => EngineFont::Garamond12,
            Font::Google => EngineFont::Google,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ChantOptions {
    /// Drop-cap height in staves, 0 to 4; 0 for none.
    #[uniffi(default = 1)]
    pub initial: u8,
    /// Show the annotation (or the mode) above the initial.
    #[uniffi(default = true)]
    pub annotation: bool,
    /// Lyric size in staff spaces; 0 or less keeps the default (2.7).
    #[uniffi(default = 0.0)]
    pub lyric_size: f32,
    pub font: Font,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LastLine {
    Ragged,
    Justified,
}

/// Relative durations per sign, in the timeline's weight units. Not beats: the app picks
/// the tempo. [`default_weights`] gives one pulse a note and pauses that grow with the bar.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Weights {
    #[uniffi(default = 1.0)]
    pub note: f32,
    /// A dotted note.
    #[uniffi(default = 2.0)]
    pub mora: f32,
    #[uniffi(default = 1.5)]
    pub episema: f32,
    #[uniffi(default = 0.5)]
    pub virgula: f32,
    #[uniffi(default = 0.5)]
    pub quarter: f32,
    #[uniffi(default = 1.0)]
    pub half: f32,
    #[uniffi(default = 2.0)]
    pub full: f32,
    #[uniffi(default = 3.0)]
    pub double: f32,
    #[uniffi(default = 2.0)]
    pub mediant: f32,
    #[uniffi(default = 1.0)]
    pub flex: f32,
}

impl From<Weights> for neuma::Weights {
    fn from(w: Weights) -> neuma::Weights {
        let d = neuma::Weights::SOLESMES;
        let keep = |v: f32, default: f32| if v.is_finite() && v >= 0.0 { v } else { default };
        neuma::Weights {
            note: keep(w.note, d.note),
            mora: keep(w.mora, d.mora),
            episema: keep(w.episema, d.episema),
            virgula: keep(w.virgula, d.virgula),
            minima: keep(w.quarter, d.minima),
            minor: keep(w.half, d.minor),
            maior: keep(w.full, d.maior),
            finalis: keep(w.double, d.finalis),
            mediant: keep(w.mediant, d.mediant),
            flex: keep(w.flex, d.flex),
        }
    }
}

/// The default weights (the same as a `Weights` built with no arguments).
#[uniffi::export]
pub fn default_weights() -> Weights {
    let d = neuma::Weights::SOLESMES;
    Weights {
        note: d.note,
        mora: d.mora,
        episema: d.episema,
        virgula: d.virgula,
        quarter: d.minima,
        half: d.minor,
        full: d.maior,
        double: d.finalis,
        mediant: d.mediant,
        flex: d.flex,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct LayoutOptions {
    /// Points (or pixels) per staff space; 0 or less keeps the default (6).
    #[uniffi(default = 0.0)]
    pub scale: f32,
    pub last_line: LastLine,
    pub weights: Weights,
}

/// The default score options: a one-staff initial with its annotation, EB Garamond 12.
#[uniffi::export]
pub fn default_chant_options() -> ChantOptions {
    ChantOptions {
        initial: 1,
        annotation: true,
        lyric_size: 0.0,
        font: Font::Garamond12,
    }
}

/// The default layout options: 6 units per staff space, a ragged last line, default weights.
#[uniffi::export]
pub fn default_layout_options() -> LayoutOptions {
    LayoutOptions {
        scale: 0.0,
        last_line: LastLine::Ragged,
        weights: default_weights(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A problem found while reading the score.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Diagnostic {
    pub severity: Severity,
    /// UTF-8 byte range in the source.
    pub start: u64,
    pub end: u64,
    /// A stable code such as `gabc::hyphen-in-syllable`.
    pub code: String,
    pub message: String,
}

/// What a piece of ink is, so a theme can color it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TextRole {
    Lyric,
    Hyphen,
    Initial,
    Annotation,
    Rubric,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TextRun {
    pub text: String,
    pub italic: bool,
    pub bold: bool,
    pub small_caps: bool,
    pub underline: bool,
    /// Drawn in the rubric color.
    pub rubric: bool,
}

/// One thing to draw, in output units with y down.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Item {
    /// The outline [`glyph_outline`] returns for `glyph`, drawn with its origin at (x, y)
    /// and scaled by `scale`.
    Glyph {
        glyph: u16,
        x: f32,
        y: f32,
        scale: f32,
        role: Ink,
        /// The notes this ink draws: none, one, or two for a porrectus swash.
        notes: Vec<u32>,
    },
    /// A filled rectangle: staff and ledger lines, stems, bars, episemata.
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        role: Ink,
        notes: Vec<u32>,
    },
    /// Text starting at x on `baseline`, `size` units high (the font size).
    Text {
        x: f32,
        baseline: f32,
        size: f32,
        runs: Vec<TextRun>,
        role: TextRole,
        syllable: Option<u32>,
    },
}

/// One staff's box, in output units.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct LineBox {
    pub top: f32,
    pub bottom: f32,
    /// y of the staff's middle line.
    pub staff: f32,
    /// y of the lyrics' baseline.
    pub baseline: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
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

/// One note of the timeline, in singing order.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Note {
    /// Stable across layouts of one [`Chant`].
    pub id: u32,
    pub syllable: u32,
    pub word: u32,
    pub line: u32,
    /// The notehead's center and size, in output units.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// When the note starts and how long it lasts, in weight units.
    pub start: f32,
    pub duration: f32,
    pub staff_position: i8,
    /// Diatonic steps above the clef's do.
    pub degree: i32,
    /// Semitones above the clef's do, flats applied.
    pub semitones: i16,
    pub syllable_text: String,
    /// The vowel the syllable is centered on, if any.
    pub vowel: Option<String>,
    pub shape: NoteShape,
    pub liquescent: bool,
    pub quilisma: bool,
    /// The syllable has an acute accent in the source.
    pub accent: bool,
    /// The first note of its syllable.
    pub new_syllable: bool,
    /// Inferred: part of a run of three or more single-note syllables on one pitch.
    pub recitation: bool,
    /// Advances after each full or double bar.
    pub verse: u32,
    /// 1 after the verse's mediant `*`, else 0.
    pub half: u8,
    /// UTF-8 byte range of the note in the source.
    pub span_start: u64,
    pub span_end: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PauseKind {
    Virgula,
    Minimis,
    Quarter,
    Half,
    Full,
    DottedFull,
    Double,
    Dominican,
    /// `*`.
    Mediant,
    /// `†`.
    Flex,
}

/// A pause before note `before_note` (the note count means after the last note). A mediant
/// or flex is the whole pause at its bar: the bar right after it has weight 0.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Pause {
    pub before_note: u32,
    pub kind: PauseKind,
    pub weight: f32,
    pub start: f32,
}

/// A layout at one width: what to draw, and when each note sounds.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    /// One staff space, in output units.
    pub staff_space: f32,
    pub items: Vec<Item>,
    pub lines: Vec<LineBox>,
    pub notes: Vec<Note>,
    pub pauses: Vec<Pause>,
    /// The timeline's total length, in weight units.
    pub duration: f32,
    /// The lyrics as plain text, for the accessibility label.
    pub alt_text: String,
}

/// A glyph's outline: absolute `M L C Z` path data with nonzero fill, in the units an
/// item's `scale` converts from.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct GlyphOutline {
    pub path: String,
    /// The glyph's width in staff spaces.
    pub width: f32,
}

/// The outline for an [`Item::Glyph`]'s id, or `None` for an unknown id.
#[uniffi::export]
pub fn glyph_outline(id: u16) -> Option<GlyphOutline> {
    neuma::glyph_outline(id).map(|g| GlyphOutline { path: g.d, width: g.width })
}

/// One score: engraved once, laid out on demand. Safe to share across threads.
#[derive(Debug, uniffi::Object)]
pub struct Chant {
    engraving: Engraving,
    diagnostics: Vec<Diagnostic>,
    /// The last layout's timeline, for [`Chant::note_at`].
    last: Mutex<Option<neuma::NoteMap>>,
}

#[uniffi::export]
impl Chant {
    #[uniffi::constructor]
    pub fn new(gabc: String, options: ChantOptions) -> Arc<Chant> {
        let parsed = neuma::parse(&gabc);
        let defaults = StyleOptions::default();
        let style = StyleOptions {
            initial: match options.initial.min(4) {
                0 => Initial::None,
                n => Initial::Lines(n),
            },
            annotation: options.annotation,
            lyric_size: if options.lyric_size.is_finite() && options.lyric_size > 0.0 {
                options.lyric_size
            } else {
                defaults.lyric_size
            },
            ..defaults
        };
        let engraving = parsed.score.engrave(EngineFont::from(options.font).table(), &style);
        let diagnostics = parsed
            .diagnostics
            .iter()
            .chain(&engraving.diagnostics)
            .map(|d| Diagnostic {
                severity: match d.severity {
                    neuma::Severity::Info => Severity::Info,
                    neuma::Severity::Warning => Severity::Warning,
                    neuma::Severity::Error => Severity::Error,
                },
                start: d.span.start as u64,
                end: d.span.end as u64,
                code: d.code.to_string(),
                message: d.message.clone(),
            })
            .collect();
        Arc::new(Chant {
            engraving,
            diagnostics,
            last: Mutex::new(None),
        })
    }

    /// Problems found while reading the score.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.clone()
    }

    /// Lays the score out `width` units wide.
    pub fn layout(&self, width: f32, options: LayoutOptions) -> Page {
        let opts = neuma::LayoutOptions {
            scale: if options.scale.is_finite() && options.scale > 0.0 {
                options.scale
            } else {
                neuma::LayoutOptions::default().scale
            },
            last_line: match options.last_line {
                LastLine::Ragged => neuma::LastLine::Ragged,
                LastLine::Justified => neuma::LastLine::Justified,
            },
        };
        let layout = self.engraving.layout(width, &opts);
        let list = layout.display();
        let map = layout.notes(&options.weights.into());
        let page = Page {
            width: list.width,
            height: list.height,
            staff_space: list.staff_space,
            items: list.items.into_iter().map(item).collect(),
            lines: list
                .lines
                .iter()
                .map(|l| LineBox {
                    top: l.top,
                    bottom: l.bottom,
                    staff: l.staff,
                    baseline: l.baseline,
                })
                .collect(),
            notes: map.notes.iter().map(note).collect(),
            pauses: map
                .pauses
                .iter()
                .map(|p| Pause {
                    before_note: p.before_note,
                    kind: pause_kind(p.kind),
                    weight: p.weight,
                    start: p.start,
                })
                .collect(),
            duration: map.duration,
            alt_text: list.alt_text,
        };
        *self.last_map() = Some(map);
        page
    }

    /// The note under (`x`, `y`) in the last layout, or the nearest on that line; `None`
    /// outside every line or before the first layout.
    pub fn note_at(&self, x: f32, y: f32) -> Option<u32> {
        self.last_map().as_ref()?.note_at(x, y)
    }
}

impl Chant {
    fn last_map(&self) -> MutexGuard<'_, Option<neuma::NoteMap>> {
        // The map is replaced whole, so a panic elsewhere can't leave it half-written.
        self.last.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn notes(note: Option<u32>, through: Option<u32>) -> Vec<u32> {
    match note {
        Some(first) => (first..=through.unwrap_or(first).max(first)).collect(),
        None => Vec::new(),
    }
}

fn ink(i: neuma::Ink) -> Ink {
    match i {
        neuma::Ink::Staff => Ink::Staff,
        neuma::Ink::Ledger => Ink::Ledger,
        neuma::Ink::Note => Ink::Note,
        neuma::Ink::Stem => Ink::Stem,
        neuma::Ink::Bar => Ink::Bar,
        neuma::Ink::Episema => Ink::Episema,
        neuma::Ink::Mora => Ink::Mora,
        neuma::Ink::Ictus => Ink::Ictus,
        neuma::Ink::Accidental => Ink::Accidental,
        neuma::Ink::Clef => Ink::Clef,
        neuma::Ink::Custos => Ink::Custos,
    }
}

fn item(i: neuma::Item) -> Item {
    match i {
        neuma::Item::Glyph {
            glyph,
            x,
            y,
            scale,
            role,
            note,
            through,
        } => Item::Glyph {
            glyph,
            x,
            y,
            scale,
            role: ink(role),
            notes: notes(note, through),
        },
        neuma::Item::Rect {
            x,
            y,
            w,
            h,
            role,
            note,
            through,
        } => Item::Rect {
            x,
            y,
            w,
            h,
            role: ink(role),
            notes: notes(note, through),
        },
        neuma::Item::Text {
            x,
            baseline,
            size,
            runs,
            role,
            syllable,
        } => Item::Text {
            x,
            baseline,
            size,
            runs: runs
                .into_iter()
                .map(|r| TextRun {
                    text: r.text,
                    italic: r.style.italic,
                    bold: r.style.bold,
                    small_caps: r.style.small_caps,
                    underline: r.style.underline,
                    rubric: r.style.rubric,
                })
                .collect(),
            role: match role {
                neuma::TextRole::Lyric => TextRole::Lyric,
                neuma::TextRole::Hyphen => TextRole::Hyphen,
                neuma::TextRole::Initial => TextRole::Initial,
                neuma::TextRole::Annotation => TextRole::Annotation,
                neuma::TextRole::Rubric => TextRole::Rubric,
            },
            syllable,
        },
    }
}

fn note(n: &neuma::MappedNote) -> Note {
    use neuma::score::NoteShape as S;
    Note {
        id: n.id,
        syllable: n.syllable,
        word: n.word,
        line: n.line,
        x: n.x,
        y: n.y,
        w: n.w,
        h: n.h,
        start: n.start,
        duration: n.duration,
        staff_position: n.staff_position,
        degree: n.degree,
        semitones: n.semitones,
        syllable_text: n.syllable_text.clone(),
        vowel: n.vowel.map(String::from),
        shape: match n.shape {
            S::Punctum => NoteShape::Punctum,
            S::Inclinatum => NoteShape::Inclinatum,
            S::Virga => NoteShape::Virga,
            S::VirgaReversa => NoteShape::VirgaReversa,
            S::Quilisma => NoteShape::Quilisma,
            S::Oriscus => NoteShape::Oriscus,
            S::OriscusScapus => NoteShape::OriscusScapus,
            S::Stropha => NoteShape::Stropha,
        },
        liquescent: n.liquescent,
        quilisma: n.quilisma,
        accent: n.accent,
        new_syllable: n.new_syllable,
        recitation: n.recitation,
        verse: n.verse,
        half: n.half,
        span_start: n.span.start as u64,
        span_end: n.span.end as u64,
    }
}

fn pause_kind(k: neuma::PauseKind) -> PauseKind {
    use neuma::score::BarKind as B;
    match k {
        neuma::PauseKind::Bar(b) => match b {
            B::Virgula => PauseKind::Virgula,
            B::Minimis => PauseKind::Minimis,
            B::Minima => PauseKind::Quarter,
            B::Minor => PauseKind::Half,
            B::Maior => PauseKind::Full,
            B::DottedMaior => PauseKind::DottedFull,
            B::Finalis => PauseKind::Double,
            B::Dominican(_) => PauseKind::Dominican,
        },
        neuma::PauseKind::Mediant => PauseKind::Mediant,
        neuma::PauseKind::Flex => PauseKind::Flex,
    }
}

#[cfg(test)]
mod tests;
