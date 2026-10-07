//! [`PsalmDisplay`]: a psalm as a pointed psalter prints it, with the tone shown once as a
//! line of notes ([`Tone::gabc`]) and the verses as text carrying their pointing marks.

use std::ops::Range;

use neuma::Diagnostic;

use crate::apply::{SungPsalm, sing, strip_acutes};
use crate::pointed::{Joint, Pointed, VersePart};
use crate::{PsalmOptions, Tone, ToneRole};

/// Psalm text pointed for a tone, verse by verse, as styled runs of text: what a pointed
/// psalter prints under the tone. Half-verses with no marks are pointed automatically (unless
/// [`PsalmOptions::auto_point`] is off), and the diagnostics are those [`psalm`](crate::psalm())
/// gives, `point::unsure` among them.
///
/// Each verse is a list of [`PsalmRun`]s. Their texts, joined, are the line to print (after
/// the verse number); their kinds say how to style each: the marks in red (`·` and `–` in
/// bold), rubrics in red italic, and in a flex the syllables where the voice drops
/// ([`PsalmSyllable::flex_drop`]) in italic. A syllable run gives its place in the text and in
/// the tone, for a tap or a highlight that follows the singing.
///
/// ```
/// use neuma_tones::{PsalmDisplay, PsalmOptions, PsalmRunKind, Tone};
///
/// let text = "1 Have mercy upon me, O God, after thy great · góodness; * [Sit.] according to thy mercies.";
/// let display = PsalmDisplay::new(text, Tone::named("8.G")?, &PsalmOptions::default());
/// let verse = &display.verses()[0];
/// assert_eq!(verse.number, Some(1));
/// let line: String = verse.runs.iter().map(|r| r.text.as_str()).collect();
/// assert!(line.starts_with("Have mercy upon me, O God, after thy great · góodness; * Sit. according"));
/// let point = verse.runs.iter().position(|r| r.kind == PsalmRunKind::Point).unwrap();
/// let PsalmRunKind::Syllable(after) = &verse.runs[point + 2].kind else { panic!() };
/// assert!(after.accent && &text[after.span.clone()] == "góod");
/// # Ok::<(), neuma_tones::ToneError>(())
/// ```
#[derive(Clone, Debug)]
pub struct PsalmDisplay {
    text: String,
    tone: Tone,
    options: PsalmOptions,
    verses: Vec<PsalmVerse>,
    diagnostics: Vec<Diagnostic>,
}

/// One verse of a [`PsalmDisplay`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PsalmVerse {
    /// The verse number written at the start of its line.
    pub number: Option<u32>,
    /// The verse's line in the text, in UTF-8 bytes.
    pub span: Range<usize>,
    /// The verse as printed, run by run.
    pub runs: Vec<PsalmRun>,
}

/// A piece of a verse's printed line, of one kind.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PsalmRun {
    pub text: String,
    pub kind: PsalmRunKind,
}

/// What a [`PsalmRun`] is, which says how to style it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PsalmRunKind {
    /// Spaces, and the hyphen of a word the pointing split ("judg-ed").
    Text,
    /// A sung syllable, as printed: accents kept, punctuation attached.
    Syllable(PsalmSyllable),
    /// `·`: the cadence starts at the next syllable. Printed in bold red.
    Point,
    /// `–`: the syllable before is held for a note, or before a half's first syllable, a
    /// note of the tone is left out. Printed in bold red.
    Held,
    /// `*`, the mediant. Printed in red.
    Mediant,
    /// `†`, the flex. Printed in red.
    Flex,
    /// A rubric such as a posture cue (`[Stand.]` in the text), without its brackets.
    /// Printed in red italic.
    Rubric,
}

/// A sung syllable of a [`PsalmDisplay`] verse.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PsalmSyllable {
    pub part: VersePart,
    /// What the syllable sings in the tone: its first note's role.
    pub role: ToneRole,
    /// The syllable carries an acute: it takes an accent of the cadence.
    pub accent: bool,
    /// In a flex, a syllable the voice drops on (after the flex's accent): a pointed psalter
    /// prints it in italic.
    pub flex_drop: bool,
    /// The syllable starts a word.
    pub word_start: bool,
    /// The syllable's UTF-8 bytes in the text.
    pub span: Range<usize>,
}

impl PsalmDisplay {
    /// Points `text` for `tone` as `options` say (`intone`, `auto_point` and `strip_accents`
    /// apply), verse by verse.
    #[must_use]
    pub fn new(text: &str, tone: &Tone, options: &PsalmOptions) -> PsalmDisplay {
        let mut display = PsalmDisplay {
            text: String::new(),
            tone: tone.clone(),
            options: options.clone(),
            verses: Vec::new(),
            diagnostics: Vec::new(),
        };
        display.set(text);
        display
    }

    /// Points new text for the same tone and options. Returns whether anything changed:
    /// `false` when `text` is the current text.
    pub fn update(&mut self, text: &str) -> bool {
        if text == self.text {
            return false;
        }
        self.set(text);
        true
    }

    fn set(&mut self, text: &str) {
        let mut parsed = Pointed::parse(text);
        let mut diagnostics = std::mem::take(&mut parsed.diagnostics);
        let sung = sing(&parsed, &self.tone, &self.options);
        self.verses = verses(&sung, &self.options);
        diagnostics.extend(sung.diagnostics);
        self.diagnostics = diagnostics;
        self.text = text.to_string();
    }

    /// The psalm text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The tone it is pointed for; [`Tone::gabc`] is the line of notes to print above it.
    pub fn tone(&self) -> &Tone {
        &self.tone
    }

    pub fn verses(&self) -> &[PsalmVerse] {
        &self.verses
    }

    /// Problems in the text and its pointing, with spans in the text: those
    /// [`psalm`](crate::psalm()) reports for the same text.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// The verses' runs, written as [`Pointed::to_text`] writes the marks.
fn verses(sung: &SungPsalm, options: &PsalmOptions) -> Vec<PsalmVerse> {
    let mut out = Vec::with_capacity(sung.text.verses.len());
    for (verse, neumes) in sung.text.verses.iter().zip(&sung.neumes) {
        let mut runs = Runs::default();
        for (part, neumes) in verse.parts.iter().zip(neumes) {
            for _ in 0..part.omitted {
                runs.space();
                runs.push("–", PsalmRunKind::Held);
            }
            for (s, notes) in part.syllables.iter().zip(neumes) {
                if s.joint == Joint::Word {
                    runs.space();
                    for r in &s.rubrics {
                        runs.push(r, PsalmRunKind::Rubric);
                        runs.space();
                    }
                    if s.held {
                        runs.push("–", PsalmRunKind::Held);
                        runs.space();
                    }
                    if s.cadence {
                        runs.push("·", PsalmRunKind::Point);
                        runs.space();
                    }
                } else if s.joint == Joint::Hyphen {
                    runs.push("-", PsalmRunKind::Text);
                } else if s.joint == Joint::Dot {
                    runs.push("·", PsalmRunKind::Point);
                }
                let role = notes.first().map_or(ToneRole::Tenor, |n| n.1);
                let text = if options.strip_accents {
                    strip_acutes(&s.text)
                } else {
                    s.text.clone()
                };
                runs.push(
                    &text,
                    PsalmRunKind::Syllable(PsalmSyllable {
                        part: part.kind,
                        role,
                        accent: s.accent,
                        flex_drop: part.kind == VersePart::Flex && role == ToneRole::Ending,
                        word_start: s.joint == Joint::Word,
                        span: s.span.clone(),
                    }),
                );
            }
            for _ in 0..part.held_end {
                runs.space();
                runs.push("–", PsalmRunKind::Held);
            }
            match part.kind {
                VersePart::Flex => {
                    runs.space();
                    runs.push("†", PsalmRunKind::Flex);
                }
                VersePart::Mediant => {
                    runs.space();
                    runs.push("*", PsalmRunKind::Mediant);
                }
                VersePart::Termination => {}
            }
        }
        for r in &verse.end_rubrics {
            runs.space();
            runs.push(r, PsalmRunKind::Rubric);
        }
        out.push(PsalmVerse {
            number: verse.number,
            span: verse.span.clone(),
            runs: runs.0,
        });
    }
    out
}

/// A verse's runs as they are written, with spaces kept to one between pieces.
#[derive(Default)]
struct Runs(Vec<PsalmRun>);

impl Runs {
    fn push(&mut self, text: &str, kind: PsalmRunKind) {
        match self.0.last_mut() {
            Some(last) if kind == PsalmRunKind::Text && last.kind == PsalmRunKind::Text => last.text.push_str(text),
            _ => self.0.push(PsalmRun {
                text: text.to_string(),
                kind,
            }),
        }
    }

    /// A space before the next piece, unless the line is empty or already ends in one.
    fn space(&mut self) {
        if self
            .0
            .last()
            .is_some_and(|r| !(r.kind == PsalmRunKind::Text && r.text.ends_with(' ')))
        {
            self.push(" ", PsalmRunKind::Text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point;

    const PS: &str = "1 Have mercy upon me, O God, after thy great · góodness; * [Sit.] according to the multitude of thy mercies do away · mine offénces.\n\
        4 Against thee only have I sin-ned, † and done this evil in thy · síght; * that thou mightest be justified in thy saying, and clear when · thou art júdg-ed.\n\
        7 Thou · árt – mý God, * and I will · thánk thee. [Bow.]\n\
        8 Lord, remember · Dávid, – – * – – – · práise the Lord.\n";

    fn line(v: &PsalmVerse) -> String {
        v.runs.iter().map(|r| r.text.as_str()).collect()
    }

    fn syllables(v: &PsalmVerse) -> Vec<&PsalmSyllable> {
        v.runs
            .iter()
            .filter_map(|r| match &r.kind {
                PsalmRunKind::Syllable(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn prints_the_pointed_text() {
        let tone = Tone::named("8.G").unwrap();
        let d = PsalmDisplay::new(PS, tone, &PsalmOptions::default());
        // Each line is the pointed text as `point` writes it, but for the number and the
        // brackets of the rubrics.
        let pointed = point(PS, tone).text;
        for (v, p) in d.verses().iter().zip(pointed.lines()) {
            let p = p.split_once(' ').unwrap().1.replace(['[', ']'], "");
            assert_eq!(line(v), p);
        }
        assert_eq!(
            d.verses().iter().map(|v| v.number).collect::<Vec<_>>(),
            [Some(1), Some(4), Some(7), Some(8)]
        );
        // Spans point at the syllables in the text.
        for v in d.verses() {
            for s in syllables(v) {
                assert!(PS[s.span.clone()].chars().any(char::is_alphabetic), "{s:?}");
            }
            assert_eq!(
                &PS[v.span.clone()],
                PS.lines().nth(d.verses().iter().position(|w| w == v).unwrap()).unwrap()
            );
        }
        let kinds = |v: &PsalmVerse| {
            v.runs
                .iter()
                .filter(|r| !matches!(r.kind, PsalmRunKind::Text | PsalmRunKind::Syllable(_)))
                .map(|r| r.text.as_str())
                .collect::<String>()
        };
        assert_eq!(kinds(&d.verses()[0]), "·*Sit.·");
        assert_eq!(kinds(&d.verses()[1]), "†·*·");
        assert_eq!(kinds(&d.verses()[2]), "·–*·Bow.");
        assert_eq!(kinds(&d.verses()[3]), "·––*–––·");
    }

    #[test]
    fn gives_each_syllable_its_place_in_the_tone() {
        let tone = Tone::named("8.G").unwrap();
        let d = PsalmDisplay::new(PS, tone, &PsalmOptions::default());
        let v = syllables(&d.verses()[0]);
        assert_eq!(v[0].role, ToneRole::Intonation);
        let good = v.iter().find(|s| &PS[s.span.clone()] == "góod").unwrap();
        assert!(good.accent && good.word_start && good.role == ToneRole::Accent && good.part == VersePart::Mediant);
        // The flex drops on the syllable after its accent.
        let flex: Vec<_> = syllables(&d.verses()[1])
            .into_iter()
            .filter(|s| s.part == VersePart::Flex)
            .collect();
        let drops: Vec<&str> = flex.iter().filter(|s| s.flex_drop).map(|s| &PS[s.span.clone()]).collect();
        assert_eq!(drops, ["ned,"]);
        assert!(!flex.last().unwrap().word_start);
        // Later verses recite: no intonation.
        assert_eq!(syllables(&d.verses()[2])[0].role, ToneRole::Tenor);
    }

    #[test]
    fn points_unmarked_text_and_follows_updates() {
        let tone = Tone::named("8.G").unwrap();
        let text = "O praise the Lord, all ye heathen * praise him, all ye nations.";
        let mut d = PsalmDisplay::new(text, tone, &PsalmOptions::default());
        assert!(d.verses()[0].runs.iter().any(|r| r.kind == PsalmRunKind::Point));
        assert_eq!(d.diagnostics(), crate::psalm(text, tone, &PsalmOptions::default()).diagnostics);
        assert!(!d.update(text));
        assert!(d.update("For his merciful kindness is ever more and more towards us * and the truth of the Lord endureth for ever."));
        assert_eq!(d.verses().len(), 1);
        // Unsure pointings are reported, as `psalm` reports them.
        let hard = "And the angel said unto them, Fear not: for, behold, I bring you good tidings * which shall be to all people.";
        let d = PsalmDisplay::new(hard, tone, &PsalmOptions::default());
        assert_eq!(d.diagnostics(), crate::psalm(hard, tone, &PsalmOptions::default()).diagnostics);
        // Without auto-pointing nothing is added.
        let plain = PsalmDisplay::new(text, tone, &PsalmOptions::default().with_auto_point(false));
        assert_eq!(line(&plain.verses()[0]), text);
        let stripped = PsalmDisplay::new(
            "Bléssed is he * that cómeth.",
            tone,
            &PsalmOptions::default().with_strip_accents(true),
        );
        assert_eq!(line(&stripped.verses()[0]), "Blessed is he * that cometh.");
    }
}
