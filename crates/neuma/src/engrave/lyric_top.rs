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

/// How far a letter's ink runs past its advance on the left and on the right, in ems: the
/// hook of an `f`, and most italic capitals. EB Garamond's, where more than a fiftieth of an em.
fn side_overhang(c: char, italic: bool) -> (f32, f32) {
    const REGULAR: &[(char, f32, f32)] = &[
        ('f', 0.0, 0.1),
        ('J', 0.07, 0.0),
        ('Q', 0.0, 0.12),
        ('Y', 0.0, 0.04),
        ('j', 0.02, 0.0),
    ];
    const ITALIC: &[(char, f32, f32)] = &[
        ('f', 0.19, 0.18),
        ('j', 0.2, 0.04),
        ('p', 0.12, 0.0),
        ('g', 0.1, 0.03),
        ('y', 0.1, 0.0),
        ('s', 0.03, 0.0),
        ('i', 0.0, 0.03),
        ('r', 0.0, 0.04),
        ('t', 0.0, 0.04),
        ('c', 0.0, 0.02),
        ('é', 0.0, 0.07),
        ('í', 0.0, 0.07),
        ('ó', 0.0, 0.05),
        ('A', 0.06, 0.0),
        ('B', 0.02, 0.0),
        ('C', 0.0, 0.08),
        ('E', 0.03, 0.05),
        ('F', 0.0, 0.12),
        ('G', 0.0, 0.03),
        ('H', 0.02, 0.09),
        ('I', 0.02, 0.09),
        ('J', 0.18, 0.12),
        ('K', 0.02, 0.09),
        ('M', 0.05, 0.06),
        ('N', 0.0, 0.12),
        ('P', 0.0, 0.05),
        ('S', 0.0, 0.06),
        ('T', 0.0, 0.12),
        ('U', 0.0, 0.13),
        ('V', 0.0, 0.13),
        ('W', 0.0, 0.13),
        ('X', 0.06, 0.1),
        ('Y', 0.0, 0.16),
        ('Z', 0.02, 0.04),
        ('0', 0.0, 0.03),
        ('5', 0.0, 0.03),
        ('6', 0.0, 0.09),
        ('7', 0.0, 0.11),
        ('9', 0.0, 0.04),
    ];
    let table = if italic { ITALIC } else { REGULAR };
    table.iter().find(|e| e.0 == c).map_or((0.0, 0.0), |e| (e.1, e.2))
}

/// How far a lyric's ink runs past its advance at its start and at its end, in staff spaces.
pub(crate) fn overhang(runs: &[LyricRun], size: f32) -> (f32, f32) {
    let mut chars = runs.iter().flat_map(|r| r.text.chars().map(move |c| (c, r.style.italic)));
    let first = chars.next();
    let last = runs
        .iter()
        .rev()
        .find_map(|r| r.text.chars().next_back().map(|c| (c, r.style.italic)));
    let lead = first.map_or(0.0, |(c, i)| side_overhang(c, i).0);
    let tail = last.map_or(0.0, |(c, i)| side_overhang(c, i).1);
    (lead * size, tail * size)
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
            let (lead, tail) = side_overhang(c, r.style.italic);
            let (left, right) = (left - lead * size, right + tail * size);
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
