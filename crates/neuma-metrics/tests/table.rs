//! The shipped EB Garamond 12 table. Expected widths were measured in Chromium with the same
//! OTF files, `font-variant-ligatures: none` and kerning on.

use neuma::score::TextStyle;
use neuma::{MetricsTable, TextMeasure};

const TABLE: &[u8] = include_bytes!("../../neuma/fonts/eb-garamond-12.bin");

#[test]
fn matches_chromium() {
    let t = MetricsTable::from_bytes(TABLE).unwrap();
    assert!(t.has_face(TextStyle::REGULAR));
    assert!(t.has_face(TextStyle {
        italic: true,
        ..TextStyle::REGULAR
    }));
    assert!(!t.has_face(TextStyle {
        bold: true,
        ..TextStyle::REGULAR
    }));
    for (word, em) in [
        ("BEHOLD", 4.0241),
        ("Lift", 1.4611),
        ("office", 2.2131),
        ("AVAWAY", 3.5470),
        ("Wave,", 2.2511),
        ("℣", 0.5981),
    ] {
        let got = t.advance(word, TextStyle::REGULAR);
        assert!((got - em).abs() < 0.001, "{word}: {got} vs {em}");
    }
}

/// The Google Fonts EB Garamond (v33, static 400 instances), which web pages load.
const GOOGLE: &[u8] = include_bytes!("../../neuma/fonts/eb-garamond-google.bin");

#[test]
fn google_table_matches_chromium() {
    let t = MetricsTable::from_bytes(GOOGLE).unwrap();
    assert!(t.has_face(TextStyle::REGULAR));
    for (word, em) in [
        ("BEHOLD", 4.0620),
        ("Lift", 1.4611),
        ("office", 2.2231),
        ("AVAWAY", 3.6270),
        ("Wave,", 2.2281),
        ("cleanse", 2.6770),
        ("ledge", 1.9511),
    ] {
        let got = t.advance(word, TextStyle::REGULAR);
        assert!((got - em).abs() < 0.001, "{word}: {got} vs {em}");
    }
}
