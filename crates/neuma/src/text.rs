//! Text measurement. The engine measures lyrics only through [`TextMeasure`], so every
//! renderer gets the widths it will draw (docs/DESIGN.md, "Lyrics and text measurement").

use crate::score::TextStyle;

/// Measures text in the lyric face, in ems. The engine sizes every syllable, hyphen, initial
/// and annotation through this, so a renderer whose text matches the measurer draws text where
/// the layout expects it: lyrics set with ligatures off (`font-variant-ligatures: none` on
/// the web, `'liga' 0` on Android, `.ligature: 0` on iOS), kerning on, and small caps from the
/// face's `smcp` feature. [`MetricsTable`](crate::MetricsTable) measures from a table built
/// from the font files, the same numbers on every platform; [`ApproxMeasure`] guesses.
///
/// A measurer must be `Send + Sync` to engrave a [`Chant`](crate::Chant) with it
/// ([`ChantOptions::with_measure`](crate::ChantOptions::with_measure)).
pub trait TextMeasure {
    /// Advance of `text` in `style`, in ems, with ligatures off, kerning on and real small
    /// caps, the way the renderers set lyrics.
    fn advance(&self, text: &str, style: TextStyle) -> f32;
    /// Ascent and descent in ems for `style`'s face.
    fn vertical(&self, style: TextStyle) -> (f32, f32);
    /// Whether this measurer has a real face for `style`.
    fn has_face(&self, style: TextStyle) -> bool;
    /// The advance of every prefix of `text` that ends a character, pushed onto `out` in
    /// order: for each character, what [`advance`](Self::advance) gives for the text up to
    /// its end. The default measures each prefix; a measurer that adds its characters up one
    /// by one can push the running sums instead, which are the same numbers.
    fn prefix_advances(&self, text: &str, style: TextStyle, out: &mut Vec<f32>) {
        for (k, c) in text.char_indices() {
            out.push(self.advance(&text[..k + c.len_utf8()], style));
        }
    }
}

/// A rough measurer for tests and previews when no metrics table is at hand: average
/// old-style serif advances by character class.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApproxMeasure;

/// [`ApproxMeasure`]'s advance of `c`.
fn approx_advance(c: char) -> f32 {
    match c {
        'i' | 'j' | 'l' | 't' | 'f' | 'r' | 'I' | 'J' | '.' | ',' | ';' | ':' | '\'' | '!' => 0.27,
        'm' | 'w' | 'M' | 'W' => 0.75,
        ' ' => 0.22,
        '-' => 0.3,
        c if c.is_uppercase() => 0.66,
        c if ('\u{0300}'..='\u{036F}').contains(&c) => 0.0,
        _ => 0.47,
    }
}

impl TextMeasure for ApproxMeasure {
    fn advance(&self, text: &str, style: TextStyle) -> f32 {
        let mut w = 0.0;
        for c in text.chars() {
            w += approx_advance(c);
        }
        if style.small_caps { w * 0.85 } else { w }
    }

    fn prefix_advances(&self, text: &str, style: TextStyle, out: &mut Vec<f32>) {
        let mut w = 0.0;
        for c in text.chars() {
            w += approx_advance(c);
            out.push(if style.small_caps { w * 0.85 } else { w });
        }
    }

    fn vertical(&self, _style: TextStyle) -> (f32, f32) {
        (0.8, 0.25)
    }

    fn has_face(&self, style: TextStyle) -> bool {
        !style.bold
    }
}
