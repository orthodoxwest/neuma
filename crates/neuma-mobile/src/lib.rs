//! neuma for iOS and Android: a `Chant` engraves a score once and lays it out at any
//! width. Each `Layout` it returns gives the display list to draw natively, the playback
//! timeline and hit tests, for itself alone.
//!
//! The bindings are UniFFI (namespace `neuma`). An app builds this crate as its native
//! library, or depends on it from its own UniFFI crate and generates bindings for both in
//! library mode.
//! Glyphs cross as ids: fetch each outline once with `glyph_outline` and draw it at an
//! item's position and scale. Lyrics are drawn with the app's EB Garamond, ligatures off.
//!
//! Every option record field has a default, so `ChantOptions()` and `LayoutOptions()` are
//! the usual options, and the engine's own rules apply to any value it can't use. An
//! option field of an enum type is nullable and defaults to null, which means the default
//! its documentation names (UniFFI can't give an enum field another default). Ids,
//! indices, counts and offsets are signed `Int`s. Positions are in output units (staff
//! spaces times `LayoutOptions.scale`) from the page's top left, y down; boxes are `x, y,
//! w, h` from their top-left corner, and a timeline note's notehead center is `cx, cy`.

use std::sync::{Arc, Mutex, PoisonError, RwLock};

uniffi::setup_scaffolding!("neuma");

/// Which EB Garamond the app draws lyrics with, so they are measured as drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum LyricFont {
    /// The version Google Fonts serves (the default).
    #[default]
    Google,
    /// The EB Garamond 12 release.
    Garamond12,
}

impl From<LyricFont> for neuma::LyricFont {
    fn from(f: LyricFont) -> neuma::LyricFont {
        match f {
            LyricFont::Google => neuma::LyricFont::Google,
            LyricFont::Garamond12 => neuma::LyricFont::Garamond12,
        }
    }
}

/// How a score is engraved. Every field has a default, so `ChantOptions()` is the usual
/// one; values the engine can't use keep their defaults.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ChantOptions {
    /// Drop-cap height in staves: 0 or less for none, at most 4.
    #[uniffi(default = 1)]
    pub initial: i32,
    /// Show the annotation (or the mode) above the initial.
    #[uniffi(default = true)]
    pub annotation: bool,
    /// Lyric size in staff spaces; one that isn't a positive number keeps 2.45.
    #[uniffi(default = 2.45)]
    pub lyric_size: f32,
    /// The EB Garamond the lyrics are drawn with; null for Google Fonts' (an enum field
    /// can't have another default in the bindings).
    #[uniffi(default)]
    pub font: Option<LyricFont>,
}

impl Default for ChantOptions {
    fn default() -> ChantOptions {
        ChantOptions {
            initial: 1,
            annotation: true,
            lyric_size: 2.45,
            font: None,
        }
    }
}

impl From<ChantOptions> for neuma::ChantOptions {
    fn from(o: ChantOptions) -> neuma::ChantOptions {
        neuma::ChantOptions::default()
            .with_initial(neuma::Initial::from_staves(o.initial.into()))
            .with_annotation(o.annotation)
            .with_lyric_size(o.lyric_size)
            .with_font(o.font.unwrap_or_default().into())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum LastLine {
    #[default]
    Ragged,
    Justified,
}

/// Relative durations per sign, in the timeline's weight units. Not beats: the app picks
/// the tempo. `Weights()` gives one pulse a note and pauses that grow with the bar.
/// `virgula` also times the minimis bar (`^`), and `half` the Dominican bars. Negative or
/// non-finite values keep the default; values are capped at 1000.
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

impl Default for Weights {
    fn default() -> Weights {
        let d = neuma::Weights::default();
        Weights {
            note: d.note,
            mora: d.mora,
            episema: d.episema,
            virgula: d.virgula,
            quarter: d.quarter,
            half: d.half,
            full: d.full,
            double: d.double,
            mediant: d.mediant,
            flex: d.flex,
        }
    }
}

impl From<Weights> for neuma::Weights {
    fn from(w: Weights) -> neuma::Weights {
        // The timeline replaces any weight it can't use.
        neuma::Weights::default()
            .with_note(w.note)
            .with_mora(w.mora)
            .with_episema(w.episema)
            .with_virgula(w.virgula)
            .with_quarter(w.quarter)
            .with_half(w.half)
            .with_full(w.full)
            .with_double(w.double)
            .with_mediant(w.mediant)
            .with_flex(w.flex)
    }
}

/// How a score is laid out. Every field has a default, so `LayoutOptions()` is the usual one.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct LayoutOptions {
    /// Output units (points or pixels) per staff space; one that isn't a positive number
    /// keeps 6.
    #[uniffi(default = 6.0)]
    pub scale: f32,
    /// null for ragged.
    #[uniffi(default)]
    pub last_line: Option<LastLine>,
    /// Keep only the first this many lines, as broken for the whole score, for previews
    /// such as an incipit; 0 or less keeps them all. An initial spanning more lines keeps
    /// its full size, and the height includes it. The timeline ends with the kept lines.
    #[uniffi(default = 0)]
    pub max_lines: i32,
}

impl Default for LayoutOptions {
    fn default() -> LayoutOptions {
        LayoutOptions {
            scale: 6.0,
            last_line: None,
            max_lines: 0,
        }
    }
}

impl From<LayoutOptions> for neuma::LayoutOptions {
    fn from(o: LayoutOptions) -> neuma::LayoutOptions {
        neuma::LayoutOptions::default()
            .with_scale(o.scale)
            .with_last_line(match o.last_line.unwrap_or_default() {
                LastLine::Ragged => neuma::LastLine::Ragged,
                LastLine::Justified => neuma::LastLine::Justified,
            })
            .with_max_lines(usize::try_from(o.max_lines).unwrap_or(0))
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
    pub start: i32,
    pub end: i32,
    /// The same range in UTF-16 code units: Kotlin and Java string indices, and `NSRange`.
    pub utf16_start: i32,
    pub utf16_end: i32,
    /// A stable code such as `gabc::hyphen-in-syllable` (listed in docs/diagnostics.md).
    pub code: String,
    pub message: String,
    /// The one edit that fixes the problem, where there is one.
    pub fix: Option<Fix>,
}

/// A source edit: replace the range (empty to insert) with `replacement`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Fix {
    /// UTF-8 byte range in the source.
    pub start: i32,
    pub end: i32,
    /// The same range in UTF-16 code units.
    pub utf16_start: i32,
    pub utf16_end: i32,
    pub replacement: String,
    /// What the edit does, for a quick-fix menu.
    pub title: String,
}

/// What a source-map element is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ElementKind {
    Note,
    Bar,
    Syllable,
}

/// How an offset into the source counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum OffsetUnit {
    /// UTF-8 bytes, as spans are stored.
    Utf8,
    /// UTF-16 code units: Kotlin and Java string indices, and `NSRange` locations.
    Utf16,
}

/// A note, bar or syllable as drawn, with its source.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SourceElement {
    pub kind: ElementKind,
    /// The note id, or the bar's or syllable's index in the score.
    pub index: i32,
    /// UTF-8 byte range in the source.
    pub start: i32,
    pub end: i32,
    /// The same range in UTF-16 code units.
    pub utf16_start: i32,
    pub utf16_end: i32,
    pub line: i32,
    /// The box drawn, from its top-left corner, in output units. A syllable's box spans
    /// its line's height across its notes and lyric.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// A note's notehead center (the box can be trimmed off center); for a bar or
    /// syllable, the box's center.
    pub cx: f32,
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

/// One thing to draw, in output units from the page's top left, y down.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Item {
    /// The outline `glyph_outline` returns for `glyph`, drawn with its origin at (x, y)
    /// and scaled by `scale`.
    Glyph {
        glyph: i32,
        x: f32,
        y: f32,
        scale: f32,
        role: Ink,
        /// The notes this ink draws: none, one, or two for a porrectus swash.
        notes: Vec<i32>,
    },
    /// A filled rectangle from its top-left corner: staff and ledger lines, stems, bars,
    /// episemata.
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        role: Ink,
        notes: Vec<i32>,
    },
    /// Text starting at x on `baseline`, `size` units high (the font size).
    Text {
        x: f32,
        baseline: f32,
        size: f32,
        runs: Vec<TextRun>,
        role: TextRole,
        syllable: Option<i32>,
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
pub struct TimelineNote {
    /// Stable across layouts of one `Chant`.
    pub id: i32,
    pub syllable: i32,
    pub word: i32,
    pub line: i32,
    /// The notehead's center, in output units.
    pub cx: f32,
    pub cy: f32,
    /// The notehead's size.
    pub w: f32,
    pub h: f32,
    /// When the note starts and how long it lasts, in weight units.
    pub start: f32,
    pub duration: f32,
    pub staff_position: i32,
    /// Diatonic steps above the clef's do.
    pub degree: i32,
    /// Semitones above the clef's do, flats applied.
    pub semitones: i32,
    pub syllable_text: String,
    /// The vowel the syllable is centered on, if any.
    pub vowel: Option<String>,
    pub shape: NoteShape,
    pub liquescent: bool,
    /// The syllable has an acute accent in the source.
    pub accent: bool,
    /// The first note of its syllable.
    pub new_syllable: bool,
    /// Inferred: part of a run of three or more single-note syllables on one pitch.
    pub recitation: bool,
    /// Advances after each full or double bar.
    pub verse: i32,
    /// 1 after the verse's mediant `*`, else 0.
    pub half: i32,
    /// UTF-8 byte range of the note in the source.
    pub source_start: i32,
    pub source_end: i32,
    /// The same range in UTF-16 code units.
    pub source_utf16_start: i32,
    pub source_utf16_end: i32,
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

/// A pause before note `before_note` (the note count means after the last note), its
/// start and duration in weight units. A mediant or flex is the whole pause at its bar: the
/// bar right after it lasts 0.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Pause {
    pub before_note: i32,
    pub kind: PauseKind,
    pub start: f32,
    pub duration: f32,
}

/// When each note sounds, and where it is drawn.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Timeline {
    pub notes: Vec<TimelineNote>,
    pub pauses: Vec<Pause>,
    /// The total length, in weight units.
    pub duration: f32,
}

/// What a layout draws.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    /// One staff space, in output units.
    pub staff_space: f32,
    pub items: Vec<Item>,
    pub lines: Vec<LineBox>,
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

/// The outline for a glyph item's id, or `None` for an unknown id.
#[uniffi::export]
pub fn glyph_outline(id: i32) -> Option<GlyphOutline> {
    let id = u16::try_from(id).ok()?;
    neuma::glyph_outline(id).map(|g| GlyphOutline { path: g.d, width: g.width })
}

/// One score: engraved once, laid out on demand, and updated in place as it is edited.
/// Safe to share across threads; layouts run side by side, and each `Layout` answers for
/// itself, so a thumbnail and the main view never disturb each other's hit tests.
#[derive(Debug, uniffi::Object)]
pub struct Chant {
    inner: RwLock<Inner>,
}

#[derive(Debug)]
struct Inner {
    chant: neuma::Chant,
    diagnostics: Vec<Diagnostic>,
    psalm: Option<Psalm>,
}

/// A psalm the chant was set from, to set again on each update.
#[derive(Debug)]
struct Psalm {
    tone: neuma_tones::Tone,
    options: neuma_tones::PsalmOptions,
    notes: Vec<PsalmNote>,
}

impl Inner {
    fn new(chant: neuma::Chant, psalm: Option<Psalm>) -> Inner {
        let mut inner = Inner {
            chant,
            diagnostics: Vec::new(),
            psalm,
        };
        inner.refresh();
        inner
    }

    fn refresh(&mut self) {
        let utf16 = self.chant.utf16();
        self.diagnostics = self.chant.diagnostics().iter().map(|d| diagnostic(d, utf16)).collect();
    }
}

#[uniffi::export]
impl Chant {
    #[uniffi::constructor]
    pub fn new(gabc: String, options: ChantOptions) -> Arc<Chant> {
        Chant::wrap(Inner::new(neuma::Chant::with_options(&gabc, options.into()), None))
    }

    /// Sets psalm text to a tone, as [`psalm`] does, and engraves it with its spans in the
    /// text: timeline notes' and hit tests' sources, and diagnostics, count the text, and
    /// `psalm_notes` tells each note's place in the tone. `tone` is a built-in tone's name
    /// such as `8.G`, or a tone block (`name:`, `clef:`, `mediant:` and `termination:`
    /// lines). `update` sets new text to the same tone.
    #[uniffi::constructor]
    pub fn from_psalm(text: String, tone: String, psalm: PsalmOptions, options: ChantOptions) -> Result<Arc<Chant>, ToneError> {
        let tone = if tone.contains(':') {
            neuma_tones::Tone::parse(&tone)?
        } else {
            neuma_tones::Tone::named(&tone)?.clone()
        };
        let options_in = psalm_options(psalm);
        let s = neuma_tones::psalm(&text, &tone, &options_in);
        let notes = psalm_notes(&s, &neuma::Utf16Index::new(&text));
        let psalm = Psalm {
            tone,
            options: options_in,
            notes,
        };
        Ok(Chant::wrap(Inner::new(s.into_chant(options.into()), Some(psalm))))
    }

    /// Replaces the score with `src` (GABC, or psalm text for a chant made with
    /// `from_psalm`), keeping the options, as an editor does on each change: only the
    /// syllables around the edit are engraved again, and the next layout reuses the line
    /// breaks it can. Layouts made before keep showing the old score.
    pub fn update(&self, src: String) {
        let mut inner = self.write();
        let inner = &mut *inner;
        match &mut inner.psalm {
            None => inner.chant.update(&src),
            Some(p) => {
                let s = neuma_tones::psalm(&src, &p.tone, &p.options);
                p.notes = psalm_notes(&s, &neuma::Utf16Index::new(&src));
                inner.chant.update_score(s.score, &src, s.diagnostics);
            }
        }
        inner.refresh();
    }

    /// Engraves the score again with new options, as when the reader changes the text size
    /// (Dynamic Type, say). Options that engrave the same change nothing.
    pub fn set_options(&self, options: ChantOptions) {
        let mut inner = self.write();
        inner.chant.set_options(options.into());
        inner.refresh();
    }

    /// Problems found while reading the score.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.read().diagnostics.clone()
    }

    /// The score's library entry.
    pub fn summary(&self) -> Summary {
        summary(self.read().chant.summary())
    }

    /// For a chant made with `from_psalm`, each note's place in the tone: `psalm_notes()[i]`
    /// describes note `i`. Empty otherwise.
    pub fn psalm_notes(&self) -> Vec<PsalmNote> {
        self.read().psalm.as_ref().map(|p| p.notes.clone()).unwrap_or_default()
    }

    /// Lays the score out `width` output units wide. The chant remembers its last few
    /// layouts, so asking again with the same width and options is cheap.
    pub fn layout(&self, width: f32, options: LayoutOptions) -> Arc<Layout> {
        let inner = self.read();
        let layout = inner.chant.layout_with(width, &options.into());
        Arc::new(Layout {
            layout,
            timeline: Mutex::new(None),
        })
    }
}

impl Chant {
    fn wrap(inner: Inner) -> Arc<Chant> {
        Arc::new(Chant { inner: RwLock::new(inner) })
    }

    // A panic is an engine bug, and UniFFI reports it to the caller; the Chant is still
    // whole (each update replaces what it changes), so later calls go on.
    fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A layout of a chant at one width: what to draw, when each note sounds, and what is under
/// a point or a caret. It keeps answering for the score it was made from after the chant is
/// updated.
#[derive(Debug, uniffi::Object)]
pub struct Layout {
    layout: neuma::Layout,
    /// The last timeline made, by the weights (as sanitized) it was timed with.
    timeline: Mutex<Option<(neuma::Weights, Arc<neuma::Timeline>)>>,
}

#[uniffi::export]
impl Layout {
    /// What to draw.
    pub fn page(&self) -> Page {
        let list = self.layout.display();
        Page {
            width: list.width,
            height: list.height,
            staff_space: list.staff_space,
            items: list.items.into_iter().filter_map(item).collect(),
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
            alt_text: list.alt_text,
        }
    }

    /// When each note sounds, timed with `weights`.
    pub fn timeline(&self, weights: Weights) -> Timeline {
        let t = self.timed(weights);
        let utf16 = self.utf16();
        Timeline {
            notes: t.notes.iter().map(|n| note(n, utf16)).collect(),
            pauses: t
                .pauses
                .iter()
                .map(|p| Pause {
                    before_note: int(p.before_note),
                    kind: pause_kind(p.kind),
                    start: p.start,
                    duration: p.duration,
                })
                .collect(),
            duration: t.duration,
        }
    }

    /// The note sounding at time `t` (in weight units) of the timeline timed with
    /// `weights`: null during a pause, before the first note and after the last. For a
    /// playhead that follows audio, on each frame; the timeline is kept between calls.
    pub fn note_at_time(&self, t: f32, weights: Weights) -> Option<TimelineNote> {
        let timeline = self.timed(weights);
        timeline.note_at_time(t).map(|n| note(n, self.utf16()))
    }

    /// The note under (`x`, `y`), or the nearest on that line; null outside every line.
    pub fn note_at(&self, x: f32, y: f32) -> Option<i32> {
        self.layout.note_at(x, y).map(int)
    }

    /// The note, bar or syllable under (`x`, `y`), with its source: a notehead, else a bar
    /// within half a staff space, else a syllable's box, else the nearest syllable on that
    /// line. null outside every line.
    pub fn source_at(&self, x: f32, y: f32) -> Option<SourceElement> {
        let utf16 = self.utf16();
        self.layout.source_at(x, y).map(|e| element(e, utf16))
    }

    /// What to highlight for a caret at `offset` in the source, counted in `unit`: the notes
    /// and bar whose source holds it, then a box per line for its syllable, most specific
    /// first. A caret just after a note, as after typing it, counts as on it; one past the
    /// end is at the end, and a negative one at the start.
    pub fn elements_at(&self, offset: i32, unit: OffsetUnit) -> Vec<SourceElement> {
        let utf16 = self.utf16();
        let offset = usize::try_from(offset).unwrap_or(0);
        let byte = match unit {
            OffsetUnit::Utf8 => offset,
            OffsetUnit::Utf16 => utf16.to_utf8(offset),
        };
        self.layout.elements_at(byte).into_iter().map(|e| element(e, utf16)).collect()
    }
}

impl Layout {
    fn utf16(&self) -> &neuma::Utf16Index {
        self.layout.utf16().expect("a chant's layouts know its source")
    }

    /// The timeline timed with `weights`, kept for the next call with the same weights.
    fn timed(&self, weights: Weights) -> Arc<neuma::Timeline> {
        let weights = neuma::Weights::from(weights).sanitized();
        let mut kept = self.timeline.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((w, t)) = kept.as_ref()
            && *w == weights
        {
            return Arc::clone(t);
        }
        let t = Arc::new(self.layout.timeline_with(&weights));
        *kept = Some((weights, Arc::clone(&t)));
        t
    }
}

/// A count, index or offset as the bindings' `Int`. Scores are far below 2^31 bytes.
fn int<T: TryInto<i32>>(v: T) -> i32 {
    v.try_into().unwrap_or(i32::MAX)
}

fn element_kind(k: neuma::ElementKind) -> ElementKind {
    #[allow(clippy::wildcard_enum_match_arm)] // a kind added later reads as a syllable
    match k {
        neuma::ElementKind::Note => ElementKind::Note,
        neuma::ElementKind::Bar => ElementKind::Bar,
        neuma::ElementKind::Syllable | _ => ElementKind::Syllable,
    }
}

fn element(e: &neuma::Element, utf16: &neuma::Utf16Index) -> SourceElement {
    let r = utf16.range_to_utf16(&e.span);
    SourceElement {
        kind: element_kind(e.kind),
        index: int(e.index),
        start: int(e.span.start),
        end: int(e.span.end),
        utf16_start: int(r.start),
        utf16_end: int(r.end),
        line: int(e.line),
        x: e.x,
        y: e.y,
        w: e.w,
        h: e.h,
        cx: e.cx,
    }
}

/// What a chant `office-part` header names, in Latin or English, spelled out or abbreviated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum OfficePart {
    Antiphon,
    Introit,
    Gradual,
    Alleluia,
    Tract,
    Sequence,
    Offertory,
    Communion,
    Hymn,
    Responsory,
    ShortResponsory,
    Versicle,
    Chapter,
    Collect,
    Psalm,
    Canticle,
    Kyrie,
    Gloria,
    Credo,
    Sanctus,
    Agnus,
    /// Something else; the header itself is in `office_part`.
    Other,
}

/// The `mode`, `mode-modifier` and `mode-differentia` headers.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Mode {
    /// 1 to 8, when the header starts with an arabic or roman number.
    pub number: Option<i32>,
    /// The `mode` header as written, such as `8`, `VIII` or `per`.
    pub name: String,
    pub modifier: Option<String>,
    pub differentia: Option<String>,
}

/// A score's library entry: its descriptive headers (TeX removed; missing or empty ones
/// are null) and what can be read off its notes without laying it out.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Summary {
    pub name: Option<String>,
    /// The `office-part` header as written, and what it names.
    pub office_part: Option<String>,
    pub kind: Option<OfficePart>,
    pub mode: Option<Mode>,
    pub occasion: Option<String>,
    pub book: Option<String>,
    pub language: Option<String>,
    pub transcriber: Option<String>,
    pub gabc_copyright: Option<String>,
    pub score_copyright: Option<String>,
    pub commentary: Option<String>,
    pub annotations: Vec<String>,
    /// Every other header, as written and in source order (`source`, `translation-of`).
    pub other_headers: Vec<HeaderField>,
    /// The opening words: up to the first bar (other than a virgula) at or after the end of
    /// the second word, at most eight words.
    pub incipit: String,
    /// All the sung text, for full-text search.
    pub text: String,
    /// The lowest and highest notes, in semitones above the clef's do.
    pub lowest: Option<i32>,
    pub highest: Option<i32>,
    /// The last note, in semitones above the clef's do.
    pub final_pitch: Option<i32>,
    pub notes: i32,
    pub syllables: i32,
    pub words: i32,
    /// The length with the default weights, in pulses.
    pub duration: f32,
}

/// A header the summary doesn't type.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HeaderField {
    pub name: String,
    pub value: String,
}

/// Summarizes a score without engraving it for display: cheap enough to index a library.
#[uniffi::export]
pub fn summarize(gabc: String) -> Summary {
    summary(neuma::summarize(&gabc))
}

/// A diagnostic, with UTF-16 offsets from `utf16`, the index of the text its spans count.
fn diagnostic(d: &neuma::Diagnostic, utf16: &neuma::Utf16Index) -> Diagnostic {
    let r = utf16.range_to_utf16(&d.span);
    Diagnostic {
        severity: match d.severity {
            neuma::Severity::Info => Severity::Info,
            neuma::Severity::Warning => Severity::Warning,
            neuma::Severity::Error => Severity::Error,
        },
        start: int(d.span.start),
        end: int(d.span.end),
        utf16_start: int(r.start),
        utf16_end: int(r.end),
        code: d.code.to_string(),
        message: d.message.clone(),
        fix: d.fix.as_ref().map(|f| {
            let r = utf16.range_to_utf16(&f.span);
            Fix {
                start: int(f.span.start),
                end: int(f.span.end),
                utf16_start: int(r.start),
                utf16_end: int(r.end),
                replacement: f.replacement.clone(),
                title: f.title.clone(),
            }
        }),
    }
}

/// When a psalm's intonation is sung.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum Intone {
    /// On the first verse only, as at the Office.
    #[default]
    FirstVerse,
    /// On every verse, as in the Gospel canticles.
    EveryVerse,
    Never,
}

/// How psalm text is set. Every field has a default, so `PsalmOptions()` is the usual one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PsalmOptions {
    /// null for the first verse only.
    #[uniffi(default)]
    pub intone: Option<Intone>,
    /// Point half-verses that have no marks before setting them; off leaves them
    /// unpointed.
    #[uniffi(default = true)]
    pub auto_point: bool,
}

impl Default for PsalmOptions {
    fn default() -> PsalmOptions {
        PsalmOptions {
            intone: None,
            auto_point: true,
        }
    }
}

/// Which part of a verse a note is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum VersePart {
    /// Up to the flex `†`.
    Flex,
    /// Up to the mediant `*`.
    Mediant,
    /// After the mediant.
    Termination,
}

/// What a note does in the psalm tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ToneRole {
    Intonation,
    /// The reciting note.
    Tenor,
    Preparatory,
    Accent,
    /// After an accent: passing notes and the cadence's ending.
    Ending,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PsalmNote {
    /// Index of the verse in the text.
    pub verse: i32,
    /// The printed verse number.
    pub number: Option<i32>,
    pub part: VersePart,
    pub role: ToneRole,
    /// The sung syllable's UTF-8 bytes in the text, as a timeline note's source.
    pub source_start: i32,
    pub source_end: i32,
    /// The same range in UTF-16 code units.
    pub source_utf16_start: i32,
    pub source_utf16_end: i32,
}

/// Psalm text set to a tone; `notes[i]` describes note `i`. To engrave it with its spans in
/// the text, use `Chant.from_psalm`.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PsalmSetting {
    pub gabc: String,
    pub notes: Vec<PsalmNote>,
    /// Problems in the text and its pointing, with spans in the text.
    pub diagnostics: Vec<Diagnostic>,
}

/// A tone that can't be had. Its message says why ("no built-in tone 9.z").
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Error)]
#[uniffi(flat_error)]
pub enum ToneError {
    /// No built-in tone has that name.
    Unknown { name: String },
    /// The tone block can't be read.
    Invalid { reason: String },
}

impl std::fmt::Display for ToneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToneError::Unknown { name } => write!(f, "no built-in tone {name}"),
            ToneError::Invalid { reason } => f.write_str(reason),
        }
    }
}

impl std::error::Error for ToneError {}

impl From<neuma_tones::ToneError> for ToneError {
    fn from(e: neuma_tones::ToneError) -> ToneError {
        #[allow(clippy::wildcard_enum_match_arm)] // any other error reads as invalid
        match e {
            neuma_tones::ToneError::Unknown { name } => ToneError::Unknown { name },
            neuma_tones::ToneError::Invalid { reason } => ToneError::Invalid { reason },
            other => ToneError::Invalid { reason: other.to_string() },
        }
    }
}

/// Sets psalm text (a verse per line, the mediant marked `*`, optionally pointed with `†`,
/// `·`, acutes and `–`) to a built-in tone such as `8.G` (see [`tone_names`]). Half-verses
/// with no marks are pointed automatically (see [`point`]); `point::unsure` diagnostics flag
/// those to check.
#[uniffi::export]
pub fn psalm(text: String, tone: String, options: PsalmOptions) -> Result<PsalmSetting, ToneError> {
    Ok(setting(neuma_tones::Tone::named(&tone)?, &text, options))
}

/// [`psalm`] for a tone of your own, given as a tone block (`name:`, `clef:`, `mediant:`
/// and `termination:` lines).
#[uniffi::export]
pub fn psalm_with_tone(text: String, tone: String, options: PsalmOptions) -> Result<PsalmSetting, ToneError> {
    Ok(setting(&neuma_tones::Tone::parse(&tone)?, &text, options))
}

/// The pointer's choice for one half-verse.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HalfPointing {
    /// Index of the verse in the text.
    pub verse: i32,
    pub part: VersePart,
    /// The model's probability for its choice, 0 to 1; below about 0.8 it is worth checking.
    pub confidence: f32,
    /// The half already carried marks, which were kept.
    pub kept: bool,
    /// The half's sung syllables in the text, first to last, in UTF-8 bytes.
    pub source_start: i32,
    pub source_end: i32,
    /// The same range in UTF-16 code units.
    pub source_utf16_start: i32,
    pub source_utf16_end: i32,
}

/// Psalm text with pointing marks added.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Pointing {
    /// The pointed text, in the same markup `psalm` reads.
    pub text: String,
    /// Each verse's mediant and termination, in order.
    pub halves: Vec<HalfPointing>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Points psalm text (a verse per line, the mediant marked `*`) for a built-in tone: marks
/// each half-verse's accents and cadence start, keeping halves that already carry marks.
#[uniffi::export]
pub fn point(text: String, tone: String) -> Result<Pointing, ToneError> {
    Ok(pointing(neuma_tones::Tone::named(&tone)?, &text))
}

/// [`point`] for a tone of your own, given as a tone block.
#[uniffi::export]
pub fn point_with_tone(text: String, tone: String) -> Result<Pointing, ToneError> {
    Ok(pointing(&neuma_tones::Tone::parse(&tone)?, &text))
}

fn verse_part(k: neuma_tones::VersePart) -> VersePart {
    match k {
        neuma_tones::VersePart::Flex => VersePart::Flex,
        neuma_tones::VersePart::Mediant => VersePart::Mediant,
        neuma_tones::VersePart::Termination => VersePart::Termination,
    }
}

fn pointing(tone: &neuma_tones::Tone, text: &str) -> Pointing {
    let p = neuma_tones::point(text, tone);
    let utf16 = neuma::Utf16Index::new(text);
    Pointing {
        halves: p
            .halves
            .iter()
            .map(|h| {
                let r = utf16.range_to_utf16(&h.span);
                HalfPointing {
                    verse: int(h.verse),
                    part: verse_part(h.part),
                    confidence: h.confidence,
                    kept: h.kept,
                    source_start: int(h.span.start),
                    source_end: int(h.span.end),
                    source_utf16_start: int(r.start),
                    source_utf16_end: int(r.end),
                }
            })
            .collect(),
        diagnostics: p.diagnostics.iter().map(|d| diagnostic(d, &utf16)).collect(),
        text: p.text,
    }
}

/// The built-in tones' names.
#[uniffi::export]
pub fn tone_names() -> Vec<String> {
    neuma_tones::Tone::builtin().iter().map(|t| t.name.clone()).collect()
}

fn tone_role(r: neuma_tones::ToneRole) -> ToneRole {
    #[allow(clippy::wildcard_enum_match_arm)] // a role added later reads as passing notes
    match r {
        neuma_tones::ToneRole::Intonation => ToneRole::Intonation,
        neuma_tones::ToneRole::Tenor => ToneRole::Tenor,
        neuma_tones::ToneRole::Preparatory => ToneRole::Preparatory,
        neuma_tones::ToneRole::Accent => ToneRole::Accent,
        neuma_tones::ToneRole::Ending | _ => ToneRole::Ending,
    }
}

fn psalm_options(options: PsalmOptions) -> neuma_tones::PsalmOptions {
    neuma_tones::PsalmOptions::default()
        .with_intone(match options.intone.unwrap_or_default() {
            Intone::FirstVerse => neuma_tones::Intone::FirstVerse,
            Intone::EveryVerse => neuma_tones::Intone::EveryVerse,
            Intone::Never => neuma_tones::Intone::Never,
        })
        .with_auto_point(options.auto_point)
}

fn psalm_notes(s: &neuma_tones::PsalmSetting, utf16: &neuma::Utf16Index) -> Vec<PsalmNote> {
    s.notes
        .iter()
        .map(|n| {
            let r = utf16.range_to_utf16(&n.span);
            PsalmNote {
                verse: int(n.verse),
                number: n.number.map(int),
                part: verse_part(n.part),
                role: tone_role(n.role),
                source_start: int(n.span.start),
                source_end: int(n.span.end),
                source_utf16_start: int(r.start),
                source_utf16_end: int(r.end),
            }
        })
        .collect()
}

fn setting(tone: &neuma_tones::Tone, text: &str, options: PsalmOptions) -> PsalmSetting {
    let utf16 = neuma::Utf16Index::new(text);
    let s = neuma_tones::psalm(text, tone, &psalm_options(options));
    PsalmSetting {
        notes: psalm_notes(&s, &utf16),
        diagnostics: s.diagnostics.iter().map(|d| diagnostic(d, &utf16)).collect(),
        gabc: s.gabc,
    }
}

fn office_part(k: neuma::OfficePart) -> OfficePart {
    use neuma::OfficePart as P;
    #[allow(clippy::wildcard_enum_match_arm)] // a kind added later reads as other
    match k {
        P::Antiphon => OfficePart::Antiphon,
        P::Introit => OfficePart::Introit,
        P::Gradual => OfficePart::Gradual,
        P::Alleluia => OfficePart::Alleluia,
        P::Tract => OfficePart::Tract,
        P::Sequence => OfficePart::Sequence,
        P::Offertory => OfficePart::Offertory,
        P::Communion => OfficePart::Communion,
        P::Hymn => OfficePart::Hymn,
        P::Responsory => OfficePart::Responsory,
        P::ShortResponsory => OfficePart::ShortResponsory,
        P::Versicle => OfficePart::Versicle,
        P::Chapter => OfficePart::Chapter,
        P::Collect => OfficePart::Collect,
        P::Psalm => OfficePart::Psalm,
        P::Canticle => OfficePart::Canticle,
        P::Kyrie => OfficePart::Kyrie,
        P::Gloria => OfficePart::Gloria,
        P::Credo => OfficePart::Credo,
        P::Sanctus => OfficePart::Sanctus,
        P::Agnus => OfficePart::Agnus,
        P::Other | _ => OfficePart::Other,
    }
}

fn summary(s: neuma::Summary) -> Summary {
    Summary {
        name: s.name,
        office_part: s.office_part,
        kind: s.kind.map(office_part),
        mode: s.mode.map(|m| Mode {
            number: m.number.map(i32::from),
            name: m.name,
            modifier: m.modifier,
            differentia: m.differentia,
        }),
        occasion: s.occasion,
        book: s.book,
        language: s.language,
        transcriber: s.transcriber,
        gabc_copyright: s.gabc_copyright,
        score_copyright: s.score_copyright,
        commentary: s.commentary,
        annotations: s.annotations,
        other_headers: s
            .other_headers
            .into_iter()
            .map(|(name, value)| HeaderField { name, value })
            .collect(),
        incipit: s.incipit,
        text: s.text,
        lowest: s.lowest,
        highest: s.highest,
        final_pitch: s.final_pitch,
        notes: int(s.notes),
        syllables: int(s.syllables),
        words: int(s.words),
        duration: s.duration,
    }
}

fn notes(note: Option<u32>, through: Option<u32>) -> Vec<i32> {
    match note {
        Some(first) => (first..=through.unwrap_or(first).max(first)).map(int).collect(),
        None => Vec::new(),
    }
}

fn ink(i: neuma::Ink) -> Ink {
    #[allow(clippy::wildcard_enum_match_arm)] // ink added later draws as a note's
    match i {
        neuma::Ink::Staff => Ink::Staff,
        neuma::Ink::Ledger => Ink::Ledger,
        neuma::Ink::Stem => Ink::Stem,
        neuma::Ink::Bar => Ink::Bar,
        neuma::Ink::Episema => Ink::Episema,
        neuma::Ink::Mora => Ink::Mora,
        neuma::Ink::Ictus => Ink::Ictus,
        neuma::Ink::Accidental => Ink::Accidental,
        neuma::Ink::Clef => Ink::Clef,
        neuma::Ink::Custos => Ink::Custos,
        neuma::Ink::Note | _ => Ink::Note,
    }
}

fn text_role(r: neuma::TextRole) -> TextRole {
    #[allow(clippy::wildcard_enum_match_arm)] // text added later sets as a lyric
    match r {
        neuma::TextRole::Hyphen => TextRole::Hyphen,
        neuma::TextRole::Initial => TextRole::Initial,
        neuma::TextRole::Annotation => TextRole::Annotation,
        neuma::TextRole::Rubric => TextRole::Rubric,
        neuma::TextRole::Lyric | _ => TextRole::Lyric,
    }
}

/// An item to draw; `None` for a kind this version doesn't know.
fn item(i: neuma::Item) -> Option<Item> {
    #[allow(clippy::wildcard_enum_match_arm)] // an item kind added later isn't drawn
    Some(match i {
        neuma::Item::Glyph {
            glyph,
            x,
            y,
            scale,
            role,
            note,
            through,
        } => Item::Glyph {
            glyph: i32::from(glyph),
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
            role: text_role(role),
            syllable: syllable.map(int),
        },
        _ => return None,
    })
}

fn note_shape(s: neuma::NoteShape) -> NoteShape {
    use neuma::NoteShape as S;
    #[allow(clippy::wildcard_enum_match_arm)] // a shape added later reads as a punctum
    match s {
        S::Inclinatum => NoteShape::Inclinatum,
        S::Virga => NoteShape::Virga,
        S::VirgaReversa => NoteShape::VirgaReversa,
        S::Quilisma => NoteShape::Quilisma,
        S::Oriscus => NoteShape::Oriscus,
        S::OriscusScapus => NoteShape::OriscusScapus,
        S::Stropha => NoteShape::Stropha,
        S::Punctum | _ => NoteShape::Punctum,
    }
}

fn note(n: &neuma::TimelineNote, utf16: &neuma::Utf16Index) -> TimelineNote {
    let r = utf16.range_to_utf16(&n.span);
    TimelineNote {
        id: int(n.id),
        syllable: int(n.syllable),
        word: int(n.word),
        line: int(n.line),
        cx: n.cx,
        cy: n.cy,
        w: n.w,
        h: n.h,
        start: n.start,
        duration: n.duration,
        staff_position: n.staff_position,
        degree: n.degree,
        semitones: n.semitones,
        syllable_text: n.syllable_text.clone(),
        vowel: n.vowel.map(String::from),
        shape: note_shape(n.shape),
        liquescent: n.liquescent,
        accent: n.accent,
        new_syllable: n.new_syllable,
        recitation: n.recitation,
        verse: int(n.verse),
        half: int(n.half),
        source_start: int(n.span.start),
        source_end: int(n.span.end),
        source_utf16_start: int(r.start),
        source_utf16_end: int(r.end),
    }
}

fn pause_kind(k: neuma::PauseKind) -> PauseKind {
    use neuma::BarKind as B;
    #[allow(clippy::wildcard_enum_match_arm)] // a pause added later reads as a half bar's
    match k {
        neuma::PauseKind::Bar(b) => match b {
            B::Virgula => PauseKind::Virgula,
            B::Minimis => PauseKind::Minimis,
            B::Quarter => PauseKind::Quarter,
            B::Full => PauseKind::Full,
            B::DottedFull => PauseKind::DottedFull,
            B::Double => PauseKind::Double,
            B::Dominican(_) => PauseKind::Dominican,
            B::Half | _ => PauseKind::Half,
        },
        neuma::PauseKind::Mediant => PauseKind::Mediant,
        neuma::PauseKind::Flex => PauseKind::Flex,
        _ => PauseKind::Half,
    }
}

#[cfg(test)]
mod tests;
