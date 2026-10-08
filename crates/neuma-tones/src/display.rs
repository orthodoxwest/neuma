//! [`PsalmDisplay`]: a psalm as a pointed psalter prints it, with the tone shown once as a
//! line of notes ([`Tone::gabc`]) and the verses as text carrying their pointing marks.

use std::ops::Range;

use neuma::Diagnostic;

use crate::apply::{SungPsalm, sing};
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
/// ([`PsalmSyllable::flex_drop`]) in italic. A pointed psalter prints no acute in a flex,
/// which [`Accents::OutsideFlex`](crate::Accents::OutsideFlex) gives. A syllable run gives its
/// place in the text and in the tone, for a tap or a highlight that follows the singing.
/// [`Tone::label`] names the tone above it.
///
/// The runs hold two invisible characters, so that a line never breaks between a mark and
/// its syllable ([`PsalmRunKind::Text`]): U+00A0 (no-break space) and U+2060 (word joiner).
/// A renderer that maps characters to glyphs without shaping the text, as a simple PDF or
/// canvas writer does, should drop U+2060, which has no width but for which a font may draw
/// a missing-glyph box. Text copied from a display keeps the invisible U+2060, so a search of
/// it for "blood-guiltiness" or a typed "–" misses the word; [`psalm`](crate::psalm()) and
/// [`point`](crate::point()) read such text back as its source.
///
/// ```
/// use neuma_tones::{PsalmDisplay, PsalmOptions, PsalmRunKind, Tone};
///
/// let text = "1 Have mercy upon me, O God, after thy great · góodness; * [Sit.] according to thy mercies.";
/// let display = PsalmDisplay::new(text, Tone::named("8.G")?, &PsalmOptions::default());
/// let verse = &display.verses()[0];
/// assert_eq!(verse.number, Some(1));
/// let line: String = verse.runs.iter().map(|r| r.text.as_str()).collect();
/// // A line never breaks between a mark and its syllable: those spaces are U+00A0.
/// assert!(line.starts_with("Have mercy upon me, O God, after thy great ·\u{a0}góodness;\u{a0}* Sit. according"));
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
    /// The text to print.
    pub text: String,
    /// What it is, which says how to style it.
    pub kind: PsalmRunKind,
}

/// What a [`PsalmRun`] is, which says how to style it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PsalmRunKind {
    /// Spaces, and the hyphen of a word the pointing split ("judg-ed") or that is spelled
    /// with one ("blood-guiltiness"). A space that must not break a line, between a `·` and
    /// its syllable or before a `*`, `†` or held `–`, is U+00A0, and a spelling hyphen is
    /// followed by U+2060 (word joiner), as is each `–` ([`Held`](Self::Held)): to search the
    /// text, read U+00A0 as a space and drop U+2060; a renderer that doesn't shape text should
    /// drop U+2060 too. [`psalm`](crate::psalm()) reads both back, a `-` and U+2060 as a
    /// spelling hyphen.
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
    /// The half-verse it is in.
    pub part: VersePart,
    /// What the syllable sings in the tone: its first note's role.
    pub role: ToneRole,
    /// The syllable carries an acute: it takes an accent of the cadence.
    pub accent: bool,
    /// In a flex, a syllable the voice drops on (after the flex's accent), even for one of
    /// its notes: a pointed psalter prints it in italic.
    pub flex_drop: bool,
    /// The syllable starts a word.
    pub word_start: bool,
    /// The syllable's UTF-8 bytes in the text.
    pub span: Range<usize>,
}

impl PsalmDisplay {
    /// Points `text` for `tone` as `options` say (`intone`, `auto_point` and `accents`
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

    /// The verses, pointed for the tone, in order.
    #[must_use]
    pub fn verses(&self) -> &[PsalmVerse] {
        &self.verses
    }

    /// Problems in the text and its pointing, with spans in the text: those
    /// [`psalm`](crate::psalm()) reports for the same text.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// The verses' runs, written as [`Pointed::to_text`] writes the marks. The space between a
/// `·` and its syllable, and before a `*`, `†` or held `–`, is U+00A0, and a word joiner
/// (U+2060) follows each `–` and spelling hyphen, so a line never breaks between a mark and
/// the syllable it belongs to: an en dash allows a break after it even before U+00A0 (UAX #14,
/// LB12a). Pointed text reads both, so a line copied from the display sets as its source.
fn verses(sung: &SungPsalm, options: &PsalmOptions) -> Vec<PsalmVerse> {
    let mut out = Vec::with_capacity(sung.text.verses.len());
    for (verse, neumes) in sung.text.verses.iter().zip(&sung.neumes) {
        let mut runs = Runs::default();
        for (part, neumes) in verse.parts.iter().zip(neumes) {
            // Dashes before the first syllable go with it, the `·` among them where written.
            let after = part.omitted_after_point.min(part.omitted);
            for r in &part.lead_rubrics {
                runs.space();
                runs.push(r, PsalmRunKind::Rubric);
            }
            for k in 0..part.omitted {
                if k == 0 {
                    runs.space();
                } else {
                    runs.nbsp();
                }
                if after > 0 && k == part.omitted - after {
                    runs.push("·", PsalmRunKind::Point);
                    runs.nbsp();
                }
                runs.push(HELD, PsalmRunKind::Held);
            }
            for (i, (s, notes)) in part.syllables.iter().zip(neumes).enumerate() {
                match s.joint {
                    Joint::Word => {
                        if i == 0 && part.omitted > 0 {
                            runs.nbsp();
                        } else {
                            runs.space();
                        }
                        for r in &s.rubrics {
                            runs.push(r, PsalmRunKind::Rubric);
                            runs.space();
                        }
                        if s.held {
                            runs.nbsp();
                            runs.push(HELD, PsalmRunKind::Held);
                            runs.space();
                        }
                        if s.cadence && !(i == 0 && after > 0) {
                            runs.push("·", PsalmRunKind::Point);
                            runs.nbsp();
                        }
                    }
                    Joint::Hyphen => runs.push("-", PsalmRunKind::Text),
                    Joint::Spelling => {
                        runs.push(SPELLING, PsalmRunKind::Text);
                        if s.cadence {
                            runs.push("·", PsalmRunKind::Point);
                        }
                    }
                    Joint::Dot => runs.push("·", PsalmRunKind::Point),
                    Joint::Split => {}
                }
                let role = notes.first().map_or(ToneRole::Tenor, |n| n.1);
                runs.push(
                    &options.accents.shown(&s.text, part.kind),
                    PsalmRunKind::Syllable(PsalmSyllable {
                        part: part.kind,
                        role,
                        accent: s.accent,
                        flex_drop: part.kind == VersePart::Flex && notes.iter().any(|n| n.1 == ToneRole::Ending),
                        word_start: s.joint == Joint::Word,
                        span: s.span.clone(),
                    }),
                );
            }
            for _ in 0..part.held_end {
                runs.nbsp();
                runs.push(HELD, PsalmRunKind::Held);
            }
            match part.kind {
                VersePart::Flex => {
                    runs.nbsp();
                    runs.push("†", PsalmRunKind::Flex);
                }
                VersePart::Mediant => {
                    runs.nbsp();
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

/// A verse's runs as they are written, with one space between pieces.
#[derive(Default)]
struct Runs(Vec<PsalmRun>);

const NBSP: char = '\u{a0}';
/// A held `–`, with a word joiner so the line can't break after it.
const HELD: &str = "–\u{2060}";
/// A spelling hyphen: a plain `-`, which every font has, and a word joiner.
const SPELLING: &str = "-\u{2060}";

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
            .is_some_and(|r| !(r.kind == PsalmRunKind::Text && r.text.ends_with([' ', NBSP])))
        {
            self.push(" ", PsalmRunKind::Text);
        }
    }

    /// A space the line can't break at, unless the line is empty: it replaces a plain one.
    fn nbsp(&mut self) {
        match self.0.last_mut() {
            None => {}
            Some(r) if r.kind == PsalmRunKind::Text && r.text.ends_with(NBSP) => {}
            Some(r) if r.kind == PsalmRunKind::Text && r.text.ends_with(' ') => {
                r.text.pop();
                r.text.push(NBSP);
            }
            Some(_) => self.push("\u{a0}", PsalmRunKind::Text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Accents, point};

    const PS: &str = "1 Have mercy upon me, O God, after thy great · góodness; * [Sit.] according to the multitude of thy mercies do away · mine offénces.\n\
        4 Against thee only have I sin-ned, † and done this evil in thy · síght; * that thou mightest be justified in thy saying, and clear when · thou art júdg-ed.\n\
        7 Thou · árt – mý God, * and I will · thánk thee. [Bow.]\n\
        8 Lord, remember · Dávid, – – * – – – · práise the Lord.\n";

    /// The verse's line, with its non-breaking spaces as plain ones.
    fn line(v: &PsalmVerse) -> String {
        v.runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<String>()
            .replace(NBSP, " ")
            .replace('\u{2060}', "")
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
                .replace('\u{2060}', "")
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
            &PsalmOptions::default().with_accents(Accents::None),
        );
        assert_eq!(line(&stripped.verses()[0]), "Blessed is he * that cometh.");
    }

    #[test]
    fn reads_its_own_lines_back() {
        // A line copied from the display, U+00A0 and U+2060 and all, sets and points as the
        // text it came from; so does one with U+2011 for its spelling hyphens.
        let text = "1 Deliver me from blood\\-guiltiness, O God, thou that art the God of my · héalth; * and my tongue shall sing of thy · ríghteousness.\n\
            2 For thou desirest no sacrifice, † else would I give it thee * but thou delightest not in burnt-offerings.\n\
            3 My soul thirsteth for thée, † my flesh also longeth after · thée * in a barren and dry land · where no wáter is.\n\
            4 Lord, remember · Dávid, – – * – · – – práise the Lord.\n\
            5 Thou hast the pre-·eminence, and c\\-\\-d \\-dashes\\-, * and hon·-our and well\\-·plé-a-sing.\n";
        // With every acute printed: one taken out of a flex can't be read back.
        let options = PsalmOptions::default();
        for name in ["8.G", "1.D", "per"] {
            let tone = Tone::named(name).unwrap();
            let d = PsalmDisplay::new(text, tone, &options);
            let copied: String = d
                .verses()
                .iter()
                .map(|v| {
                    let line: String = v.runs.iter().map(|r| r.text.as_str()).collect();
                    format!("{} {line}\n", v.number.unwrap())
                })
                .collect();
            assert!(copied.contains('\u{a0}') && copied.contains("-\u{2060}") && copied.contains("–\u{2060}"));
            let nb_hyphen = copied.replace("-\u{2060}", "\u{2011}");
            for copy in [&copied, &nb_hyphen] {
                assert_eq!(
                    crate::psalm(copy, tone, &options).gabc,
                    crate::psalm(text, tone, &options).gabc,
                    "{name}"
                );
                assert_eq!(point(copy, tone).text, point(text, tone).text, "{name}");
            }
        }
    }

    #[test]
    fn keeps_spelling_hyphens() {
        let tone = Tone::named("8.G").unwrap();
        let text = "Deliver me from blood\\-guiltiness, O God, thou that art the God of my · héalth; * and my tongue shall sing of thy · ríghteousness.\n\
            For thou art my · hópe * thou hast the pre\\-·eminence.";
        let d = PsalmDisplay::new(text, tone, &PsalmOptions::default());
        // A word joiner after a spelling hyphen: the line never breaks after "pre-".
        let raw = |v: &PsalmVerse| v.runs.iter().map(|r| r.text.as_str()).collect::<String>();
        assert!(raw(&d.verses()[0]).starts_with("Deliver me from blood-\u{2060}guiltiness, O God"));
        assert!(
            raw(&d.verses()[1]).ends_with("thou hast the pre-\u{2060}·eminence."),
            "{}",
            raw(&d.verses()[1])
        );
        // Each piece is its own syllable, with its own span.
        let pieces: Vec<&str> = syllables(&d.verses()[0])
            .iter()
            .map(|s| &text[s.span.clone()])
            .skip(5)
            .take(4)
            .collect();
        assert_eq!(pieces, ["blood", "guil", "ti", "ness,"]);
        // `point` writes them back, and the cadence starts after the hyphen.
        let pointed = point(text, tone).text;
        assert!(
            pointed.contains("blood\\-guiltiness") && pointed.contains("pre\\-·eminence"),
            "{pointed}"
        );
        let pre = syllables(&d.verses()[1])
            .into_iter()
            .find(|s| &text[s.span.clone()] == "e")
            .unwrap();
        assert!(!pre.word_start && pre.role == ToneRole::Preparatory, "{pre:?}");
    }

    #[test]
    fn binds_marks_to_their_syllables() {
        let tone = Tone::named("8.G").unwrap();
        let text = "My soul thirsteth for thée, † my flesh also longeth after · thée * in a barren and dry land · where no wáter is.\n\
            Lord, remember · Dávid, – – * – · – – práise the Lord.";
        let d = PsalmDisplay::new(text, tone, &PsalmOptions::default());
        let raw: String = d.verses()[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert!(
            raw.contains("thée,\u{a0}† my") && raw.contains("after ·\u{a0}thée\u{a0}* in"),
            "{raw:?}"
        );
        assert!(raw.contains("land ·\u{a0}where"), "{raw:?}");
        let raw: String = d.verses()[1].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            raw,
            "Lord, remember ·\u{a0}Dávid,\u{a0}–\u{2060}\u{a0}–\u{2060}\u{a0}* –\u{2060}\u{a0}·\u{a0}–\u{2060}\u{a0}–\u{2060}\u{a0}práise the Lord."
        );
        // A held syllable in a flex drops on its second note.
        let flex: Vec<_> = syllables(&d.verses()[0])
            .into_iter()
            .filter(|s| s.part == VersePart::Flex)
            .collect();
        let last = flex.last().unwrap();
        assert!(last.flex_drop && last.role == ToneRole::Accent, "{last:?}");
        assert_eq!(flex.iter().filter(|s| s.flex_drop).count(), 1);
    }

    #[test]
    fn prints_accents_as_asked() {
        let tone = Tone::named("8.G").unwrap();
        let text = "Know this also, that the Lord hath chosen to · himsélf † the man that is · gódly * when I call upon the Lord, · hé will hear me.";
        let shown = |accents| line(&PsalmDisplay::new(text, tone, &PsalmOptions::default().with_accents(accents)).verses()[0]);
        assert_eq!(shown(Accents::All), text);
        assert_eq!(
            shown(Accents::OutsideFlex),
            "Know this also, that the Lord hath chosen to · himself † the man that is · gódly * when I call upon the Lord, · hé will hear me."
        );
        assert_eq!(shown(Accents::None), text.replace(['é', 'ó'], "e").replace("gedly", "godly"));
    }

    #[test]
    fn reads_the_number_after_an_opening_rubric() {
        let tone = Tone::named("8.G").unwrap();
        let text = "[Stand.] 5 For I acknowledge my · fáults * and my sin is ever · befóre me.";
        let d = PsalmDisplay::new(text, tone, &PsalmOptions::default());
        assert_eq!(d.verses()[0].number, Some(5));
        assert!(line(&d.verses()[0]).starts_with("Stand. For I"));
        assert_eq!(d.verses()[0].runs[0].kind, PsalmRunKind::Rubric);
    }
}
