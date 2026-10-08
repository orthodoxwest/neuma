//! The drop-cap initial and the annotations above it (docs/DESIGN.md,
//! "Lyrics and text measurement", under Initials).

use crate::score::{Header, Lyric};

/// The tallest initial, in staves.
pub(crate) const MAX_LINES: u8 = 4;

/// Cap height of the lyric face, in ems (EB Garamond's is 0.65). It sizes the initial so its
/// capital spans the staves.
pub(crate) const CAP_HEIGHT: f32 = 0.65;

/// How far a capital's ink runs past its advance, in ems, on the left and on the right: the
/// tail of EB Garamond's Q sweeps well past it, and would reach the staff beside it.
pub(crate) fn overhang(initial: &str) -> (f32, f32) {
    let side = |c: Option<char>, table: &[(char, f32)]| c.and_then(|c| table.iter().find(|e| e.0 == c)).map_or(0.0, |e| e.1);
    let left = side(initial.chars().next(), &[('J', 0.07)]);
    let right = side(
        initial.chars().find(|c| c.is_alphabetic()),
        &[('Q', 0.12), ('Y', 0.045), ('W', 0.015)],
    );
    (left, right)
}

/// How far an initial's ink reaches below the baseline, in ems: the tails of Q and J, and a
/// lowercase descender.
pub(crate) fn depth(initial: &str) -> f32 {
    match initial.chars().next() {
        Some('Q') => 0.27,
        Some('J') => 0.21,
        Some('g' | 'j' | 'p' | 'q' | 'y' | 'ç' | 'ý' | 'ÿ' | 'Ç' | 'Ą' | 'Ę' | 'Į' | 'Ų') => 0.3,
        _ => 0.02,
    }
}

/// How far an initial's ink rises above its cap height, in ems: an accent, a lowercase
/// ascender, the apex of an A, the serifs of a T. Measured from the letter's outline in both
/// built-in faces ([`RISE_LETTERS`]); another Latin, Greek or Cyrillic letter rises by no more
/// than its overshoot ([`OVERSHOOT`]), and a letter of any other script by [`OTHER_SCRIPT_RISE`].
/// Combining marks after the letter are taken as an accent ([`MARK_RISE`]).
pub(crate) fn rise(initial: &str) -> f32 {
    let mut chars = initial.chars();
    let base = chars.next().map_or(OVERSHOOT, |c| match RISE_LETTERS.chars().position(|l| l == c) {
        Some(i) => f32::from(RISE[i]) / 100.0,
        None if latin_greek_or_cyrillic(c) => OVERSHOOT,
        None => OTHER_SCRIPT_RISE,
    });
    if chars.any(|m| COMBINING.contains(&m)) {
        base.max(MARK_RISE)
    } else {
        base
    }
}

/// The combining diacritical marks an initial takes with its letter.
pub(crate) const COMBINING: std::ops::RangeInclusive<char> = '\u{0300}'..='\u{036F}';

/// The rise of a letter of the built-in faces with nothing above its cap height: its
/// overshoot, in ems.
const OVERSHOOT: f32 = 0.015;

/// The rise allowed for combining marks over the letter, in ems: an acute over a capital
/// reaches about 0.19 em above the cap height in both built-in faces, and two marks stacked
/// no more than 0.25 (as the precomposed Ǟ, Ǖ and Ȫ measure). The faces set Vietnamese pairs
/// such as A with U+0302 and U+0300 side by side, and don't stack a third mark over a capital
/// at all, so more marks are given no more room.
const MARK_RISE: f32 = 0.25;

/// The rise allowed for a letter outside the Latin, Greek and Cyrillic scripts, in ems. The
/// built-in faces don't have it, so it is drawn from whatever font the renderer falls back
/// to, and may stand well above a capital's height (a CJK ideograph fills its em square;
/// Khmer and the Indic scripts set marks above the letter): enough room that the annotation
/// clears it in common fallback fonts.
const OTHER_SCRIPT_RISE: f32 = 0.25;

/// Whether `c` is in a Latin, Greek or Cyrillic block, whose letters the built-in faces draw
/// (those that rise above the cap height are in [`RISE_LETTERS`]).
fn latin_greek_or_cyrillic(c: char) -> bool {
    matches!(
        u32::from(c),
        // Latin: Basic Latin to the IPA extensions and spacing modifiers, phonetic extensions,
        // Latin Extended Additional, C, D and E, and the Latin ligatures.
        0x0000..=0x02FF | 0x1D00..=0x1DBF | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF | 0xAB30..=0xAB6F | 0xFB00..=0xFB06
        // Greek and Coptic, and Greek Extended.
        | 0x0370..=0x03FF | 0x1F00..=0x1FFF
        // Cyrillic, its supplement, and Extended A, B and C.
        | 0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F
    )
}

/// The letters of the built-in EB Garamond faces whose ink rises more than 0.02 em above the
/// cap height (0.65 em), and by how much, in hundredths of an em rounded up ([`RISE`]): the
/// higher of the Google Fonts and EB Garamond 12 regular faces' outlines.
const RISE_LETTERS: &str = concat!(
    "ATZbdfhklÀÁÂÃÄÅÈÉÊËÌÍÎÏÑÒÓÔÕÖÙÚÛÜÝßðþĀĂĄĆĈĊČĎďđĒĔĖĚĜĞĠģĤĥħĨĪ",
    "ĬİĴķĹĺļľŀłŃŇŉŌŎŐŔŘŚŜŠŢŤŦŨŪŬŮŰŴŶŸŹŻŽſƀƃƅƇƌƒƓƕƙƚƛƠƥƩƪƬƭƮƯƵǀǁǂǄ",
    "ǆǉǍǏǑǓǕǖǗǘǙǚǛǜǞǟǠǡǢǦǨǩǬǮǱǳǴǸǺǻǼǾȀȂȄȆȈȊȌȎȐȒȔȖȚȞȟȡȢȣȦȪȫȬȭȮȰȱȲȴ",
    "ȸȺȻȾɅɆɊɓɖɗɠɦɧɫɬɭɮɺʃʄʆʎʔʕʖʠʡʢʣʤʥʧʩʪʫʰʱʹʺʻʼʽˈˡʹΆΈΉΊΌΎΏΐΑΔΖΛΞΣΤ",
    "ΦΪΫάέήίΰβδζθλξόύώϑϓϔϡЀЁЂЃЇЋЌЍЎАЙТФбђћѢѣѶҊҌҍҐҞҟҤҬҴһҾӁӏӐӒӖӚӜӞӢ",
    "ӤӦӪӬӮӰӲӴӸԧᴬᴮᴰᴱᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾᴿᵀᵁᵂᵇᵈᵏᶞᶠḀḂḃḅḇḈḊḋḍḏḑḓḔḕḖḗḜḞḟḠḢḣḥḦḧḩḫ",
    "ḮḯḰḱḳḵḷḸḹḻḽḾṀṄṌṍṎṏṐṑṒṓṔṖṘṜṠṤṥṦṧṨṪṬṮṰṸṹṺṻṼẀẂẄẆẊẌẎẐẒẔẖẛẜẝẟẠẢảẤ",
    "ấẦầẨẩẪẫẬẮắẰằẲẳẴẵẶẺẻẼẾếỀềỂểỄễỆỈỉỎỏỐốỒồỔổỖỗỘỚỜỞởỠỢỦủỨỪỬửỮỰỲỶỷỸ",
    "ỻỽἆἇἈἉἊἋἌἍἎἏἚἛἜἝἦἧἪἫἬἭἮἯἶἷἺἻἼἽἾἿὊὋὌὍὖὗὛὝὟὦὧὪὫὬὭὮὯὰάὲέὴήὶίὸόὺ",
    "ύὼώᾆᾇᾈᾉᾊᾋᾌᾍᾎᾏᾖᾗᾚᾛᾜᾝᾞᾟᾦᾧᾪᾫᾬᾭᾮᾯᾲᾴᾸᾹᾺΆᾼῂῄῈΈῊΉῒΐῗῘῙῚΊῢΰῧῨῩῪΎῲῴῸΌ",
    "ῺΏℎℓÅⱡⱨⱪⱹⱿꜧꜨꝃꝉꞎꟾꟿﬀﬁﬂﬃﬄﬅ",
);
const RISE: [u8; 623] = [
    4, 5, 3, 6, 6, 6, 6, 6, 6, 19, 19, 18, 18, 15, 20, 19, 19, 18, 15, 19, 19, 18, 15, 18, 19, 19, 18, 18, 15, 19, 19, 18, 15, 19, 6, 6, 6,
    13, 17, 4, 19, 18, 16, 18, 18, 6, 6, 13, 17, 16, 18, 18, 17, 16, 4, 18, 22, 6, 18, 13, 17, 16, 18, 6, 19, 21, 6, 6, 6, 6, 19, 20, 4,
    13, 17, 20, 19, 20, 19, 18, 20, 5, 20, 5, 18, 13, 17, 20, 20, 18, 18, 15, 19, 16, 18, 6, 6, 6, 6, 8, 7, 6, 8, 6, 6, 6, 6, 8, 6, 3, 6,
    5, 6, 5, 13, 3, 6, 6, 6, 18, 6, 6, 20, 20, 20, 20, 25, 6, 29, 11, 28, 11, 29, 11, 25, 6, 26, 6, 13, 20, 20, 16, 13, 20, 3, 6, 19, 19,
    33, 11, 19, 19, 21, 17, 21, 18, 21, 17, 21, 17, 21, 17, 21, 17, 5, 20, 21, 6, 3, 6, 16, 25, 6, 25, 6, 16, 26, 5, 13, 6, 6, 7, 7, 7, 3,
    7, 3, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 4, 5, 5, 6, 6, 5, 5, 6, 6, 6, 6, 6, 6, 6, 5, 6, 3, 3, 4, 5, 5, 6, 5, 3, 5, 5, 5, 5, 5,
    5, 5, 6, 4, 2, 3, 2, 3, 3, 5, 3, 15, 15, 4, 4, 4, 4, 6, 6, 6, 8, 5, 5, 10, 4, 4, 4, 6, 5, 15, 3, 19, 15, 5, 19, 17, 5, 19, 19, 21, 4,
    21, 5, 5, 6, 6, 6, 4, 6, 25, 21, 4, 6, 11, 4, 6, 4, 5, 5, 6, 15, 24, 6, 21, 15, 21, 15, 17, 17, 13, 15, 15, 15, 17, 13, 15, 20, 17, 17,
    6, 4, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 5, 3, 3, 6, 6, 5, 6, 5, 4, 16, 22, 6, 6, 19, 16, 22, 6, 6, 6, 6, 28, 10, 28, 10, 17,
    16, 20, 13, 16, 20, 6, 15, 18, 6, 6, 31, 11, 19, 16, 6, 6, 6, 13, 15, 6, 6, 19, 16, 16, 29, 13, 29, 8, 28, 10, 28, 10, 19, 16, 16, 13,
    16, 24, 10, 28, 10, 16, 16, 5, 5, 5, 29, 13, 28, 5, 18, 19, 19, 15, 16, 16, 15, 16, 18, 3, 3, 6, 20, 6, 6, 6, 4, 20, 3, 22, 8, 21, 8,
    24, 7, 30, 10, 18, 27, 8, 26, 8, 28, 6, 32, 8, 17, 20, 3, 18, 21, 8, 21, 8, 24, 7, 29, 10, 18, 20, 3, 20, 3, 21, 8, 21, 8, 24, 7, 29,
    9, 18, 19, 19, 20, 3, 18, 8, 20, 3, 19, 19, 20, 3, 18, 13, 19, 20, 3, 18, 6, 6, 8, 8, 4, 4, 5, 5, 5, 5, 10, 10, 5, 5, 5, 5, 8, 8, 5, 5,
    5, 5, 10, 10, 8, 8, 5, 5, 5, 5, 10, 10, 5, 5, 5, 5, 8, 8, 5, 5, 10, 8, 8, 5, 5, 5, 5, 10, 10, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    8, 8, 4, 4, 5, 5, 5, 5, 10, 10, 8, 8, 5, 5, 5, 5, 10, 10, 8, 8, 5, 5, 5, 5, 10, 10, 4, 4, 17, 13, 5, 5, 4, 4, 4, 5, 5, 5, 5, 6, 6, 10,
    17, 13, 5, 5, 6, 6, 10, 17, 13, 5, 5, 4, 4, 5, 5, 5, 5, 6, 6, 20, 6, 6, 6, 6, 3, 6, 5, 6, 6, 6, 10, 2, 6, 6, 6, 6, 6, 6,
];

/// Whether to set the score's first letter as a drop cap, and how many staves tall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Initial {
    /// No initial: the first syllable is set as the others are.
    None,
    /// The initial spans this many staves (at most 4, and no more than the layout has): its
    /// cap height runs from the top line of the first staff to the bottom line of the last.
    /// `Lines(1)`, the default, stands on the first line's lyric baseline instead, as a
    /// one-line initial does in Solesmes books: its cap height runs from the staff's top line
    /// down to the lyrics.
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
            if !COMBINING.contains(&m) {
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
    fn rise_is_measured_per_letter() {
        assert_eq!(RISE_LETTERS.chars().count(), RISE.len());
        assert_eq!(rise("H"), 0.015);
        assert_eq!(rise("T"), 0.05);
        assert_eq!(rise("h"), 0.06);
        assert_eq!(rise("É"), 0.19);
        // Capitals beyond ASCII with nothing above them.
        for c in ["Æ", "Đ", "Χ", "Œ", "Ç"] {
            assert!(rise(c) < 0.03, "{c}");
        }
        assert_eq!(rise("E\u{0301}"), 0.25);
    }

    #[test]
    fn rise_allows_for_other_scripts_and_marks() {
        // Not in the built-in faces: drawn from a fallback font, often taller than a capital.
        for c in ["天", "ស", "ա", "א", "ა"] {
            assert_eq!(rise(c), OTHER_SCRIPT_RISE, "{c}");
        }
        // Latin, Greek and Cyrillic letters the table doesn't list rise by their overshoot.
        for c in ["H", "Ŋ", "Ω", "Ш", "Ꙗ"] {
            assert_eq!(rise(c), OVERSHOOT, "{c}");
        }
        // Marks, stacked or not, get the room two stacked marks take in the built-in faces.
        assert_eq!(rise("A\u{0323}"), MARK_RISE);
        assert_eq!(rise("A\u{0308}\u{0304}"), rise("\u{01DE}"));
        assert_eq!(rise("A\u{0306}\u{0302}\u{0301}"), MARK_RISE);
        assert_eq!(rise("天\u{0301}"), OTHER_SCRIPT_RISE.max(MARK_RISE));
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
