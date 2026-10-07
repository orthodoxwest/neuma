//! English syllabification for sung text.
//!
//! Sung syllables differ from typographic hyphenation: a singer opens syllables wherever
//! English allows ("e-ver", "peo-ple", "pre-pared"), and short fragments are fine. So the
//! splitter finds the vowel nuclei, then gives each consonant cluster between two nuclei the
//! longest onset English allows, keeping doubled consonants and digraphs such as `ck` and
//! `ng` apart as a singer would ("sin-ned", "wick-ed", "sing-ing"). Two kinds of word keep
//! their parts whole, as hand-pointed psalters print them: a stem before `-ing`, `-eth`,
//! `-ed` and most `-est` ("lov-ing", "help-eth", "ex-alt-ed", "great-est"), and compounds
//! ("there-fore", "with-out", "house-hold"). A short exception list covers words the rules
//! get wrong.

/// Where a word splits into sung syllables: byte offsets into the word, in order, each a
/// boundary between two syllables.
pub(crate) fn split_points(word: &str) -> Vec<usize> {
    // Letters only, as plain lower-case ASCII with the byte offset of the character they came
    // from: accents dropped, ligatures spelt out ("ﬁ" → "fi"), and any other letter read as a
    // consonant, so the rules below can index letters as bytes.
    let mut letters: Vec<(usize, u8)> = Vec::with_capacity(word.len());
    for (i, c) in word.char_indices().filter(|(_, c)| c.is_alphabetic()) {
        let f = fold(c);
        let spelt: &str = match f {
            'a'..='z' => {
                letters.push((i, f as u8));
                continue;
            }
            'æ' | 'œ' => "e",
            'ß' => "ss",
            'ﬁ' => "fi",
            'ﬂ' => "fl",
            'ﬀ' => "ff",
            'ﬃ' => "ffi",
            'ﬄ' => "ffl",
            'ﬅ' | 'ﬆ' => "st",
            'ç' | 'Ç' => "c",
            'ñ' | 'Ñ' => "n",
            'þ' | 'Þ' | 'ð' | 'Ð' => "th",
            _ => "b",
        };
        letters.extend(spelt.bytes().map(|b| (i, b)));
    }
    if letters.len() < 2 {
        return Vec::new();
    }
    let folded: String = letters.iter().map(|&(_, c)| c as char).collect();
    let mut points: Vec<usize> = letter_points(&folded)
        .into_iter()
        .filter_map(|k| letters.get(k).map(|&(i, _)| i))
        .filter(|&i| i > 0)
        .collect();
    points.dedup();
    points
}

/// [`split_points`] as letter indices into a folded word.
fn letter_points(folded: &str) -> Vec<usize> {
    if let Some(parts) = exception(folded) {
        return parts.to_vec();
    }
    if let Some(head) = compound_head(folded) {
        let mut out = vec![head];
        out.extend(letter_points(&folded[head..]).into_iter().map(|k| k + head));
        return out;
    }
    let groups = nuclei(folded);
    let l: Vec<char> = folded.chars().collect();
    let suffix = suffix_start(&l, &groups);
    let mut points = Vec::new();
    for w in groups.windows(2) {
        let (prev_end, next_start) = (w[0].1, w[1].0);
        let cut = match suffix {
            Some(at) if at == next_start && prev_end < next_start => {
                // A consonant doubled for the suffix divides ("stop-ped", "sin-ned"); a
                // stem's own double stays ("dwell-ing", "bless-ed").
                let (a, b) = (l[next_start - 1], next_start.checked_sub(2).map(|k| l[k]));
                if b == Some(a) && !matches!(a, 'l' | 's' | 'f' | 'z') {
                    next_start - 1
                } else {
                    next_start
                }
            }
            _ => boundary(&l, prev_end, next_start),
        };
        points.push(cut);
    }
    points
}

/// Where a suffix sung as its own syllable starts, when the stem before it keeps its
/// consonants: `-ing`, `-eth`, a sounded `-ed`, and `-est` after one consonant or a digraph
/// ("great-est", "high-est", but "har-vest", "tem-pest").
fn suffix_start(l: &[char], groups: &[(usize, usize)]) -> Option<usize> {
    let &(s, e) = groups.last()?;
    if groups.len() < 2 || e - s != 1 {
        return None;
    }
    let tail: String = l[s..].iter().collect();
    let stem: String = l[..s].iter().collect();
    let consonants = stem.chars().rev().take_while(|&c| !is_vowel(c)).count();
    if consonants == 0 {
        return None;
    }
    let ok = match tail.as_str() {
        "ing" | "ings" | "eth" => true,
        // Only a sounded `-ed` ("ex-alt-ed"), not the `-led` of "trou-bled".
        "ed" => stem.ends_with(['t', 'd']),
        "est" => consonants == 1 || ["gh", "ng", "ck", "th", "sh", "ch", "ll", "ss"].iter().any(|d| stem.ends_with(d)) && consonants == 2,
        _ => false,
    };
    ok.then_some(s)
}

/// The length of a word heading a compound ("there-fore", "with-in", "house-hold"), when
/// what follows sounds as a word of its own.
fn compound_head(word: &str) -> Option<usize> {
    const HEADS: &[&str] = &["there", "where", "fore", "some", "whole", "house", "bride", "false", "safe", "with"];
    let head = HEADS.iter().find(|h| word.len() > h.len() + 1 && word.starts_with(*h))?;
    let rest = &word[head.len()..];
    let first = rest.chars().next()?;
    let ok = if is_vowel(first) {
        // "there-in", "where-of", "with-out", but not "with-er" or "fore-ign".
        matches!(*head, "there" | "where" | "with")
            && ["in", "of", "on", "out", "al", "as", "at", "upon", "unto", "after"]
                .iter()
                .any(|r| rest == *r || (rest.len() > r.len() + 1 && rest.starts_with(r) && *head != "with"))
    } else {
        rest.chars().any(is_vowel) && !matches!(rest, "s" | "st" | "d" | "r" | "rs" | "n")
    };
    ok.then_some(head.len())
}

/// Splits `word` into its sung syllables.
#[cfg(test)]
fn syllables(word: &str) -> Vec<&str> {
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
    let next_i = k > 0 && l.get(k + 1) == Some(&'i');
    match c {
        // `y` before a vowel is a consonant: "ye", "yea", "beyond".
        // But not before `i`: "cry-ing", "car-ry-ing".
        'y' => next_i || !(next_vowel && (k == 0 || !is_vowel(l[k - 1]))),
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
        let b2 = if last.0 >= 2 { l[last.0 - 2] } else { ' ' };
        let silent = match tail.as_str() {
            // Not after a consonant and `l` or `r` ("tem-ple", "scep-tre").
            "e" => !(matches!(before(last.0), 'l' | 'r') && !is_vowel(b2) && b2 != before(last.0) && b2 != ' '),
            "es" => {
                let b = before(last.0);
                let b2 = if last.0 >= 2 { l[last.0 - 2] } else { ' ' };
                !(matches!(b, 's' | 'x' | 'z' | 'c' | 'g') || (b == 'h' && matches!(b2, 'c' | 's'))) && !(b == 'l' && !is_vowel(b2))
            }
            "ed" => !matches!(before(last.0), 't' | 'd') && !(before(last.0) == 'l' && !is_vowel(b2) && !matches!(b2, 'l' | 'r' | 'w')),
            _ => false,
        };
        // A `u` after `g` is a glide ("tongue", "plagues"), and so is a `y` after a vowel
        // ("de-stroyed").
        let b = before(last.0);
        let consonant = !is_vowel(b)
            || (b == 'u' && b2 == 'g' && last.0 >= 3 && (is_vowel(l[last.0 - 3]) || l[last.0 - 3] == 'n'))
            || (b == 'y' && is_vowel(b2));
        if silent && consonant {
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
        // An `i` after `sh`, or after `n` or `l` closing a syllable, is a glide too
        // ("fa-shion", "do-min-ion", "pa-vil-ion", "mil-lion").
        ('i', 'o' | 'a' | 'u') => {
            let before2 = if k >= 3 { l[k - 3] } else { ' ' };
            k >= 2
                && !matches!(before, 'c' | 't' | 's' | 'x' | 'g' | 'q')
                && !(before == 'h' && matches!(before2, 's' | 'c'))
                && !(matches!(before, 'n' | 'l') && (is_vowel(before2) || before2 == before))
        }
        // "mighti-er", "low-li-est", "li-eth" (but "field", "friend", "mercies").
        ('i', 'e') => {
            let rest: String = l[k + 1..].iter().collect();
            matches!(rest.as_str(), "r" | "st" | "th" | "rs" | "t" | "ty")
                || (matches!(rest.as_str(), "nt" | "nce" | "nts") && !matches!(before, 'c' | 't' | 's' | 'g' | 'q'))
        }
        // "sanc-tu-a-ry", "cru-el", "Je-hu-ah" (but "qua", "guard").
        ('u', 'a') => !matches!(before, 'q' | 'g'),
        ('u', 'e') => l.get(k + 1) == Some(&'l'),
        // "vir-tu-ous", "pre-sump-tu-ous".
        ('u', 'o') => !matches!(before, 'q' | 'g'),
        // "do-er", "go-est", "what-so-e-ver".
        ('o', 'e') => {
            let rest: String = l[k + 1..].iter().collect();
            matches!(rest.as_str(), "r" | "rs" | "st") || rest.starts_with('v')
        }
        ('y', 'i') => true,
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
    // A soft `g` starts its syllable ("an-gel", "dan-ger").
    let soft = c == "ng" && matches!(l.get(next_start), Some('e' | 'i' | 'y'));
    if matches!(c, "ck" | "ng" | "x") && !soft {
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
        "misael" => &[2, 4],
        "even" => &[1],
        "poor" | "door" | "floor" | "through" | "though" => &[],
        "being" => &[2],
        "upon" => &[2],
        "nothing" => &[2],
        "anything" => &[2, 3],
        "vineyard" | "vineyards" => &[4],
        "nevertheless" => &[2, 5, 8],
        "eye" | "eyes" | "eyed" | "teeth" | "priest" | "isle" | "isles" | "hatred" => match word {
            "hatred" => &[3],
            _ => &[],
        },
        "giant" | "giants" => &[2],
        "alien" | "aliens" => &[1, 3],
        "fiery" => &[2, 3],
        "moab" => &[2],
        "cassia" => &[3, 4],
        "business" => &[4],
        "marriage" => &[3],
        "create" | "created" | "creator" => &[2, 4],
        "hereafter" => &[4, 6],
        "herein" | "hereof" | "hereby" => &[4],
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
            ("dwelleth", "dwell-eth"),
            ("glorious", "glo-ri-ous"),
            ("nation", "na-tion"),
            ("precious", "pre-cious"),
            ("righteousness", "righ-teous-ness"),
            ("everlasting", "e-ver-last-ing"),
            ("multitude", "mul-ti-tude"),
            ("acknowledge", "ac-know-ledge"),
            ("throughly", "through-ly"),
            ("maketh", "mak-eth"),
            ("goeth", "go-eth"),
            ("lieth", "li-eth"),
            ("seeth", "se-eth"),
            ("mightier", "migh-ti-er"),
            ("love", "love"),
            ("praises", "prai-ses"),
            ("comes", "comes"),
            ("turned", "turned"),
            ("exalted", "ex-alt-ed"),
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
            ("heaven", "hea-ven"),
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
            ("upon", "up-on"),
            ("Amen.", "A-men."),
            ("away", "a-way"),
            ("awake", "a-wake"),
            ("jewel", "jew-el"),
            ("reward", "re-ward"),
            ("stopped", "stopped"),
            ("beginning", "be-gin-ning"),
            ("greatest", "great-est"),
            ("harvest", "har-vest"),
            ("therefore", "there-fore"),
            ("wherein", "where-in"),
            ("without", "with-out"),
            ("wither", "wi-ther"),
            ("forest", "for-est"),
            ("household", "house-hold"),
            ("tongue", "tongue"),
            ("plagued", "plagued"),
            ("argue", "ar-gue"),
            ("destroyed", "de-stroyed"),
            ("troubled", "trou-bled"),
            ("called", "called"),
            ("sceptre", "scep-tre"),
            ("fire", "fire"),
            ("crying", "cry-ing"),
            ("carrying", "car-ry-ing"),
            ("dominion", "do-mi-nion"),
            ("fashioned", "fa-shioned"),
            ("lion", "li-on"),
            ("obedient", "o-be-di-ent"),
            ("whatsoever", "what-so-e-ver"),
            ("doers", "do-ers"),
            ("virtuous", "vir-tu-ous"),
            ("yield", "yield"),
            ("vineyard", "vine-yard"),
            ("vinegar", "vi-ne-gar"),
            ("something", "some-thing"),
            ("nothing", "no-thing"),
            ("angels", "an-gels"),
            ("bringeth", "bring-eth"),
            ("Misael", "Mi-sa-el"),
            ("deﬁled", "de-ﬁled"),
            ("fulﬁlled", "ful-ﬁlled"),
            ("Señora", "Se-ño-ra"),
            ("conﬂict", "con-ﬂict"),
            ("aßtra", "a-ßtra"),
            ("oþra", "o-þra"),
            ("Cæsar", "Cæ-sar"),
        ] {
            if split(w) != want {
                bad.push(format!("{w}: {} (want {want})", split(w)));
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn any_letters_split_on_char_boundaries() {
        let odd = [
            "ﬁ", "ﬂ", "ß", "ñ", "þ", "ð", "ç", "æ", "œ", "é", "e\u{301}", "α", "я", "ש", "中", "😀", "'", "1",
        ];
        let plain = ["a", "e", "str", "n", "o", "w", "y", "ck"];
        for x in odd {
            for y in odd {
                for p in plain {
                    for w in [format!("{p}{x}{p}{y}{p}"), format!("{x}{y}"), format!("{x}{p}{x}"), x.repeat(5)] {
                        let pts = split_points(&w);
                        assert!(pts.windows(2).all(|p| p[0] < p[1]), "{w}");
                        assert!(pts.iter().all(|&i| i > 0 && i < w.len() && w.is_char_boundary(i)), "{w}");
                        assert_eq!(syllables(&w).concat(), w);
                    }
                }
            }
        }
    }
}
