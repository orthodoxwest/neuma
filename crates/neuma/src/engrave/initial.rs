//! The drop-cap initial and the annotations above it (DESIGN sections 5 and 6.1).

use crate::score::{Header, Lyric};

/// The tallest initial, in staves.
pub(crate) const MAX_LINES: u8 = 4;

/// Cap height of the lyric face, in ems (EB Garamond's is 0.65). It sizes the initial so its
/// capital spans the staves.
pub(crate) const CAP_HEIGHT: f32 = 0.65;

/// Whether to set the score's first letter as a drop cap, and how many staves tall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Initial {
    None,
    /// The initial spans this many staves (at most 4, and no more than the layout has): its
    /// cap height runs from the top line of the first staff to the bottom line of the last.
    /// `Lines(1)`, the default, is GregorioTeX's default initial instead: four times the lyric
    /// size, standing on the first line's lyric baseline.
    Lines(u8),
}

impl Initial {
    /// An initial `staves` staves tall, as the bindings and the CLI take a count: 0 or less
    /// for none, and more than 4 for 4.
    #[must_use]
    pub fn from_staves(staves: i64) -> Initial {
        match staves {
            ..=0 => Initial::None,
            n => Initial::Lines(n.min(i64::from(MAX_LINES)) as u8),
        }
    }
}

impl Default for Initial {
    fn default() -> Initial {
        Initial::Lines(1)
    }
}

/// Splits the drop cap off a lyric: the first letter and any combining marks after it, and
/// the rest of the lyric with its explicit center shifted to match. `None` when the lyric
/// doesn't start with a letter.
pub(crate) fn split_initial(lyric: &Lyric) -> Option<(String, Lyric)> {
    let first = lyric.runs.iter().position(|r| !r.text.is_empty())?;
    let run = &lyric.runs[first];
    if run.consonant {
        return None;
    }
    let mut chars = run.text.chars();
    let c = chars.next()?;
    if !c.is_alphabetic() {
        return None;
    }
    let mut initial = c.to_string();
    let mut taken = 1;
    let rest: String = {
        let mut rest = chars.peekable();
        while let Some(&m) = rest.peek() {
            if !('\u{0300}'..='\u{036F}').contains(&m) {
                break;
            }
            initial.push(m);
            rest.next();
            taken += 1;
        }
        rest.collect()
    };
    let mut out = lyric.clone();
    out.runs[first].text = rest;
    out.runs.retain(|r| !r.text.is_empty());
    out.center = lyric
        .center
        .as_ref()
        .filter(|r| r.start >= taken)
        .map(|r| r.start - taken..r.end - taken);
    Some((initial, out))
}

/// The lines shown above the initial: the `annotation` headers (at most two, the first on
/// top), or else the mode written as a lower-case roman numeral, as GregorioTeX prints it,
/// with its modifier and differentia.
pub(crate) fn annotations(header: &Header) -> Vec<String> {
    let given: Vec<String> = header
        .get_all("annotation")
        .map(strip_tex)
        .filter(|a| !a.is_empty())
        .take(2)
        .collect();
    if !given.is_empty() {
        return given;
    }
    let Some(mode) = header.get("mode").map(strip_tex).filter(|m| !m.is_empty()) else {
        return Vec::new();
    };
    let mut line = match mode.parse::<u32>() {
        Ok(n) if (1..=8).contains(&n) => ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii"][n as usize - 1].to_string(),
        _ => mode,
    };
    for name in ["mode-modifier", "mode-differentia"] {
        if let Some(v) = header.get(name).map(strip_tex).filter(|v| !v.is_empty()) {
            line.push(' ');
            line.push_str(&v);
        }
    }
    vec![line]
}

/// Plain text from a header value that may hold TeX: commands and braces are dropped.
pub(crate) fn strip_tex(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // A control word, or a control symbol such as `\&`, which keeps its character.
                if chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                        chars.next();
                    }
                    if chars.peek() == Some(&' ') {
                        chars.next();
                    }
                } else if let Some(s) = chars.next() {
                    out.push(s);
                }
            }
            '{' | '}' => {}
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn splits_the_first_letter() {
        let lyric = Lyric::from_plain("Pu");
        let (i, rest) = split_initial(&lyric).unwrap();
        assert_eq!((i.as_str(), rest.plain().as_str()), ("P", "u"));
        let lyric = Lyric::from_plain("E\u{0301}x");
        let (i, rest) = split_initial(&lyric).unwrap();
        assert_eq!((i.as_str(), rest.plain().as_str()), ("E\u{0301}", "x"));
        let lyric = Lyric {
            center: Some(1..2),
            ..Lyric::from_plain("Ky")
        };
        assert_eq!(split_initial(&lyric).unwrap().1.center, Some(0..1));
        let (i, rest) = split_initial(&Lyric::from_plain("A")).unwrap();
        assert_eq!((i.as_str(), rest.is_empty()), ("A", true));
        assert!(split_initial(&Lyric::from_plain("1. A")).is_none());
    }

    #[test]
    fn annotation_lines() {
        let h = parse("annotation: Ant.;\nannotation: VIII G;\nmode: 8;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["Ant.", "VIII G"]);
        let h = parse("mode: 8;\nmode-differentia: G;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["viii G"]);
        let h = parse("mode: 2;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["ii"]);
        let h = parse("annotation: {\\sc Ps.};\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["Ps."]);
        let h = parse("mode: per.;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["per."]);
        assert!(annotations(&parse("(c4)").score.header).is_empty());
    }
}
