//! Automatic pointing: marks where each half-verse's cadence falls, for a given tone.
//!
//! A tone's cadence needs one or two accented syllables and a few preparatory syllables before
//! them. A hand-pointed psalter places them by ear: on the stressed syllables near the end of the
//! half-verse, but not always on the last one ("cleanse me · fróm my sin", "bless · yé the
//! Lord"), keeping a rhythm of strong and weak syllables into the ending. The pointer scores
//! every way to place the accents among the last syllables with a linear model over the
//! syllables' lexical stress (from a stress dictionary), the words involved, how many syllables
//! follow the last accent, and the gap between accents. The weights were fitted to a hand-pointed
//! English psalter. On psalms held out from fitting (with the hand's own syllable splits) they
//! agree with it on about 81% of the half-verses; the hand agrees with itself across different
//! settings of a psalm about 89% of the time.
//!
//! Each half-verse comes with a confidence (the model's probability for its choice), so an
//! editor can flag the doubtful ones. Halves that already carry a mark (an accent, `·` or `–`)
//! are kept as written, so a correction survives pointing again. An acute in plain text
//! ("café") counts as a mark, since it is how the markup writes an accent.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use neuma::Diagnostic;

use crate::pointed::{Joint, Part, Pointed, VersePart};
use crate::syllable::fold;
use crate::tone::{Cadence, Slot, Tone};

/// The pointer's choice for one half-verse.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HalfPointing {
    /// The verse's index in the text.
    pub verse: usize,
    pub part: VersePart,
    /// The model's probability for the chosen pointing, from 0 to 1; 1 for a half that was
    /// kept as written, and 0 for one too short for the tone, which accents every syllable.
    pub confidence: f32,
    /// The half already carried marks, which were kept.
    pub kept: bool,
    /// The half's sung syllables in the text, in UTF-8 bytes from the first one's start to
    /// the last one's end; empty, at the verse's start, for a half with none.
    pub span: Range<usize>,
}

/// A pointed text and how sure the pointer is of each half-verse.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Pointing {
    /// The text with the marks added, a verse a line, in the markup [`psalm`](crate::psalm())
    /// reads: pointing it again gives the same text.
    pub text: String,
    /// The mediant and termination of each verse, in order. A flex is never pointed: its
    /// cadence falls on the last syllable before the `†`.
    pub halves: Vec<HalfPointing>,
    /// Problems reading the text, with spans in it.
    pub diagnostics: Vec<Diagnostic>,
}

/// Points plain (or partly pointed) text for `tone`: one verse per line, with the mediant `*`
/// and optionally a flex `†`. A half the pointer is less than [`UNSURE`](crate::UNSURE) sure
/// of is worth checking.
#[must_use]
pub fn point(text: &str, tone: &Tone) -> Pointing {
    let (pointed, halves) = point_parsed(&Pointed::parse(text), tone);
    Pointing {
        text: pointed.to_text(),
        halves,
        diagnostics: pointed.diagnostics,
    }
}

/// [`point`] for text already parsed: the text split into sung syllables with the marks
/// added (its diagnostics kept), and each half-verse's choice.
pub(crate) fn point_parsed(text: &Pointed, tone: &Tone) -> (Pointed, Vec<HalfPointing>) {
    let mut out = text.syllabified();
    let mut halves = Vec::new();
    for (vi, verse) in out.verses.iter_mut().enumerate() {
        for part in &mut verse.parts {
            let cadence = match part.kind {
                VersePart::Flex => continue,
                VersePart::Mediant => &tone.mediant,
                VersePart::Termination => &tone.termination,
            };
            let kept = part.omitted > 0 || part.held_end > 0 || part.syllables.iter().any(|s| s.accent || s.cadence || s.held);
            let zero = may_end_on_accent(part.kind, tone);
            let confidence = if kept { 1.0 } else { point_part(part, cadence, zero) };
            let span = match (part.syllables.first(), part.syllables.last()) {
                (Some(a), Some(b)) => a.span.start..b.span.end,
                _ => verse.span.start..verse.span.start,
            };
            halves.push(HalfPointing {
                verse: vi,
                part: part.kind,
                confidence,
                kept,
                span,
            });
        }
    }
    (out, halves)
}

/// Marks one unpointed half and returns the confidence.
fn point_part(part: &mut Part, cadence: &Cadence, zero: bool) -> f32 {
    let accents = cadence.accents();
    let n = part.syllables.len();
    if accents == 0 || n == 0 {
        return 1.0;
    }
    let ctx = Context::new(part);
    let tag = tag(part.kind, accents, zero);
    let ok = accentable(part);
    let w = weights();
    let mut scored: Vec<(f32, Vec<usize>)> = candidates(&ok, accents)
        .into_iter()
        .map(|c| {
            (
                features(&ctx, &c, &tag)
                    .iter()
                    .map(|f| w.get(f.as_str()).copied().unwrap_or(0.0))
                    .sum(),
                c,
            )
        })
        .collect();
    let (confidence, chosen) = match scored.iter().cloned().reduce(|a, b| if b.0 > a.0 { b } else { a }) {
        Some((best, c)) => {
            let t = temperature();
            let total: f32 = scored.iter().map(|(s, _)| ((s - best) / t).exp()).sum();
            (1.0 / total, c)
        }
        // Fewer syllables than accents: every syllable that can takes one (the last if none
        // can), and the tone's missing accents are left out.
        None => {
            let all: Vec<usize> = (0..n).filter(|&i| ok[i]).collect();
            (0.0, if all.is_empty() { vec![n - 1] } else { all })
        }
    };
    scored.clear();
    for &a in &chosen {
        let s = &mut part.syllables[a];
        s.accent = true;
        s.text = with_acute(&s.text);
    }
    for pair in chosen.windows(2) {
        let s = &mut part.syllables[pair[1]];
        if pair[1] == pair[0] + 1 && s.joint == Joint::Word {
            s.held = true;
        }
    }
    let prep = cadence.preparatory();
    let first = chosen[0];
    let start = first.saturating_sub(prep);
    part.omitted = prep.saturating_sub(first);
    let s = &mut part.syllables[start];
    s.cadence = true;
    if s.joint != Joint::Word {
        s.joint = Joint::Dot;
    }
    confidence
}

/// The softmax temperature that turns score margins into probabilities, fitted with the
/// weights.
fn temperature() -> f32 {
    static T: OnceLock<f32> = OnceLock::new();
    *T.get_or_init(|| {
        WEIGHTS
            .lines()
            .find_map(|l| l.strip_prefix("# temperature\t")?.trim().parse().ok())
            .unwrap_or(1.0)
    })
}

const WEIGHTS: &str = include_str!("../tones/pointing.weights");

/// Whether a half's last syllable may take its accent, which tags the half's features: only
/// for a mediant whose cadence returns to the tenor. A hand-pointed psalter ends a termination
/// on its accent almost never, whatever the tone.
fn may_end_on_accent(kind: VersePart, tone: &Tone) -> bool {
    kind == VersePart::Mediant && ends_on_tenor(&tone.mediant)
}

/// Whether a one-accent cadence returns to the reciting note after its accent, so the accent
/// can fall on the last syllable (`'k jr j` after a `jr` tenor). A hand-pointed psalter puts a
/// two-accent cadence's last accent on the last syllable almost never, so those are left out.
fn ends_on_tenor(c: &Cadence) -> bool {
    let Some(at) = c.slots.iter().position(|s| matches!(s, Slot::Accent(_))) else {
        return false;
    };
    c.accents() == 1
        && c.slots[at + 1..].iter().all(|s| match s {
            Slot::Fixed(n) | Slot::Open(n) => *n == c.tenor,
            Slot::Accent(_) => false,
        })
}

/// Which syllables can take an accent: those with a letter or digit, not bare punctuation.
fn accentable(part: &Part) -> Vec<bool> {
    part.syllables.iter().map(|s| s.text.chars().any(char::is_alphanumeric)).collect()
}

fn tag(kind: VersePart, accents: usize, zero: bool) -> String {
    let k = match kind {
        VersePart::Flex => "flex",
        VersePart::Mediant => "med",
        VersePart::Termination => "term",
    };
    format!("{k}{accents}:z{}", u8::from(zero))
}

/// Ways to place `accents` accents on the accentable (`ok`) syllables among the last of them:
/// the last accent at most four syllables from the end, all within the last nine.
fn candidates(ok: &[bool], accents: usize) -> Vec<Vec<usize>> {
    let n = ok.len();
    let lo = n.saturating_sub(9);
    let mut out = Vec::new();
    let mut cur = Vec::with_capacity(accents);
    fn walk(from: usize, ok: &[bool], left: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        let n = ok.len();
        if left == 0 {
            if cur.last().is_some_and(|&l| n - 1 - l <= 4) {
                out.push(cur.clone());
            }
            return;
        }
        for i in from..n {
            if n - i < left {
                break;
            }
            if !ok[i] {
                continue;
            }
            cur.push(i);
            walk(i + 1, ok, left - 1, cur, out);
            cur.pop();
        }
    }
    walk(lo, ok, accents, &mut cur, &mut out);
    out
}

/// Each syllable's word and its place in it.
struct Context {
    classes: Vec<String>,
    /// Each syllable's word, for monosyllables; empty otherwise.
    words: Vec<String>,
    /// Whether punctuation follows the syllable.
    pauses: Vec<bool>,
    /// Which word each syllable belongs to.
    word_of: Vec<usize>,
}

impl Context {
    fn new(part: &Part) -> Context {
        let mut classes = Vec::with_capacity(part.syllables.len());
        let mut words = Vec::with_capacity(part.syllables.len());
        let syl = &part.syllables;
        let pauses = syl
            .iter()
            .map(|s| s.text.trim_end().ends_with([',', ';', ':', '.', '!', '?']))
            .collect();
        let mut word_of = Vec::with_capacity(syl.len());
        let mut i = 0;
        while i < syl.len() {
            let mut j = i + 1;
            while j < syl.len() && syl[j].joint != Joint::Word {
                j += 1;
            }
            let word: String = syl[i..j]
                .iter()
                .flat_map(|s| s.text.chars())
                .filter(|c| c.is_alphabetic())
                .map(fold)
                .collect();
            let n = j - i;
            if n == 1 {
                classes.push(if is_function(&word) { format!("f:{word}") } else { "M".to_string() });
                words.push(word);
            } else {
                let st = stress(&word, n);
                classes.extend(st.iter().map(|d| format!("P{d}")));
                words.extend(std::iter::repeat_n(String::new(), n));
            }
            word_of.extend(std::iter::repeat_n(i, j - i));
            i = j;
        }
        Context {
            classes,
            words,
            pauses,
            word_of,
        }
    }

    fn class(&self, i: Option<usize>, edge: &str) -> String {
        match i.and_then(|i| self.classes.get(i)) {
            Some(c) => c.clone(),
            None => edge.to_string(),
        }
    }
}

fn features(ctx: &Context, acc: &[usize], tag: &str) -> Vec<String> {
    let n = ctx.classes.len();
    let last = acc[acc.len() - 1];
    let after = (n - 1 - last).min(4);
    let mut f = vec![
        format!("after|{tag}|{after}"),
        format!("fin|{tag}|{}|{after}", ctx.class(Some(n - 1), "END")),
    ];
    for (j, &a) in acc.iter().enumerate() {
        let role = if j + 1 == acc.len() { 'L' } else { 'F' };
        let c = ctx.class(Some(a), "END");
        let next = ctx.class(Some(a + 1), "END");
        f.push(format!("cls|{role}|{c}"));
        f.push(format!("nxt|{role}|{next}"));
        f.push(format!("prv|{role}|{}", ctx.class(a.checked_sub(1), "BEG")));
        f.push(format!("pair|{role}|{c}|{next}"));
        f.push(format!("tag|{role}|{tag}|{c}"));
        f.push(format!("tri|{role}|{}|{c}|{next}", ctx.class(a.checked_sub(1), "BEG")));
        if !ctx.words[a].is_empty() {
            f.push(format!("word|{role}|{}", ctx.words[a]));
        }
        f.push(format!("pause|{role}|{}", u8::from(ctx.pauses[a])));
        f.push(format!(
            "lastword|{role}|{}|{}",
            u8::from(ctx.word_of[a] == ctx.word_of[n - 1]),
            (n - 1 - a).min(4)
        ));
    }
    for pair in acc.windows(2) {
        f.push(format!("gap|{tag}|{}", (pair[1] - pair[0] - 1).min(4)));
    }
    f
}

/// The fitted weights, keyed by feature.
fn weights() -> &'static HashMap<&'static str, f32> {
    static W: OnceLock<HashMap<&'static str, f32>> = OnceLock::new();
    W.get_or_init(|| {
        WEIGHTS
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let (k, v) = l.rsplit_once('\t')?;
                Some((k, v.parse().ok()?))
            })
            .collect()
    })
}

/// Words that are unstressed unless the rhythm needs them: articles, pronouns, prepositions,
/// conjunctions and auxiliary verbs.
fn is_function(word: &str) -> bool {
    const WORDS: &[&str] = &[
        "a", "all", "also", "am", "an", "and", "are", "art", "as", "at", "be", "been", "being", "but", "by", "can", "could", "did", "do",
        "dost", "doth", "even", "for", "from", "had", "has", "hath", "have", "he", "her", "here", "hers", "him", "his", "how", "i", "if",
        "in", "into", "is", "it", "its", "let", "may", "me", "might", "mine", "must", "my", "no", "nor", "not", "o", "of", "oh", "on",
        "or", "our", "ours", "out", "shall", "shalt", "she", "should", "so", "than", "that", "the", "thee", "their", "theirs", "them",
        "then", "there", "these", "they", "thine", "this", "those", "thou", "thy", "to", "unto", "up", "upon", "us", "was", "we", "were",
        "what", "when", "where", "which", "who", "whom", "whose", "why", "will", "wilt", "with", "would", "ye", "yea", "you", "your",
        "yours",
    ];
    WORDS.binary_search(&word).is_ok()
}

/// Stress digits for each of a word's `n` sung syllables: `1` primary, `2` secondary, `0` none.
pub(crate) fn stress(word: &str, n: usize) -> Vec<u8> {
    let pattern = lookup(word).map(str::to_string).or_else(|| {
        // Archaic endings take the stem's stress: "mak-eth", "ex-alt-est".
        ["eth", "est", "edst"].iter().find_map(|suffix| {
            let stem = word.strip_suffix(suffix)?;
            let mut rev = stem.char_indices().rev();
            let undoubled = match (rev.next(), rev.next()) {
                (Some((at, a)), Some((_, b))) if a == b && stem.chars().count() > 2 => Some(&stem[..at]),
                _ => None,
            };
            [Some(stem.to_string()), Some(format!("{stem}e")), undoubled.map(str::to_string)]
                .into_iter()
                .flatten()
                .find_map(|s| lookup(&s))
                .map(|p| format!("{p}0"))
        })
    });
    let digits: Vec<u8> = match pattern {
        Some(p) => p.bytes().map(|b| b - b'0').collect(),
        None => (0..n).map(|k| u8::from(k == 0)).collect(),
    };
    align(&digits, n)
}

/// Fits a dictionary pattern to the syllabifier's count.
fn align(p: &[u8], n: usize) -> Vec<u8> {
    use std::cmp::Ordering;
    match p.len().cmp(&n) {
        Ordering::Equal => p.to_vec(),
        Ordering::Less => p.iter().copied().chain(std::iter::repeat_n(0, n - p.len())).collect(),
        Ordering::Greater if p[..n].contains(&1) => p[..n].to_vec(),
        Ordering::Greater => p[p.len() - n..].to_vec(),
    }
}

fn lookup(word: &str) -> Option<&'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            include_str!("../tones/stress.txt")
                .lines()
                .filter(|l| !l.starts_with('#'))
                .filter_map(|l| l.split_once(' '))
                .collect()
        })
        .get(word)
        .copied()
}

/// The syllable with an acute on its first vowel ("Lord" → "Lórd"), passing over a
/// consonantal y ("yóuth") and the u after q ("quéen").
fn with_acute(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    let mut done = false;
    let chars: Vec<char> = text.chars().collect();
    let vowel = |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'æ');
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j].to_ascii_lowercase());
        let next = chars.get(i + 1).copied();
        let consonant = match c.to_ascii_lowercase() {
            'y' => next.is_some_and(vowel) && !prev.is_some_and(vowel),
            'u' => prev == Some('q') && next.is_some_and(vowel),
            _ => false,
        };
        if !done && !consonant {
            let accented = match c {
                'a' => Some('á'),
                'e' => Some('é'),
                'i' => Some('í'),
                'o' => Some('ó'),
                'u' => Some('ú'),
                'y' => Some('ý'),
                'A' => Some('Á'),
                'E' => Some('É'),
                'I' => Some('Í'),
                'O' => Some('Ó'),
                'U' => Some('Ú'),
                'Y' => Some('Ý'),
                'æ' => Some('ǽ'),
                'Æ' => Some('Ǽ'),
                _ => None,
            };
            if let Some(a) = accented {
                out.push(a);
                done = true;
                continue;
            }
        }
        out.push(c);
    }
    if !done {
        // No plain vowel: a combining acute after the first letter (or the first character,
        // which keeps the accent in the markup).
        let at = chars.iter().position(|c| c.is_alphabetic()).unwrap_or(0);
        let mut out: String = chars.iter().take(at + 1).collect();
        out.push('\u{301}');
        out.extend(chars.iter().skip(at + 1));
        return out;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(name: &str) -> &'static Tone {
        Tone::named(name).unwrap()
    }

    #[test]
    fn points_plain_text() {
        let p = point(
            "O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.",
            tone("8.G"),
        );
        assert_eq!(
            p.text.trim_end(),
            "O come, let us sing unto the · Lórd * let us heartily rejoice in the strength of · our salvátion."
        );
        assert!(p.halves.iter().all(|h| !h.kept && h.confidence > 0.0 && h.confidence <= 1.0));
        // Pointing again keeps every half, and the marks read back as written.
        let again = point(&p.text, tone("8.G"));
        assert_eq!(again.text, p.text);
        assert!(again.halves.iter().all(|h| h.kept && h.confidence == 1.0));
    }

    #[test]
    fn stress_is_char_safe() {
        // The last two bytes of ṹ are equal; the stem must still be cut by characters.
        let _ = point("maṹeth maṹeth * x", tone("8.G"));
        assert_eq!(stress("blessethth", 2).len(), 2);
    }

    #[test]
    fn acute_skips_consonants() {
        assert_eq!(with_acute("youth"), "yóuth");
        assert_eq!(with_acute("Yea"), "Yéa");
        assert_eq!(with_acute("queen"), "quéen");
        assert_eq!(with_acute("ye"), "yé");
        assert_eq!(with_acute("my"), "mý");
        assert_eq!(with_acute("psst"), "p\u{301}sst");
        assert_eq!(with_acute("42"), "4\u{301}2");
    }

    #[test]
    fn punctuation_takes_no_accent() {
        let p = point_parsed(&Pointed::parse("I called upon the Lord ! * and he heard me ?"), tone("1.D")).0;
        for part in p.verses.iter().flat_map(|v| &v.parts) {
            for s in &part.syllables {
                assert!(!s.accent || s.text.chars().any(char::is_alphanumeric), "{:?}", s.text);
            }
        }
    }

    #[test]
    fn short_halves_skip_punctuation() {
        let p = point("Lord ! * God ?", tone("1.D"));
        assert_eq!(p.halves[0].confidence, 0.0);
        assert!(p.text.starts_with("· Lórd ! *"), "{}", p.text);
    }

    #[test]
    fn dashes_are_kept() {
        let p = point("– – praise the Lord * O my soul", tone("1.D"));
        assert!(p.halves[0].kept);
        assert!(p.text.starts_with("– – praise the Lord *"), "{}", p.text);
    }

    #[test]
    fn one_accent_cadences_can_end_on_the_tenor() {
        assert!(ends_on_tenor(&tone("8.G").mediant));
        assert!(!ends_on_tenor(&tone("8.G").termination));
        assert!(!ends_on_tenor(&tone("1.D").mediant));
    }

    #[test]
    fn function_words_are_sorted() {
        let words = ["a", "all", "yours", "unto", "the"];
        assert!(words.iter().all(|w| is_function(w)));
        assert!(!is_function("lord"));
    }
}
