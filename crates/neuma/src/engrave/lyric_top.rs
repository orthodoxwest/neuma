//! How high a lyric's letters reach, so the notes over them can be kept clear of them.
//!
//! The metrics tables carry advances only, so heights come from the letter: an x-height letter,
//! an ascender, a capital, or a letter with an accent. The values are EB Garamond's (the
//! highest of its regular and italic glyphs), and serve other book faces near enough.

use crate::score::LyricRun;
use crate::text::TextMeasure;

/// An accent over a lowercase letter, or a dot over `i` and `j`, in ems above the baseline.
const ACCENTED_LOWER: f32 = 0.67;
/// An accent over a capital.
const ACCENTED_CAPITAL: f32 = 0.86;
/// Ascenders, and anything this table doesn't know.
const ASCENDER: f32 = 0.72;
const CAPITAL: f32 = 0.7;
const X_HEIGHT: f32 = 0.44;
/// A hyphen's top.
pub(crate) const HYPHEN_TOP: f32 = 0.27;

/// The top of `c` above the baseline, in ems.
fn char_top(c: char, small_caps: bool) -> f32 {
    if small_caps && c.is_lowercase() {
        return if c.is_ascii() || matches!(c, 'æ' | 'œ' | 'ç' | 'ø') {
            0.5
        } else {
            ACCENTED_LOWER + 0.05
        };
    }
    match c {
        'a' | 'c' | 'e' | 'g' | 'm' | 'n' | 'o' | 'q' | 'r' | 's' | 'u' | 'v' | 'w' | 'x' | 'y' | 'z' | 'æ' | 'œ' | 'ç' | 'ø' => {
            X_HEIGHT
        }
        'p' | 't' => 0.52,
        'i' | 'j' => 0.63,
        '.' | ',' => 0.13,
        ';' | ':' => 0.39,
        '-' | '\u{2010}' | '\u{2013}' | '\u{2014}' => HYPHEN_TOP,
        '«' | '»' | '‹' | '›' | '+' | '=' => 0.42,
        ' ' | '\u{a0}' | '\u{202f}' => 0.0,
        '0'..='9' | '†' | '‡' | '!' | '?' | '*' | '℣' | '℟' | 'Æ' | 'Œ' | 'Ç' | 'Ø' => CAPITAL,
        'A'..='Z' => CAPITAL,
        c if c.is_ascii() => ASCENDER,
        // Latin letters with an accent: `á`, `ǽ`, `Ú`.
        c if c.is_lowercase() && ('\u{c0}'..='\u{24f}').contains(&c) => ACCENTED_LOWER,
        c if c.is_uppercase() && ('\u{c0}'..='\u{24f}').contains(&c) => ACCENTED_CAPITAL,
        _ => ASCENDER,
    }
}

/// The tops of a lyric's letters, as `(left, right, top)` in staff spaces: left and right from
/// the text's start, the top above its baseline. Neighbouring letters of one height share an
/// entry.
pub(crate) fn profile(runs: &[LyricRun], measure: &dyn TextMeasure, size: f32) -> Vec<(f32, f32, f32)> {
    let mut out: Vec<(f32, f32, f32)> = Vec::new();
    let mut x = 0.0;
    for r in runs {
        for (k, c) in r.text.char_indices() {
            let left = x + measure.advance(&r.text[..k], r.style) * size;
            let right = x + measure.advance(&r.text[..k + c.len_utf8()], r.style) * size;
            if ('\u{300}'..='\u{36f}').contains(&c) {
                // A combining accent raises the letter before it.
                if let Some(last) = out.last_mut() {
                    last.2 = if last.2 >= CAPITAL * size {
                        ACCENTED_CAPITAL * size
                    } else {
                        last.2.max(ACCENTED_LOWER * size)
                    };
                }
                continue;
            }
            let top = char_top(c, r.style.small_caps) * size;
            // The hook of an `f` runs past its advance.
            let right = if c == 'f' { right + 0.1 * size } else { right };
            match out.last_mut() {
                Some(last) if last.2 == top && (last.1 - left).abs() < 1e-4 => last.1 = right,
                _ => out.push((left, right, top)),
            }
        }
        x += measure.advance(&r.text, r.style) * size;
    }
    out.retain(|e| e.2 > 0.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::TextStyle;

    #[test]
    fn letters_reach_their_heights() {
        let runs = [LyricRun {
            text: "Ámen".into(),
            style: TextStyle::REGULAR,
            consonant: false,
        }];
        let p = profile(&runs, &crate::ApproxMeasure, 1.0);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].2, ACCENTED_CAPITAL);
        assert_eq!(p[1].2, X_HEIGHT);
        assert!((p[1].1 - crate::ApproxMeasure.advance("Ámen", TextStyle::REGULAR)).abs() < 1e-6);
        // A combining accent raises its letter.
        let runs = [LyricRun {
            text: "ae\u{301}".into(),
            style: TextStyle::REGULAR,
            consonant: false,
        }];
        assert_eq!(profile(&runs, &crate::ApproxMeasure, 1.0).last().unwrap().2, ACCENTED_LOWER);
    }
}
