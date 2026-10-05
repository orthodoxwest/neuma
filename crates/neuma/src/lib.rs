//! Neuma engraves Gregorian chant in square notation, from GABC or from pointed psalm text and a
//! psalm tone, for any front end: it returns a platform-neutral display list (and SVG), plus a
//! note map with each note's pitch, duration weight and position for practice tools.
//!
//! The pipeline is `parse` → [`Score`] → [`Score::engrave`] (width-independent, cache it) →
//! [`Engraving::layout`] (cheap; rerun on every resize) → display list, SVG and note map.

pub mod diag;
pub mod gabc;
pub mod glyphs;
pub mod score;

pub use diag::{Diagnostic, Severity};
pub use gabc::{Parsed, parse, to_gabc};
pub use score::{Score, ScoreBuilder};


impl Score {
    /// This score as GABC.
    pub fn to_gabc(&self) -> String {
        gabc::to_gabc(self)
    }
}
