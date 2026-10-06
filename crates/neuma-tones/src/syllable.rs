//! English syllabification for sung text.
//!
//! Sung syllables differ from typographic hyphenation: a singer opens syllables wherever
//! English allows ("e-ver", "peo-ple", "pre-pared"), and short fragments are fine. So the
//! splitter finds the vowel nuclei, then gives each consonant cluster between two nuclei the
//! longest onset English allows, keeping doubled consonants and digraphs such as `ck` and
//! `ng` apart as a singer would ("sin-ned", "wick-ed", "sing-ing"). A short exception list
//! covers words the rules get wrong.

/// Where a word splits into sung syllables: byte offsets into the word, in order, each a
/// boundary between two syllables.
pub fn split_points(word: &str) -> Vec<usize> {
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    // Letters only, folded to plain lower-case ASCII where possible, with their byte offsets.
    let letters: Vec<(usize, char)> = chars
        .iter()
        .filter(|(_, c)| c.is_alphabetic())
        .map(|&(i, c)| (i, fold(c)))
        .collect();
    if letters.len() < 2 {
        return Vec::new();
    }
    let folded: String = letters.iter().map(|&(_, c)| c).collect();
    if let Some(parts) = exception(&folded) {
        return parts.iter().filter_map(|&k| letters.get(k).map(|&(i, _)| i)).collect();
    }
    let groups = nuclei(&folded);
    let l: Vec<char> = folded.chars().collect();
    let mut points = Vec::new();
    for w in groups.windows(2) {
        let (prev_end, next_start) = (w[0].1, w[1].0);
        let cut = boundary(&l, prev_end, next_start);
        if let Some(&(i, _)) = letters.get(cut) {
            points.push(i);
        }
    }
    points
}

/// Splits `word` into its sung syllables.
pub fn syllables(word: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut prev = 0;
    for p in split_points(word) {
        out.push(&word[prev..p]);
        prev = p;
    }
    out.push(&word[prev..]);
    out
}

/// A letter without its accent, in lower case (`É` → `e`); other letters lower-cased.
pub(crate) fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'Á' | 'À' | 'Â' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        'æ' | 'Æ' | 'ǽ' | 'Ǽ' => 'æ',
        'œ' | 'Œ' => 'œ',
        _ => c.to_lowercase().next().unwrap_or(c),
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'æ' | 'œ')
}

/// Whether the letter at `k` sounds as a vowel.
fn vowel_at(l: &[char], k: usize) -> bool {
    let c = l[k];
    let next_vowel = l.get(k + 1).is_some_and(|&n| is_vowel(n) && n != 'y');
    match c {
        // `y` before a vowel is a consonant: "ye", "yea", "beyond".
        'y' => !(next_vowel && (k == 0 || !is_vowel(l[k - 1]))),
        // `u` after `q`, and after `g` before a vowel ("guide", "language"), is a glide.
        'u' if k > 0 && (l[k - 1] == 'q' || (l[k - 1] == 'g' && next_vowel)) => false,
        _ => is_vowel(c),
    }
}

/// The vowel nuclei as [start, end) letter ranges.
fn nuclei(word: &str) -> Vec<(usize, usize)> {
    let l: Vec<char> = word.chars().collect();
    let n = l.len();
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut k = 0;
    while k < n {
        if !vowel_at(&l, k) {
            k += 1;
            continue;
        }
        let start = k;
        k += 1;
        while k < n && vowel_at(&l, k) {
            k += 1;
        }
        // A `w` or `y` closing a vowel ("law", "new", "day") belongs to it unless a vowel
        // follows, where it starts the next syllable ("po-wer", "pray-ing" keeps the y).
        groups.push((start, k));
    }
    // Split vowel runs that are two syllables (hiatus).
    let mut split: Vec<(usize, usize)> = Vec::new();
    for &(s, e) in &groups {
        let mut s = s;
        for k in s + 1..e {
            if hiatus(&l, k) {
                split.push((s, k));
                s = k;
            }
        }
        split.push((s, e));
    }
    let mut groups = split;
    // -eth and -est after a vowel are a syllable of their own ("go-eth", "se-eth", "li-est").
    if n >= 4 && (word.ends_with("eth") || word.ends_with("est")) && is_vowel(l[n - 4]) {
        let e = n - 3;
        if let Some(last) = groups.last_mut()
            && last.0 < e
            && last.1 > e
        {
            let s = last.0;
            *last = (e, last.1);
            groups.insert(groups.len() - 1, (s, e));
        }
    }
    // Silent endings: a final `e` ("love", "praise"), `-es` and `-ed` after most consonants
    // ("comes", "turned"), and an `e` before a suffix ("judgement", "lovely").
    if groups.len() >= 2 {
        let last = *groups.last().unwrap();
        let before = |k: usize| if k > 0 { l[k - 1] } else { ' ' };
        let tail: String = l[last.0..].iter().collect();
        let silent = match tail.as_str() {
            "e" => !(before(last.0) == 'l' && last.0 >= 2 && !is_vowel(l[last.0 - 2])),
            "es" => {
                let b = before(last.0);
                let b2 = if last.0 >= 2 { l[last.0 - 2] } else { ' ' };
                !(matches!(b, 's' | 'x' | 'z' | 'c' | 'g') || (b == 'h' && matches!(b2, 'c' | 's'))) && !(b == 'l' && !is_vowel(b2))
            }
            "ed" => !matches!(before(last.0), 't' | 'd'),
            _ => false,
        };
        if silent && !is_vowel(before(last.0)) {
            groups.pop();
        }
    }
    let mut k = 1;
    while k + 1 < groups.len() + 1 && k < groups.len() {
        let (s, e) = groups[k];
        let rest: String = l[e..].iter().collect();
        let suffix = ["ment", "ly", "ful", "ness", "less"].iter().any(|x| rest.starts_with(x));
        if e - s == 1 && l[s] == 'e' && suffix && s > 0 && !is_vowel(l[s - 1]) && k + 1 < groups.len() {
            groups.remove(k);
        } else {
            k += 1;
        }
    }
    groups
}

/// Whether two adjacent vowel letters at `k - 1` and `k` belong to different syllables.
fn hiatus(l: &[char], k: usize) -> bool {
    let (a, b) = (l[k - 1], l[k]);
    let before = if k >= 2 { l[k - 2] } else { ' ' };
    match (a, b) {
        // "glo-ri-ous", "li-on", "Mes-si-ah", but "na-tion", "pre-cious", "re-gion".
        ('i', 'o' | 'a' | 'u') => !matches!(before, 'c' | 't' | 's' | 'x' | 'g' | 'q') && k >= 2,
        // "mighti-er", "low-li-est", "li-eth" (but "field", "friend", "mercies").
        ('i', 'e') => {
            let rest: String = l[k + 1..].iter().collect();
            matches!(rest.as_str(), "r" | "st" | "th" | "rs" | "t" | "ty")
        }
        // "sanc-tu-a-ry", "cru-el", "Je-hu-ah" (but "qua", "guard").
        ('u', 'a') => !matches!(before, 'q' | 'g'),
        ('u', 'e') => l.get(k + 1) == Some(&'l'),
        // "be-ing", "se-eth", "cre-ate" is rarer than "great", so `ea` stays together.
        ('e', 'i') => l.get(k + 1) == Some(&'n') && l.get(k + 2) == Some(&'g'),
        ('o', 'i') => l.get(k + 1) == Some(&'n') && l.get(k + 2) == Some(&'g'),
        // "pray-ing", "slay-er": a `y` closing a vowel ends its syllable.
        ('y', _) => k >= 2 && is_vowel(before),
        (_, 'y') => false,
        _ => false,
    }
}

/// The letter index where the syllable ending at `prev_end` and the one starting at
/// `next_start` divide.
fn boundary(l: &[char], prev_end: usize, next_start: usize) -> usize {
    if prev_end >= next_start {
        return next_start;
    }
    let cluster: String = l[prev_end..next_start].iter().collect();
    // A `w` or `y` right after a vowel closes it ("pow-er", "pray-ing").
    let lead = match l[prev_end] {
        'w' if prev_end > 0 && matches!(l[prev_end - 1], 'o' | 'e') && !is_prefix(&l[..prev_end]) => 1,
        'y' if prev_end > 0 && is_vowel(l[prev_end - 1]) => 1,
        _ => 0,
    };
    let c = &cluster[lead..];
    if c.is_empty() {
        return next_start;
    }
    // Doubled consonants divide ("sin-ned"); `ck`, `ng` and `x` alone stay in the coda
    // ("wick-ed", "sing-ing", "ex-alt").
    let b: Vec<char> = c.chars().collect();
    if b.len() >= 2 && b[0] == b[1] {
        return prev_end + lead + 1;
    }
    if matches!(c, "ck" | "ng" | "x") {
        return next_start;
    }
    // The longest valid onset goes to the next syllable, except that an `s` before another
    // consonant closes the syllable before ("las-ting", "mi-nis-ter") unless that syllable
    // is a prefix ("de-stroy", "re-store").
    for take in (1..=c.len()).rev() {
        let onset = &c[c.len() - take..];
        if is_onset(onset) {
            let start = next_start - take;
            let prefix = is_prefix(&l[..start]);
            if take >= 2 && onset.starts_with('s') && !onset.starts_with("sh") && !prefix {
                return start + 1;
            }
            return start;
        }
    }
    next_start
}

/// A prefix that keeps the next syllable's onset whole ("de-stroy", "re-ward").
fn is_prefix(head: &[char]) -> bool {
    let h: String = head.iter().collect();
    matches!(h.as_str(), "re" | "de" | "be" | "pre")
}

fn is_onset(s: &str) -> bool {
    const ONSETS: &[&str] = &[
        "bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "pl", "pr", "tr", "tw", "dw", "sw", "sc", "sk", "sl", "sm", "sn", "sp", "st",
        "scr", "spl", "spr", "str", "squ", "shr", "thr", "phr", "chr", "sch", "th", "ch", "sh", "ph", "wh", "qu", "kn", "wr", "gu",
    ];
    let mut cs = s.chars();
    match (cs.next(), cs.next()) {
        (Some(c), None) => c.is_alphabetic() && !is_vowel(c) && c != 'x',
        _ => ONSETS.contains(&s),
    }
}

/// Words the rules get wrong, as letter indices where they split.
fn exception(word: &str) -> Option<&'static [usize]> {
    Some(match word {
        "wicked" | "naked" | "crooked" | "wretched" | "rugged" | "ragged" => &[4],
        "sion" | "zion" => &[2],
        "beloved" => &[2, 5],
        "israel" => &[2, 4],
        "every" => &[1, 3],
        "evening" => &[3],
        "heaven" | "heavens" => &[4],
        "even" => &[1],
        "poor" | "door" | "floor" | "through" | "though" => &[],
        "being" => &[2],
        "beyond" => &[2],
        "seeing" | "fleeing" => &[3],
        "doing" | "going" => &[2],
        "fire" | "hire" | "desire" | "lyre" => match word {
            "desire" => &[2],
            _ => &[],
        },
        "alleluia" => &[2, 4, 6],
        "hallelujah" => &[3, 5, 7],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(w: &str) -> String {
        syllables(w).join("-")
    }

    #[test]
    fn common_words() {
        let mut bad = Vec::new();
        for (w, want) in [
            ("mercy", "mer-cy"),
            ("goodness", "good-ness"),
            ("wickedness", "wick-ed-ness"),
            ("ever", "e-ver"),
            ("people", "peo-ple"),
            ("rejoice", "re-joice"),
            ("offences", "of-fen-ces"),
            ("before", "be-fore"),
            ("sinned", "sinned"),
            ("dwelleth", "dwel-leth"),
            ("glorious", "glo-ri-ous"),
            ("nation", "na-tion"),
            ("precious", "pre-cious"),
            ("righteousness", "righ-teous-ness"),
            ("everlasting", "e-ver-las-ting"),
            ("multitude", "mul-ti-tude"),
            ("acknowledge", "ac-know-ledge"),
            ("throughly", "through-ly"),
            ("maketh", "ma-keth"),
            ("goeth", "go-eth"),
            ("lieth", "li-eth"),
            ("seeth", "se-eth"),
            ("mightier", "migh-ti-er"),
            ("love", "love"),
            ("praises", "prai-ses"),
            ("comes", "comes"),
            ("turned", "turned"),
            ("exalted", "ex-al-ted"),
            ("temple", "tem-ple"),
            ("judgement", "judge-ment"),
            ("power", "pow-er"),
            ("praying", "pray-ing"),
            ("singing", "sing-ing"),
            ("Lord", "Lord"),
            ("God;", "God;"),
            ("Góodness,", "Góod-ness,"),
            ("Sion", "Si-on"),
            ("Israel", "Is-ra-el"),
            ("heaven", "heav-en"),
            ("wicked", "wick-ed"),
            ("sanctuary", "sanc-tu-a-ry"),
            ("ye", "ye"),
            ("beyond", "be-yond"),
            ("quiet", "qui-et"),
            ("judges", "jud-ges"),
            ("table", "ta-ble"),
            ("lovely", "love-ly"),
            ("destroy", "de-stroy"),
            ("minister", "mi-nis-ter"),
            ("majesty", "ma-jes-ty"),
            ("distress", "dis-tress"),
            ("children", "chil-dren"),
            ("handmaiden", "hand-mai-den"),
            ("Father", "Fa-ther"),
            ("salvation", "sal-va-tion"),
            ("Jerusalem", "Je-ru-sa-lem"),
            ("enemies", "e-ne-mies"),
            ("upon", "u-pon"),
            ("Amen.", "A-men."),
            ("away", "a-way"),
            ("awake", "a-wake"),
            ("jewel", "jew-el"),
            ("reward", "re-ward"),
        ] {
            if split(w) != want {
                bad.push(format!("{w}: {} (want {want})", split(w)));
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }
}
