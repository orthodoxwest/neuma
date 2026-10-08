//! Neuma engraves Gregorian chant in square notation from GABC (and, with `neuma_tones`, from
//! pointed psalm text and a psalm tone) for any front end: it returns a platform-neutral display list (and SVG), plus a
//! timeline with each note's pitch, duration weight and position for practice tools, and a
//! source map for editors.
//!
//! [`Chant`] is the front door: it owns a score and everything made from it, and updates
//! incrementally as the source changes.
//!
//! ```
//! use neuma::Chant;
//!
//! let mut chant = Chant::new("(c4) Al(f)le(gf)lú(gh)ia.(g.) (::)");
//! assert!(chant.diagnostics().is_empty());
//! let layout = chant.layout(600.0); // output units wide
//! let svg = layout.svg();
//! let timeline = layout.timeline(); // each note's position, pitch, start and duration
//! assert!(svg.starts_with("<svg") && timeline.notes.len() == 6);
//! ```
//!
//! A [`Layout`] is an owned value, cheap to clone and `Send + Sync`: keep layouts at several
//! widths at once, and ask each for its SVG, display list, timeline, source map and hit tests
//! (`note_at`, `source_at`, `elements_at`) while the chant changes.
//!
//! # Concepts
//!
//! ## From a chant to a page
//!
//! Work is split by what it depends on, so that each change redoes only its share:
//!
//! 1. **Reading** ([`parse`]) turns GABC into a [`Score`], the one model of the notes and
//!    text. It depends only on the source. `neuma_tones` builds a `Score` from psalm text and
//!    a tone, and [`ScoreBuilder`] from anything else.
//! 2. **Engraving** ([`Score::engrave`]) builds the neumes, measures the lyrics, centers
//!    them on their vowels and sets the spacing, as segments that don't depend on the width.
//!    It depends on the score and the [`ChantOptions`] (lyric font and size, initial, …).
//! 3. **Layout** ([`Engraving::layout`]) breaks the segments into lines at a width,
//!    justifies them, and places clefs, custodes and the initial. It depends on the width
//!    and the [`LayoutOptions`], and is cheap enough to redo on every resize.
//! 4. **Output** comes from a [`Layout`]: [`Layout::svg`], [`Layout::display`] (a
//!    [`DisplayList`] of glyphs, rectangles and text for a native canvas),
//!    [`Layout::timeline`] (each note's place, pitch and duration) and
//!    [`Layout::source_map`] (what is drawn where, and from which source bytes).
//!
//! A [`Chant`] runs the first three and keeps what they made: after
//! [`update`](Chant::update) it reads and engraves again only around the edit, and its next
//! layout breaks again only the lines the edit touched, with the same result as starting
//! afresh. The lower stages stay public for advanced use; share an [`Engraving`] in an `Arc`
//! to lay it out at many widths. The browser package and the mobile bindings wrap a `Chant`.
//! What they call a page (`chant.layout(width)` in JavaScript, a `ChantLayout` on mobile) is
//! a `Layout` with its outputs, and a JavaScript view is the place a page is shown, laid out
//! again on each change.
//!
//! ## Units and coordinates
//!
//! - **Staff spaces** measure the notation, and everything that scales with it. One staff
//!   space is the distance between two adjacent staff positions, a line and the space beside
//!   it, so the four staff lines are two staff spaces apart. A punctum is one staff space wide.
//! - **Staff positions** ([`score::StaffPosition`]) count those steps from the staff's middle
//!   space: the lines are at −3, −1, 1 and 3, and positions grow upward (GABC's `a` is −6).
//! - **Ems** measure text: a [`TextMeasure`] gives advances in ems of the lyric font, and
//!   [`StyleOptions::lyric_size`] says how many staff spaces an em is (2.45 by default,
//!   GregorioTeX's 10 pt lyrics on its default staff). Text and notation scale together.
//! - **Glyph units** are the outlines' coordinates ([`glyph_outline`]): 100 to a staff space.
//! - **Output units** are what every output is in: staff spaces times
//!   [`LayoutOptions::scale`] (6 by default), so px in a browser, or pt or dp on a phone. The
//!   width given to `layout` is in output units, and lines are broken at `width / scale` staff
//!   spaces, so a reader's text-size change is a new `scale` and a new layout, not a new
//!   engraving.
//!
//! Every output of a layout shares one coordinate system: the origin is the layout's top left
//! corner, y grows downward, and the unit is the output unit. Boxes are given by their
//! top-left corner and size (`x`, `y`, `w`, `h`); the one point given as a center is named so
//! ([`TimelineNote::cx`] and `cy`). Source spans count UTF-8 bytes; [`Utf16Index`] converts to
//! the UTF-16 units JavaScript, Kotlin and Swift strings count.
//!
//! ## Versions
//!
//! [`Chant::version`] names a chant's state: a number no other state of any chant in the
//! process has had, which grows with each change that changed anything. An `update` or
//! `set_options` that changes nothing returns `false` and keeps the version, so a view keyed
//! on the version lays out again exactly when it must. A [`Layout`] keeps showing the state
//! it was made from for as long as it is held, whatever the chant has become since.
//!
//! ## Diagnostics
//!
//! Reading and engraving never fail. What the engine can't read, doesn't support or has to
//! approximate comes back as a [`Diagnostic`] with a [`Severity`], a source span, a stable
//! code (such as `gabc::unclosed-notes`) and, where one edit makes sense, a [`Fix`];
//! [`Chant::diagnostics`] lists them, and the engine draws what it can. Codes are stable and
//! messages are not. `docs/diagnostics.md` in the repository lists every code.
//!
//! ## Features
//!
//! - `svg` (default): [`Layout::svg`] and the SVG writer.
//! - `fonts` (default): the metrics of both built-in EB Garamond releases; `font-google` or
//!   `font-garamond12` builds in one of them ([`LyricFont`]).
//! - `json`: the `json` module, the outputs as the JSON the browser package and the CLI give.

#![warn(missing_docs)]

/// `with_*` setters for an options struct's fields, so callers outside the crate needn't
/// spell out a struct literal (the structs are `#[non_exhaustive]`).
macro_rules! setters {
    ($ty:ident { $($field:ident: $t:ty => $name:ident),* $(,)? }) => {
        impl $ty {
            $(
                #[doc = concat!("Sets [`", stringify!($field), "`](Self::", stringify!($field), ").")]
                #[must_use]
                pub fn $name(mut self, $field: $t) -> Self {
                    self.$field = $field;
                    self
                }
            )*
        }
    };
}
pub(crate) use setters;

mod chant;
#[cfg(any(feature = "svg", feature = "json"))]
pub(crate) mod decimal;
pub mod diag;
pub mod display;
pub mod engrave;
#[cfg(any(feature = "font-google", feature = "font-garamond12"))]
pub mod fonts;
pub mod gabc;
/// The glyph table generated from exsurge's outlines. Its names and variants follow the
/// generator and may change in any release; glyphs cross the public API as `u16` ids, with
/// outlines from [`glyph_outline`].
#[doc(hidden)]
pub mod glyphs;
#[cfg(feature = "json")]
pub mod json;
pub mod layout;
pub mod metrics;
pub mod notes;
pub mod score;
pub mod source;
pub mod summary;
#[cfg(feature = "svg")]
pub mod svg;
pub mod text;
pub mod vowel;

pub use chant::{Chant, ChantOptions};
pub use diag::{Diagnostic, Fix, Severity};
pub use display::{DisplayList, Item, LineBox, NoteRef, TextRole, TextRun};
pub use engrave::{AlterationScope, CustosPolicy, Engraving, Initial, Ink, StyleOptions};
#[cfg(any(feature = "font-google", feature = "font-garamond12"))]
pub use fonts::LyricFont;
pub use gabc::{Parsed, parse};
pub use glyphs::{GlyphOutline, glyph_outline};
pub use layout::{LastLine, Layout, LayoutOptions};
pub use metrics::{MetricsError, MetricsTable};
pub use notes::{Pause, PauseKind, Timeline, TimelineNote, Weights};
pub use score::{BarKind, NoteShape, Score, ScoreBuilder, TextStyle};
pub use source::{Element, ElementKind, SourceMap, Utf16Index};
pub use summary::{Mode, OfficePart, Summary, summarize};
#[cfg(feature = "svg")]
pub use svg::{SvgLine, SvgOptions, SvgParts};
pub use text::{ApproxMeasure, TextMeasure};
pub use vowel::VowelRules;

impl Score {
    /// This score as GABC.
    #[must_use]
    pub fn to_gabc(&self) -> String {
        gabc::to_gabc(self)
    }
}
