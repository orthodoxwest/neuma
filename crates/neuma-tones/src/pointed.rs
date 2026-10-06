//! Pointed psalm text: a verse per line, with the marks a hand-pointed psalter prints.
//!
//! | Mark | Meaning |
//! |---|---|
//! | `*` | The mediant: the end of the first half-verse. |
//! | `†` | The flex: a short drop before the mediant, in long first halves. |
//! | `·` | The cadence starts at the next syllable. Inside a word it also splits it ("e·ver"). |
//! | acute (`á`) | An accented syllable, which takes an accent of the tone's cadence. |
//! | `–` (en dash) between words | A note of the cadence with no syllable of its own: the syllable before it is held ("thou · árt – mý God"). |
//! | `-` inside a word | A sung syllable split ("judg-ed"). Write `\-` for a hyphen that is only spelling ("blood\-guiltiness"). |
//! | `[…]` | A rubric, such as a posture cue: kept, never sung. |
//! | `12` at the start of a line | The verse number. |
//!
//! A line starting with `#` is a comment. [`Pointed::to_text`] writes the canonical form back,
//! and parsing that form and writing it again gives the same text.

use std::fmt::Write as _;
use std::ops::Range;

use neuma::{Diagnostic, Severity};

use crate::syllable::split_points;

/// A parsed pointed text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pointed {
    pub verses: Vec<Verse>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Verse {
    pub number: Option<u32>,
    /// The flex part (if any), the first half to the mediant, and the second half.
    pub parts: Vec<Part>,
    /// Rubrics after the last syllable.
    pub end_rubrics: Vec<String>,
    /// The verse's line in the source.
    pub span: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    /// Up to the flex `†`.
    Flex,
    /// Up to the mediant `*`.
    Mediant,
    /// From the mediant to the end of the verse.
    Termination,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub kind: PartKind,
    pub syllables: Vec<Syllable>,
}

/// How a syllable joins the one before it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Joint {
    /// It starts a word.
    #[default]
    Word,
    /// The word was split with `-`.
    Hyphen,
    /// The word was split with `·`, which also starts the cadence here.
    Dot,
    /// The syllabifier split the word ([`Pointed::syllabified`]).
    Split,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Syllable {
    /// The text as sung and printed: accents kept, markup removed, punctuation attached.
    pub text: String,
    /// The text's bytes in the source.
    pub span: Range<usize>,
    pub joint: Joint,
    /// The syllable carries an acute accent.
    pub accent: bool,
    /// The cadence starts here (a `·` before it).
    pub cadence: bool,
    /// An en dash comes before it: the previous syllable is held for a note.
    pub held: bool,
    /// Rubrics written just before this syllable.
    pub rubrics: Vec<String>,
}

impl Syllable {
    pub fn starts_word(&self) -> bool {
        self.joint == Joint::Word
    }
}

/// Whether `text` holds an acute accent.
pub(crate) fn has_acute(text: &str) -> bool {
    text.chars().any(|c| "áéíóúýǽÁÉÍÓÚÝǼ\u{301}".contains(c))
}

/// Parses pointed text: one verse per line.
pub fn parse(src: &str) -> Pointed {
    let mut out = Pointed::default();
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        if body.trim().is_empty() || body.trim_start().starts_with('#') {
            continue;
        }
        if let Some(v) = parse_verse(body, start, &mut out.diagnostics) {
            out.verses.push(v);
        }
    }
    out
}

/// Byte ranges of whitespace-separated tokens, with `[…]` rubrics kept whole.
fn tokens(line: &str) -> Vec<Range<usize>> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let s = i;
        if b[i] == b'[' {
            while i < b.len() && b[i] != b']' {
                i += 1;
            }
            i = (i + 1).min(b.len());
        } else {
            while i < b.len() && !b[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        out.push(s..i);
    }
    out
}

fn diag(diags: &mut Vec<Diagnostic>, severity: Severity, span: Range<usize>, code: &'static str, message: &str) {
    diags.push(Diagnostic {
        severity,
        span,
        code,
        message: message.to_string(),
    });
}

fn parse_verse(line: &str, base: usize, diags: &mut Vec<Diagnostic>) -> Option<Verse> {
    let mut verse = Verse {
        span: base..base + line.len(),
        ..Verse::default()
    };
    let mut toks = tokens(line);
    // A leading number is the verse number.
    if let Some(first) = toks.first() {
        let t = line[first.clone()].trim_end_matches('.');
        if !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit()) {
            verse.number = t.parse().ok();
            toks.remove(0);
        }
    }
    let mut part = Part {
        kind: PartKind::Mediant,
        syllables: Vec::new(),
    };
    let mut seen_mediant = false;
    let mut seen_flex = false;
    // Marks waiting for the next syllable.
    let mut cadence = false;
    let mut held = false;
    let mut rubrics: Vec<String> = Vec::new();
    let close = |verse: &mut Verse, part: &mut Part, kind: PartKind, next: PartKind, span: Range<usize>, diags: &mut Vec<Diagnostic>| {
        if part.syllables.is_empty() {
            diag(
                diags,
                Severity::Error,
                span,
                "pointed::empty-part",
                "nothing to sing before this mark",
            );
        }
        part.kind = kind;
        let done = std::mem::replace(
            part,
            Part {
                kind: next,
                syllables: Vec::new(),
            },
        );
        verse.parts.push(done);
    };
    for r in toks {
        let span = base + r.start..base + r.end;
        let tok = &line[r.clone()];
        match tok {
            "*" => {
                if seen_mediant {
                    diag(diags, Severity::Error, span, "pointed::two-mediants", "a verse has one mediant `*`");
                    continue;
                }
                seen_mediant = true;
                pending_mark(cadence, held, &span, diags);
                cadence = false;
                held = false;
                close(&mut verse, &mut part, PartKind::Mediant, PartKind::Termination, span, diags);
            }
            "†" => {
                if seen_mediant {
                    diag(
                        diags,
                        Severity::Error,
                        span,
                        "pointed::late-flex",
                        "the flex `†` comes before the mediant",
                    );
                    continue;
                }
                if seen_flex {
                    diag(
                        diags,
                        Severity::Error,
                        span,
                        "pointed::two-flexes",
                        "a verse has at most one flex `†`",
                    );
                    continue;
                }
                seen_flex = true;
                pending_mark(cadence, held, &span, diags);
                cadence = false;
                held = false;
                close(&mut verse, &mut part, PartKind::Flex, PartKind::Mediant, span, diags);
            }
            "·" => cadence = true,
            "–" => {
                if part.syllables.is_empty() {
                    diag(
                        diags,
                        Severity::Info,
                        span,
                        "pointed::leading-dash",
                        "a dash before the first syllable has no syllable to hold, so it is kept and not sung",
                    );
                }
                held = true;
            }
            _ if tok.starts_with('[') => {
                if !tok.ends_with(']') || tok.len() < 2 {
                    diag(
                        diags,
                        Severity::Error,
                        span.clone(),
                        "pointed::open-rubric",
                        "a rubric `[` needs its `]`",
                    );
                }
                rubrics.push(tok.trim_start_matches('[').trim_end_matches(']').to_string());
            }
            _ => {
                word(tok, base + r.start, &mut part, &mut cadence, &mut held, &mut rubrics, diags);
            }
        }
    }
    let end = base + line.len();
    pending_mark(cadence, held, &(end..end), diags);
    verse.end_rubrics = rubrics;
    if !part.syllables.is_empty() || verse.parts.is_empty() {
        if !seen_mediant {
            diag(
                diags,
                Severity::Warning,
                verse.span.clone(),
                "pointed::no-mediant",
                "a verse without a mediant `*` is sung as one half-verse",
            );
        }
        part.kind = PartKind::Termination;
        if part.syllables.is_empty() {
            diag(
                diags,
                Severity::Error,
                end..end,
                "pointed::empty-part",
                "nothing to sing after the mediant",
            );
        }
        verse.parts.push(part);
    } else if seen_mediant {
        diag(
            diags,
            Severity::Error,
            end..end,
            "pointed::empty-part",
            "nothing to sing after the mediant",
        );
    }
    if verse.parts.iter().all(|p| p.syllables.is_empty()) {
        return None;
    }
    // Each half has at most one cadence mark.
    for p in &verse.parts {
        let marks: Vec<&Syllable> = p.syllables.iter().filter(|s| s.cadence).collect();
        if marks.len() > 1 {
            diag(
                diags,
                Severity::Warning,
                marks[1].span.clone(),
                "pointed::two-cadence-marks",
                "a half-verse has one `·`; the cadence starts at the last one",
            );
        }
    }
    Some(verse)
}

fn pending_mark(cadence: bool, held: bool, span: &Range<usize>, diags: &mut Vec<Diagnostic>) {
    if cadence || held {
        diag(
            diags,
            Severity::Warning,
            span.clone(),
            "pointed::dangling-mark",
            "a `·` or `–` needs a syllable after it in the same half-verse",
        );
    }
}

/// Adds one word, splitting it at `-` and `·`.
fn word(
    tok: &str,
    at: usize,
    part: &mut Part,
    cadence: &mut bool,
    held: &mut bool,
    rubrics: &mut Vec<String>,
    diags: &mut Vec<Diagnostic>,
) {
    // A `·` at either end of a word is between words.
    let mut tok_start = 0;
    let mut tok_end = tok.len();
    if tok.starts_with('·') {
        *cadence = true;
        tok_start = '·'.len_utf8();
    }
    let trailing_dot = tok[tok_start..].ends_with('·') && tok_end - tok_start > '·'.len_utf8();
    if trailing_dot {
        tok_end -= '·'.len_utf8();
    }
    let body = &tok[tok_start..tok_end];
    let mut joint = Joint::Word;
    let mut text = String::new();
    let mut text_start = at + tok_start;
    let mut i = 0;
    let push = |text: &mut String,
                joint: Joint,
                start: usize,
                end: usize,
                part: &mut Part,
                cadence: &mut bool,
                held: &mut bool,
                rubrics: &mut Vec<String>| {
        let t = std::mem::take(text);
        part.syllables.push(Syllable {
            accent: has_acute(&t),
            text: t,
            span: start..end,
            joint,
            cadence: std::mem::take(cadence) || joint == Joint::Dot,
            held: std::mem::take(held),
            rubrics: std::mem::take(rubrics),
        });
    };
    while i < body.len() {
        let rest = &body[i..];
        if rest.starts_with("\\-") {
            text.push('-');
            i += 2;
            continue;
        }
        let split = if rest.starts_with('-') && !text.is_empty() && i + 1 < body.len() {
            Some((Joint::Hyphen, 1))
        } else if rest.starts_with('·') && !text.is_empty() {
            Some((Joint::Dot, '·'.len_utf8()))
        } else {
            None
        };
        if let Some((next_joint, len)) = split {
            let end = at + tok_start + i;
            push(&mut text, joint, text_start, end, part, cadence, held, rubrics);
            joint = next_joint;
            i += len;
            text_start = at + tok_start + i;
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        text.push(c);
        i += c.len_utf8();
    }
    if text.is_empty() {
        diag(
            diags,
            Severity::Warning,
            at..at + tok.len(),
            "pointed::empty-syllable",
            "a mark with no syllable after it in the word",
        );
    } else {
        push(
            &mut text,
            joint,
            text_start,
            at + tok_start + body.len(),
            part,
            cadence,
            held,
            rubrics,
        );
    }
    if trailing_dot {
        *cadence = true;
    }
}

impl Pointed {
    /// The canonical text form.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for v in &self.verses {
            let mut first = true;
            if let Some(n) = v.number {
                let _ = write!(out, "{n}");
                first = false;
            }
            for p in &v.parts {
                for s in &p.syllables {
                    if s.joint == Joint::Word {
                        if !first {
                            out.push(' ');
                        }
                        for r in &s.rubrics {
                            let _ = write!(out, "[{r}] ");
                        }
                        if s.held {
                            out.push_str("– ");
                        }
                        if s.cadence {
                            out.push_str("· ");
                        }
                    } else {
                        // Marks inside a word can only come from a split.
                        match s.joint {
                            Joint::Hyphen => out.push('-'),
                            Joint::Dot => out.push('·'),
                            Joint::Split | Joint::Word => {}
                        }
                    }
                    first = false;
                    out.push_str(&s.text.replace('-', "\\-"));
                }
                match p.kind {
                    PartKind::Flex => out.push_str(" †"),
                    PartKind::Mediant => out.push_str(" *"),
                    PartKind::Termination => {}
                }
            }
            for r in &v.end_rubrics {
                let _ = write!(out, " [{r}]");
            }
            out.push('\n');
        }
        out
    }

    /// The same text with every word split into its sung syllables. Splits the markup made
    /// are kept; the syllabifier's are [`Joint::Split`]. A spelling hyphen
    /// ("blood-guiltiness") becomes a split, since the engraver draws hyphens itself.
    pub fn syllabified(&self) -> Pointed {
        let mut out = self.clone();
        for v in &mut out.verses {
            for p in &mut v.parts {
                let mut syls = Vec::with_capacity(p.syllables.len() * 2);
                for s in p.syllables.drain(..) {
                    split_syllable(s, &mut syls);
                }
                p.syllables = syls;
            }
        }
        out
    }
}

/// Splits one marked syllable further, at spelling hyphens and then by the syllabifier.
fn split_syllable(s: Syllable, out: &mut Vec<Syllable>) {
    // Pieces as (text, byte offset of the text within s.text).
    let mut pieces: Vec<(String, usize)> = Vec::new();
    let mut at = 0;
    for chunk in s.text.split('-') {
        if !chunk.is_empty() {
            let mut prev = 0;
            for p in split_points(chunk) {
                pieces.push((chunk[prev..p].to_string(), at + prev));
                prev = p;
            }
            pieces.push((chunk[prev..].to_string(), at + prev));
        }
        at += chunk.len() + 1;
    }
    // Punctuation-only pieces join their neighbours.
    let mut merged: Vec<(String, usize)> = Vec::new();
    for (t, o) in pieces {
        if !t.chars().any(char::is_alphanumeric)
            && let Some(last) = merged.last_mut()
        {
            last.0.push_str(&t);
            continue;
        }
        if let Some(last) = merged.last()
            && !last.0.chars().any(char::is_alphanumeric)
        {
            let (lt, lo) = merged.pop().unwrap_or_default();
            merged.push((lt + &t, lo));
            continue;
        }
        merged.push((t, o));
    }
    if merged.len() <= 1 {
        out.push(Syllable {
            text: s.text.replace('-', ""),
            ..s
        });
        return;
    }
    // The span maps text bytes to source bytes when the text was copied verbatim.
    let verbatim = s.span.len() == s.text.len();
    for (k, (t, o)) in merged.iter().enumerate() {
        let next = merged.get(k + 1).map_or(s.text.len(), |m| m.1);
        let span = if verbatim {
            s.span.start + o..s.span.start + next
        } else {
            s.span.clone()
        };
        out.push(Syllable {
            accent: has_acute(t),
            text: t.trim_end_matches('-').to_string(),
            span,
            joint: if k == 0 { s.joint } else { Joint::Split },
            cadence: k == 0 && s.cadence,
            held: k == 0 && s.held,
            rubrics: if k == 0 { s.rubrics.clone() } else { Vec::new() },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS50: &str = "1 Have mercy upon me, O God, after thy great · góodness; * [Sit.] according to the multitude of thy mercies do away · mine offénces.\n\
        3 For I acknowledge my · fáults, * and my sin is e·ver befóre me.\n\
        4 Against thee only have I sin-ned, † and done this evil in thy · síght; * that thou mightest be justified in thy saying, and clear when · thou art júdg-ed.\n\
        7 Thou · árt – mý God, * and I will · thánk thee.\n";

    #[test]
    fn reads_the_marks() {
        let p = parse(PS50);
        assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
        assert_eq!(p.verses.len(), 4);
        let v = &p.verses[0];
        assert_eq!(v.number, Some(1));
        assert_eq!(v.parts.len(), 2);
        let med = &v.parts[0];
        assert_eq!(med.kind, PartKind::Mediant);
        let cad = med.syllables.iter().position(|s| s.cadence).unwrap();
        assert_eq!(med.syllables[cad].text, "góodness;");
        assert!(med.syllables[cad].accent);
        let term = &v.parts[1];
        assert_eq!(term.syllables[0].rubrics, ["Sit."]);
        // e·ver: a split that starts the cadence.
        let t = &p.verses[1].parts[1].syllables;
        let e = t.iter().position(|s| s.text == "e").unwrap();
        assert_eq!(
            (t[e + 1].text.as_str(), t[e + 1].joint, t[e + 1].cadence),
            ("ver", Joint::Dot, true)
        );
        // The flex part and the hyphen split.
        let v4 = &p.verses[2];
        assert_eq!(
            v4.parts.iter().map(|p| p.kind).collect::<Vec<_>>(),
            [PartKind::Flex, PartKind::Mediant, PartKind::Termination]
        );
        let f = &v4.parts[0].syllables;
        assert_eq!((f[f.len() - 1].text.as_str(), f[f.len() - 1].joint), ("ned,", Joint::Hyphen));
        // The held note.
        let m = &p.verses[3].parts[0].syllables;
        let my = m.iter().find(|s| s.text == "mý").unwrap();
        assert!(my.held && my.accent && !my.cadence);
        // Spans point at the text.
        for v in &p.verses {
            for part in &v.parts {
                for s in &part.syllables {
                    assert_eq!(&PS50[s.span.clone()], s.text, "{s:?}");
                }
            }
        }
    }

    #[test]
    fn writes_what_it_reads() {
        let p = parse(PS50);
        assert_eq!(p.to_text(), PS50);
        assert_eq!(parse(&p.to_text()), p);
        let odd = "Wash me from my · wíck\\-edness, * and [Stand.] · cleanse me fróm my sin. [Bow.]\n";
        assert_eq!(parse(odd).to_text(), odd);
    }

    #[test]
    fn reports_problems() {
        let codes = |s: &str| parse(s).diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
        assert_eq!(codes("a b c"), ["pointed::no-mediant"]);
        assert_eq!(codes("a * b * c"), ["pointed::two-mediants"]);
        assert_eq!(codes("a * b † c"), ["pointed::late-flex"]);
        assert_eq!(codes("a · * b"), ["pointed::dangling-mark"]);
        assert_eq!(codes("* b"), ["pointed::empty-part"]);
        assert_eq!(codes("a *"), ["pointed::empty-part"]);
        assert_eq!(codes("a · b · c * d"), ["pointed::two-cadence-marks"]);
        assert_eq!(
            codes("[Sit a * b"),
            ["pointed::open-rubric", "pointed::no-mediant", "pointed::empty-part"]
        );
        assert!(parse("# a comment\n\n").verses.is_empty());
    }

    #[test]
    fn syllabifies_words() {
        let p = parse("For I acknowledge my · fáults, * and my blood\\-guiltiness is e·ver befóre me.").syllabified();
        let texts: Vec<Vec<&str>> = p.verses[0]
            .parts
            .iter()
            .map(|p| p.syllables.iter().map(|s| s.text.as_str()).collect())
            .collect();
        assert_eq!(texts[0], ["For", "I", "ac", "know", "ledge", "my", "fáults,"]);
        assert_eq!(
            texts[1],
            ["and", "my", "blood", "guil", "ti", "ness", "is", "e", "ver", "be", "fóre", "me."]
        );
        let t = &p.verses[0].parts[1].syllables;
        assert_eq!(t[3].joint, Joint::Split);
        assert!(t[10].accent && !t[9].accent);
    }
}
