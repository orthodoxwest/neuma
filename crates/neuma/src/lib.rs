//! Neuma engraves Gregorian chant in square notation, from GABC or from pointed psalm text and a
//! psalm tone, for any front end: it returns a platform-neutral display list (and SVG), plus a
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
//! Under it is the pipeline, for advanced use: [`parse`] → [`Score`] → [`Score::engrave`]
//! (width-independent; share it in an `Arc`) → [`Engraving::layout`] (cheap; rerun on every
//! resize) → [`Layout`]. Only a [`Chant`] engraves again incrementally after an edit.
//!
//! # Coordinates and units
//!
//! Every output of a layout shares one coordinate system: the origin is the layout's top left
//! corner, y grows downward, and the unit is the output unit, one staff space times
//! [`LayoutOptions::scale`] (px in a browser, pt or dp on a phone). Boxes are given by their
//! top-left corner and size (`x`, `y`, `w`, `h`); the one point given as a center is named so
//! ([`TimelineNote::cx`] and `cy`). Source spans count UTF-8 bytes; [`Utf16Index`] converts to
//! the UTF-16 units JavaScript, Kotlin and Swift strings count.

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
