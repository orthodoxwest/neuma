//! Built-in metrics tables for EB Garamond, the lyric face neuma's output names, so a
//! consumer that draws with one of these fonts needs no table of its own.

use std::sync::OnceLock;

use crate::metrics::MetricsTable;

/// Which EB Garamond the lyrics will be drawn with, so they are measured as drawn. The two
/// differ by about 1% in places, so measure with the one the renderer loads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LyricFont {
    /// The version Google Fonts serves. Without the `font-google` feature it measures as
    /// [`Garamond12`](Self::Garamond12).
    #[default]
    Google,
    /// The EB Garamond 12 release (Debian's EBGaramond12 OTFs). Without the
    /// `font-garamond12` feature it measures as [`Google`](Self::Google).
    Garamond12,
}

// A table left out of the build (see the crate's features) is the other one.
#[cfg(feature = "font-google")]
const GOOGLE: &[u8] = include_bytes!("../fonts/eb-garamond-google.bin");
#[cfg(not(feature = "font-google"))]
const GOOGLE: &[u8] = GARAMOND_12;
#[cfg(feature = "font-garamond12")]
const GARAMOND_12: &[u8] = include_bytes!("../fonts/eb-garamond-12.bin");
#[cfg(not(feature = "font-garamond12"))]
const GARAMOND_12: &[u8] = GOOGLE;

impl LyricFont {
    /// The metrics table's bytes, in the format [`MetricsTable::from_bytes`] reads.
    #[must_use]
    pub fn metrics_bytes(self) -> &'static [u8] {
        match self {
            LyricFont::Google => GOOGLE,
            LyricFont::Garamond12 => GARAMOND_12,
        }
    }

    /// The font's metrics, read once: the [`TextMeasure`](crate::TextMeasure) to engrave with.
    #[must_use]
    pub fn metrics(self) -> &'static MetricsTable {
        static TABLES: [OnceLock<MetricsTable>; 2] = [OnceLock::new(), OnceLock::new()];
        let slot = match self {
            LyricFont::Google => 0,
            LyricFont::Garamond12 => 1,
        };
        TABLES[slot].get_or_init(|| MetricsTable::from_bytes(self.metrics_bytes()).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_tables_parse() {
        for font in [LyricFont::Google, LyricFont::Garamond12] {
            assert!(MetricsTable::from_bytes(font.metrics_bytes()).is_ok(), "{font:?}");
            assert!(!font.metrics().faces.is_empty(), "{font:?}");
        }
    }
}
