//! Built-in metrics tables for EB Garamond, the lyric face neuma's output names, so a
//! consumer that draws with one of these fonts needs no table of its own.

use std::sync::OnceLock;

use crate::metrics::MetricsTable;

/// Which EB Garamond the lyrics will be drawn with. The two differ by about 1% in places, so
/// measure with the one the renderer loads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Font {
    /// The version Google Fonts serves.
    #[default]
    Google,
    /// The EB Garamond 12 release (Debian's EBGaramond12 OTFs).
    Garamond12,
}

const GOOGLE: &[u8] = include_bytes!("../fonts/eb-garamond-google.bin");
const GARAMOND_12: &[u8] = include_bytes!("../fonts/eb-garamond-12.bin");

impl Font {
    /// The table's bytes, in the format [`MetricsTable::from_bytes`] reads.
    pub fn table_bytes(self) -> &'static [u8] {
        match self {
            Font::Google => GOOGLE,
            Font::Garamond12 => GARAMOND_12,
        }
    }

    /// The parsed table, read once.
    pub fn table(self) -> &'static MetricsTable {
        static TABLES: [OnceLock<MetricsTable>; 2] = [OnceLock::new(), OnceLock::new()];
        let slot = match self {
            Font::Google => 0,
            Font::Garamond12 => 1,
        };
        TABLES[slot].get_or_init(|| MetricsTable::from_bytes(self.table_bytes()).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_tables_parse() {
        for font in [Font::Google, Font::Garamond12] {
            assert!(MetricsTable::from_bytes(font.table_bytes()).is_ok(), "{font:?}");
            assert!(!font.table().faces.is_empty(), "{font:?}");
        }
    }
}
