//! Neuma engraves Gregorian chant in square notation, from GABC or from pointed psalm text and a
//! psalm tone, for any front end: it returns a platform-neutral display list (and SVG), plus a
//! note map with each note's pitch, duration weight and position for practice tools.
//!
//! The pipeline is `parse` → [`Score`] → [`Score::engrave`] (width-independent, cache it) →
//! [`Engraving::layout`] (cheap; rerun on every resize) → display list, SVG and note map.

pub mod diag;
pub mod display;
pub mod engrave;
pub mod gabc;
pub mod glyphs;
pub mod layout;
pub mod metrics;
pub mod notes;
pub mod score;
#[cfg(feature = "svg")]
pub mod svg;
pub mod text;
pub mod vowel;

pub use diag::{Diagnostic, Severity};
pub use display::{DisplayList, Item, LineBox, NoteRef, TextRole, TextRun};
pub use engrave::{AlterationScope, CustosPolicy, Engraving, Ink, StyleOptions};
pub use gabc::{Parsed, parse, to_gabc};
pub use glyphs::{GlyphOutline, glyph_outline};
pub use layout::{LastLine, Layout, LayoutOptions};
pub use metrics::MetricsTable;
pub use notes::{MappedNote, NoteMap, Pause, Weights};
pub use score::{Score, ScoreBuilder};
#[cfg(feature = "svg")]
pub use svg::SvgOptions;
pub use text::{ApproxMeasure, TextMeasure};
pub use vowel::VowelRules;

impl Score {
    /// This score as GABC.
    pub fn to_gabc(&self) -> String {
        gabc::to_gabc(self)
    }
}
