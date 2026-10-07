//! Sets pointed text to a psalm tone.

use std::ops::Range;

use neuma::score::{Bar, BarKind, Figure, Lyric};
use neuma::{Diagnostic, Score, ScoreBuilder, Severity};

use crate::pointed::{self, Part, Pointed, Syllable, VersePart};
use crate::syllable::fold;
use crate::tone::{Cadence, Slot, Tone};

/// When the intonation is sung.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Intone {
    /// On the first verse only, as at the Office.
    #[default]
    FirstVerse,
    /// On every verse, as in the Magnificat and Benedictus.
    EveryVerse,
    /// Never: every verse starts on the tenor.
    Never,
}

/// Which acute accents the printed text keeps. They place the cadence's accents either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Accents {
    /// Every acute, as written or added by the pointing.
    #[default]
    All,
    /// None: the text as it is spelled.
    None,
    /// None in a flex half-verse, which a pointed psalter shows by its italics alone; the
    /// others keep theirs.
    OutsideFlex,
}

impl Accents {
    /// A syllable's text as printed in `part`.
    pub(crate) fn shown(self, text: &str, part: VersePart) -> String {
        match (self, part) {
            (Accents::None, _) | (Accents::OutsideFlex, VersePart::Flex) => strip_acutes(text),
            _ => text.to_string(),
        }
    }
}

/// How to set a psalm. Build it with the `with_*` setters:
/// `PsalmOptions::default().with_intone(Intone::EveryVerse)`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PsalmOptions {
    /// When the intonation is sung; default on the first verse.
    pub intone: Intone,
    /// Which acute accents the printed text keeps; default all.
    pub accents: Accents,
    /// The score's `name:` header.
    pub name: Option<String>,
    /// Point half-verses that carry no marks with [`point`](crate::point()) first; default
    /// true. Without it their cadence falls on the last syllables.
    pub auto_point: bool,
}

impl Default for PsalmOptions {
    fn default() -> PsalmOptions {
        PsalmOptions {
            intone: Intone::default(),
            accents: Accents::All,
            name: None,
            auto_point: true,
        }
    }
}

impl PsalmOptions {
    /// Sets [`intone`](Self::intone).
    #[must_use]
    pub fn with_intone(mut self, intone: Intone) -> PsalmOptions {
        self.intone = intone;
        self
    }

    /// Sets [`accents`](Self::accents).
    #[must_use]
    pub fn with_accents(mut self, accents: Accents) -> PsalmOptions {
        self.accents = accents;
        self
    }

    /// Sets [`name`](Self::name).
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> PsalmOptions {
        self.name = Some(name.into());
        self
    }

    /// Sets [`auto_point`](Self::auto_point).
    #[must_use]
    pub fn with_auto_point(mut self, auto_point: bool) -> PsalmOptions {
        self.auto_point = auto_point;
        self
    }
}

/// Below this confidence an automatically pointed half-verse is reported for checking.
pub const UNSURE: f32 = 0.8;

/// What a note does in the tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ToneRole {
    Intonation,
    /// The reciting note.
    Tenor,
    Preparatory,
    Accent,
    /// The notes after an accent: passing notes between accents and the cadence's ending.
    Ending,
}

/// One note of the setting, in the order the engraving numbers notes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PsalmNote {
    /// Index into the pointed text's verses.
    pub verse: usize,
    /// The printed verse number.
    pub number: Option<u32>,
    pub part: VersePart,
    pub role: ToneRole,
    /// The sung syllable's UTF-8 bytes in the psalm text: the same span as the engraved
    /// note's (`TimelineNote::span`).
    pub span: Range<usize>,
}

/// Psalm text set to a tone.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PsalmSetting {
    /// The psalm text the setting was made from.
    pub text: String,
    /// The setting as a score whose spans count UTF-8 bytes of the psalm text: a note's is its
    /// sung syllable's, a syllable's its text, and a bar's is empty at the end of its
    /// half-verse. A [`PsalmChant`](crate::PsalmChant) engraves it, and its hit tests and
    /// timeline answer in the text.
    pub score: Score,
    /// The score as GABC, for a GABC editor or file (parsed again, its spans count GABC).
    pub gabc: String,
    /// Every note's place in the tone: `notes[i]` is note `i` of the engraved score.
    pub notes: Vec<PsalmNote>,
    /// Problems in the text and its pointing, with spans in the text.
    pub diagnostics: Vec<Diagnostic>,
}

/// Sets psalm text (a verse a line, the mediant marked `*`, optionally pointed with `†`, `·`,
/// acutes and `–`) to `tone`. Half-verses with no marks are pointed automatically first
/// (unless [`PsalmOptions::auto_point`] is off); a pointing the model is unsure of is reported
/// as `point::unsure`.
///
/// ```
/// use neuma_tones::{PsalmOptions, Tone, psalm};
///
/// let text = "O praise the Lord, all ye heathen * praise him, all ye nations.";
/// let setting = psalm(text, Tone::named("8.G").unwrap(), &PsalmOptions::default());
/// assert_eq!(&text[setting.notes[0].span.clone()], "O");
/// ```
#[must_use]
pub fn psalm(text: &str, tone: &Tone, options: &PsalmOptions) -> PsalmSetting {
    let mut parsed = Pointed::parse(text);
    let mut diags = std::mem::take(&mut parsed.diagnostics);
    let mut setting = set_pointed(&parsed, tone, options);
    diags.append(&mut setting.diagnostics);
    setting.diagnostics = diags;
    setting.text = text.to_string();
    setting
}

/// What each syllable of a psalm sings: the text as set (pointed, if `auto_point`, and split
/// into sung syllables), each syllable's neumes and their roles, verse by verse and part by
/// part, and the setting's own diagnostics.
pub(crate) struct SungPsalm {
    pub text: Pointed,
    pub neumes: Vec<Vec<Sung>>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Sets each syllable of `pointed` to `tone`'s notes, as [`psalm`] and
/// [`PsalmDisplay`](crate::PsalmDisplay) both need.
pub(crate) fn sing(pointed: &Pointed, tone: &Tone, options: &PsalmOptions) -> SungPsalm {
    let mut unsure = Vec::new();
    let text = if !options.auto_point {
        pointed.syllabified()
    } else {
        let (text, halves) = crate::point::point_parsed(pointed, tone);
        for h in halves.iter().filter(|h| !h.kept && h.confidence < UNSURE && !h.span.is_empty()) {
            unsure.push((h.span.clone(), h.confidence));
        }
        text
    };
    let mut diags = Vec::new();
    if text.verses.is_empty() {
        warn(&mut diags, Severity::Warning, 0..0, "apply::empty", "there is no verse to sing");
    }
    for (span, confidence) in unsure {
        warn(
            &mut diags,
            Severity::Info,
            span,
            "point::unsure",
            &format!(
                "pointed automatically, but only {:.0}% sure: check where the accents fall",
                confidence * 100.0
            ),
        );
    }
    let mut neumes = Vec::with_capacity(text.verses.len());
    for (vi, verse) in text.verses.iter().enumerate() {
        let mut parts = Vec::with_capacity(verse.parts.len());
        for (pi, part) in verse.parts.iter().enumerate() {
            let cadence = match part.kind {
                VersePart::Flex => &tone.flex,
                VersePart::Mediant => &tone.mediant,
                VersePart::Termination => &tone.termination,
            };
            // The intonation opens the verse, whichever part comes first.
            let intone = pi == 0
                && match options.intone {
                    Intone::FirstVerse => vi == 0,
                    Intone::EveryVerse => true,
                    Intone::Never => false,
                };
            let lead: &[String] = if part.kind == VersePart::Termination {
                &cadence.lead
            } else if intone {
                &tone.mediant.lead
            } else {
                &[]
            };
            parts.push(set_part(part, cadence, lead, &mut diags));
        }
        neumes.push(parts);
    }
    SungPsalm {
        text,
        neumes,
        diagnostics: diags,
    }
}

/// [`psalm`] for text already parsed, with only the setting's own diagnostics.
fn set_pointed(pointed: &Pointed, tone: &Tone, options: &PsalmOptions) -> PsalmSetting {
    let SungPsalm {
        text,
        neumes: sung_psalm,
        diagnostics: diags,
    } = sing(pointed, tone, options);
    let mut b = ScoreBuilder::new();
    if let Some(name) = &options.name {
        b = b.header("name", name);
    }
    b = b.clef(tone.clef, tone.clef_line);
    let mut notes = Vec::new();
    let mut figures = Figures::default();
    // Each syllable's span in the text, after the clef's.
    let mut spans = Vec::new();
    let last_verse = text.verses.len().saturating_sub(1);
    for ((vi, verse), sung_verse) in text.verses.iter().enumerate().zip(&sung_psalm) {
        for (part, sung) in verse.parts.iter().zip(sung_verse) {
            for (s, neumes) in part.syllables.iter().zip(sung) {
                let mut fig = Vec::new();
                for (neume, role) in neumes {
                    let mut f = figures.get(neume);
                    for figure in &mut f {
                        match figure {
                            Figure::Note(n) => {
                                n.span = s.span.clone();
                                notes.push(PsalmNote {
                                    verse: vi,
                                    number: verse.number,
                                    part: part.kind,
                                    role: *role,
                                    span: s.span.clone(),
                                });
                            }
                            Figure::Alteration(a) => a.span = s.span.clone(),
                            _ => {}
                        }
                    }
                    fig.extend(f);
                }
                // A hyphen left in the text is printed in the pointed text but not sung: the
                // engraver draws the hyphens between syllables itself.
                let shown = options.accents.shown(&s.text, part.kind).replace('-', "");
                b = b.syllable(Lyric::from_plain(&shown), s.starts_word(), fig);
                spans.push(s.span.clone());
            }
            let end = part.syllables.last().map_or(verse.span.end, |s| s.span.end);
            let (mark, kind) = match part.kind {
                VersePart::Flex => ("†", BarKind::Quarter),
                VersePart::Mediant => ("*", BarKind::Full),
                VersePart::Termination => ("", if vi == last_verse { BarKind::Double } else { BarKind::Full }),
            };
            let bar = vec![Figure::Bar(Bar::new(kind, end..end))];
            b = b.syllable(Lyric::from_plain(mark), true, bar);
            spans.push(end..end);
        }
    }
    let mut score = b.build();
    // The clef's syllable, then the sung ones and the bars: each gets its span in the text.
    for (syl, span) in score.syllables.iter_mut().rev().zip(spans.into_iter().rev()) {
        syl.span = span;
    }
    PsalmSetting {
        text: String::new(),
        gabc: score.to_gabc(),
        score,
        notes,
        diagnostics: diags,
    }
}

/// GABC neumes parsed into figures, once each.
#[derive(Default)]
struct Figures(Vec<(String, Vec<Figure>)>);

impl Figures {
    fn get(&mut self, neume: &str) -> Vec<Figure> {
        if let Some((_, f)) = self.0.iter().find(|(n, _)| n == neume) {
            return f.clone();
        }
        let parsed = neuma::parse(&format!("(c4) a({neume})"));
        let f: Vec<Figure> = parsed
            .score
            .syllables
            .into_iter()
            .last()
            .map(|s| s.notation.into_iter().filter(|f| !matches!(f, Figure::Clef(_))).collect())
            .unwrap_or_default();
        self.0.push((neume.to_string(), f.clone()));
        f
    }
}

pub(crate) fn strip_acutes(text: &str) -> String {
    text.chars()
        .filter(|&c| c != '\u{301}')
        .map(|c| {
            if pointed::has_acute(&c.to_string()) {
                let f = fold(c);
                if c.is_uppercase() {
                    f.to_uppercase().next().unwrap_or(f)
                } else {
                    f
                }
            } else {
                c
            }
        })
        .collect()
}

type Sung = Vec<Vec<(String, ToneRole)>>;

fn warn(diags: &mut Vec<Diagnostic>, severity: Severity, span: Range<usize>, code: &'static str, message: &str) {
    diags.push(Diagnostic::new(severity, span, code, message));
}

/// The neumes each syllable of `part` takes.
fn set_part(part: &Part, cadence: &Cadence, lead: &[String], diags: &mut Vec<Diagnostic>) -> Sung {
    let n = part.syllables.len();
    let mut out = if part.held_end > 0 && n > 0 {
        // Dashes after the last syllable stand for notes it holds: set them as syllables of
        // their own, the first ones taking any accents the marks leave, then give their notes
        // to the last real syllable ("Dá-vid, – – *").
        let mut ext = part.clone();
        ext.held_end = 0;
        let start = part.syllables.iter().rposition(|s| s.cadence).unwrap_or(0);
        let marked = part.syllables[start..].iter().filter(|s| s.accent).count();
        let missing = if marked > 0 { cadence.accents().saturating_sub(marked) } else { 0 };
        let end = part.syllables[n - 1].span.end;
        for k in 0..part.held_end {
            ext.syllables.push(Syllable {
                span: end..end,
                accent: k < missing,
                ..Syllable::default()
            });
        }
        let mut sung = set_cadence(&ext, cadence, lead, diags);
        let held: Vec<(String, ToneRole)> = sung.drain(n..).flatten().collect();
        sung[n - 1].extend(held);
        sung
    } else {
        set_cadence(part, cadence, lead, diags)
    };
    // Every syllable sings something: one left without a note takes the note before it.
    for i in 0..out.len() {
        if out[i].is_empty() {
            let prev = if i > 0 { out[i - 1].last().cloned() } else { None };
            out[i].push(prev.unwrap_or_else(|| (cadence.tenor.clone(), ToneRole::Tenor)));
        }
    }
    out
}

/// [`set_part`] for a half without trailing dashes.
fn set_cadence(part: &Part, cadence: &Cadence, lead: &[String], diags: &mut Vec<Diagnostic>) -> Sung {
    let syls = &part.syllables;
    let n = syls.len();
    let mut out: Sung = vec![Vec::new(); n];
    if n == 0 {
        return out;
    }
    let part_span = syls[0].span.start..syls[n - 1].span.end;
    // Split the slots into the preparatory run and one unit per accent.
    let first_accent = cadence
        .slots
        .iter()
        .position(|s| matches!(s, Slot::Accent(_)))
        .unwrap_or(cadence.slots.len());
    let prep: &[Slot] = &cadence.slots[..first_accent];
    let mut units: Vec<(&str, &[Slot])> = Vec::new();
    let mut k = first_accent;
    while k < cadence.slots.len() {
        let Slot::Accent(a) = &cadence.slots[k] else { break };
        let end = cadence.slots[k + 1..]
            .iter()
            .position(|s| matches!(s, Slot::Accent(_)))
            .map_or(cadence.slots.len(), |p| k + 1 + p);
        units.push((a.as_str(), &cadence.slots[k + 1..end]));
        k = end;
    }
    let wanted = units.len();
    if wanted == 0 {
        // A formula with no accent (only a hand-built tone can have one): recite it all.
        return vec![vec![(cadence.tenor.clone(), ToneRole::Tenor)]; n];
    }

    // Where the cadence starts and which syllables carry its accents.
    let start = syls.iter().rposition(|s| s.cadence);
    let marked: Vec<usize> = (start.unwrap_or(0)..n).filter(|&i| syls[i].accent).collect();
    let (start, accents) = if marked.is_empty() {
        // No accent marked: the flex drops on its last syllable, and other halves guess.
        if part.kind != VersePart::Flex || start.is_some() {
            warn(
                diags,
                Severity::Warning,
                part_span.clone(),
                "apply::no-accent",
                "no accented syllable is marked in this cadence, so the last syllables take it",
            );
        }
        let tail = units
            .last()
            .map_or(0, |u| u.1.iter().filter(|s| matches!(s, Slot::Fixed(_))).count());
        let mut acc = Vec::new();
        let mut at = n.saturating_sub(1 + tail.min(n.saturating_sub(1)));
        for _ in 0..wanted {
            acc.insert(0, at);
            if at < 2 {
                break;
            }
            at -= 2;
        }
        let first = acc.first().copied().unwrap_or(0);
        (
            start.unwrap_or_else(|| first.saturating_sub(prep.iter().filter(|s| matches!(s, Slot::Fixed(_))).count())),
            acc,
        )
    } else {
        if start.is_none() && part.kind != VersePart::Flex {
            warn(
                diags,
                Severity::Info,
                syls[marked[0]].span.clone(),
                "apply::no-cadence-mark",
                "no `·` in this half-verse, so its cadence is counted from the accents",
            );
        }
        let acc: Vec<usize> = marked[marked.len().saturating_sub(wanted)..].to_vec();
        if marked.len() > wanted {
            warn(
                diags,
                Severity::Info,
                syls[marked[0]].span.clone(),
                "apply::extra-accents",
                &format!("the tone has {wanted} accent(s) here, so only the last are used"),
            );
        }
        let first = acc[0];
        let p = prep.iter().filter(|s| matches!(s, Slot::Fixed(_))).count();
        (start.unwrap_or(first.saturating_sub(p)).min(first), acc)
    };
    if accents.len() < wanted {
        warn(
            diags,
            Severity::Warning,
            part_span.clone(),
            "apply::missing-accent",
            &format!("the tone has {wanted} accents here but {} are marked", accents.len()),
        );
    }
    let units = &units[wanted - accents.len().min(wanted)..];

    // Before the cadence: the intonation, then the tenor.
    for (i, slot) in out.iter_mut().enumerate().take(start) {
        match lead.get(i) {
            Some(neume) => slot.push((neume.clone(), ToneRole::Intonation)),
            None => slot.push((cadence.tenor.clone(), ToneRole::Tenor)),
        }
    }
    if lead.len() > start && !lead.is_empty() {
        warn(
            diags,
            Severity::Info,
            part_span.clone(),
            "apply::short-intonation",
            "the half-verse is too short for the whole intonation",
        );
    }

    // Preparatory syllables.
    let first = accents[0].max(start);
    let preps = first - start;
    let fixed: Vec<&String> = prep
        .iter()
        .filter_map(|s| if let Slot::Fixed(f) = s { Some(f) } else { None })
        .collect();
    let open = prep.iter().find_map(|s| if let Slot::Open(o) = s { Some(o) } else { None });
    if preps > fixed.len() && open.is_none() {
        // More syllables than preparatory notes: the first ones stay on the tenor.
        let extra = preps - fixed.len();
        for slot in out.iter_mut().skip(start).take(extra) {
            slot.push((cadence.tenor.clone(), ToneRole::Tenor));
        }
        for (j, f) in fixed.iter().enumerate() {
            out[start + extra + j].push(((*f).clone(), ToneRole::Preparatory));
        }
        if start < first {
            warn(
                diags,
                Severity::Info,
                syls[start].span.clone(),
                "apply::extra-preparatory",
                &format!(
                    "the tone has {} preparatory note(s); the syllables before them stay on the tenor",
                    fixed.len()
                ),
            );
        }
    } else if preps < fixed.len() {
        // Fewer: the first preparatory notes are left out, which dashes before the first
        // syllable say on purpose.
        for (j, f) in fixed[fixed.len() - preps..].iter().enumerate() {
            out[start + j].push(((*f).clone(), ToneRole::Preparatory));
        }
        if part.omitted < fixed.len() - preps {
            warn(
                diags,
                Severity::Warning,
                syls[start.min(n - 1)].span.clone(),
                "apply::few-preparatory",
                &format!("the tone has {} preparatory note(s) but the pointing gives {preps}", fixed.len()),
            );
        }
    } else {
        fill(&mut out[start..first], prep, ToneRole::Preparatory);
    }

    // The accents and what follows each.
    for (u, (&at, (accent, after))) in accents.iter().zip(units).enumerate() {
        out[at].push((accent.to_string(), ToneRole::Accent));
        let next = accents.get(u + 1).copied().unwrap_or(n);
        let last = u + 1 == accents.len();
        let gap = next - at - 1;
        if gap == 0 {
            // Nothing between: the notes are sung on the accent. Between two accents that
            // is the held note the pointing marks with a dash; at the end, the open note
            // is dropped.
            for s in *after {
                match s {
                    Slot::Open(o) if !last => out[at].push((o.clone(), ToneRole::Ending)),
                    Slot::Fixed(f) => out[at].push((f.clone(), ToneRole::Ending)),
                    _ => {}
                }
            }
            if !last && !syls[next].held {
                warn(
                    diags,
                    Severity::Info,
                    syls[next].span.clone(),
                    "apply::implied-hold",
                    "two accents with nothing between: the first is held, which the pointing usually marks with `–`",
                );
            }
        } else {
            if !last && syls[next].held {
                warn(
                    diags,
                    Severity::Warning,
                    syls[next].span.clone(),
                    "apply::needless-hold",
                    "a `–` hold, but syllables come between the accents",
                );
            }
            fill(&mut out[at + 1..next], after, ToneRole::Ending);
        }
    }
    out
}

/// Gives `syls` syllables the slots in order: one each for a fixed slot, and any extra
/// syllables to the open slot. With too few syllables, the leftover fixed notes are sung on the
/// last syllable; with too many and no open slot, the extras repeat the last note.
fn fill(syls: &mut [Vec<(String, ToneRole)>], slots: &[Slot], role: ToneRole) {
    let k = syls.len();
    if k == 0 {
        return;
    }
    let fixed = slots.iter().filter(|s| matches!(s, Slot::Fixed(_))).count();
    let mut extra = k.saturating_sub(fixed);
    let mut i = 0;
    let has_open = slots.iter().any(|s| matches!(s, Slot::Open(_)));
    for s in slots {
        match s {
            Slot::Fixed(f) => {
                let at = i.min(k - 1);
                syls[at].push((f.clone(), role));
                i += 1;
            }
            Slot::Open(o) => {
                while extra > 0 && i < k {
                    syls[i].push((o.clone(), role));
                    i += 1;
                    extra -= 1;
                }
            }
            Slot::Accent(_) => {}
        }
    }
    if !has_open && extra > 0 {
        // Repeat the last note on the extra syllables (none yet: the caller fills them).
        let Some(prev) = i.min(k).checked_sub(1) else { return };
        let last = syls[prev].last().cloned();
        if let Some(note) = last {
            for s in syls.iter_mut().skip(i) {
                s.push(note.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neuma::{ApproxMeasure, StyleOptions};

    fn set(tone: &str, text: &str) -> PsalmSetting {
        psalm(text, Tone::named(tone).unwrap(), &PsalmOptions::default())
    }

    /// The notes on each syllable, as `text(notes)`.
    fn sung(s: &PsalmSetting) -> String {
        s.score
            .syllables
            .iter()
            .filter(|x| !x.text.is_empty())
            .map(|x| format!("{}({})", x.text.plain(), notes(&x.notation)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn notes(f: &[Figure]) -> String {
        f.iter()
            .filter_map(|f| match f {
                Figure::Note(n) => neuma::score::position_letter(n.position),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn tone_eight_one_accent() {
        // 8.G: intonation g h, tenor j, mediant 'k j, termination i j 'h g.
        let s = set(
            "8.G",
            "The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength.",
        );
        assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
        let out = sung(&s);
        assert!(out.starts_with("The(g) Lord(h) is(j) King,(j)"), "{out}");
        assert!(out.contains("glo(j) ri(j) ous(j) ap(j) pá(k) rel;(j) *()"), "{out}");
        assert!(out.ends_with("gird(j) ed(i) him(j) sélf(h) with(g) strength.(g)"), "{out}");
    }

    #[test]
    fn accent_on_the_last_syllable() {
        // An accent with no syllable after it takes the ending on itself (8.G mediant: kj).
        let s = set("8.G", "He hath made the round world so · súre, * that it can·not be móv-ed.");
        let out = sung(&s);
        assert!(out.contains("so(j) súre,(kj)"), "{out}");
        assert!(out.ends_with("can(j) not(i) be(j) móv(h) ed.(g)"), "{out}");
    }

    #[test]
    fn two_accents_with_a_held_note() {
        // 7.a mediant: tenor i, 'k jr 'i jr j. "thou · árt – mý God": árt holds k and j.
        let s = set("7.a", "O God, thou · árt – mý God; * early will I · séek thée.");
        let out = sung(&s);
        assert!(out.contains("thou(i) árt(kj) mý(i) God;(j)"), "{out}");
        assert!(
            !s.diagnostics.iter().any(|d| d.severity >= Severity::Warning),
            "{:?}",
            s.diagnostics
        );
    }

    #[test]
    fn flex_and_roles_line_up_with_the_engraving() {
        let text = "4 Against thee only have I sin-ned, † and done this evil in thy · síght; * that thou mightest be justified in thy saying, and clear when thou art · júdg-ed.\n\
            5 Behold, I was shapen in · wíck-ed-ness, * and in sin hath my · móther con-céiv-ed me.";
        let s = set("1.g", text);
        let out = sung(&s);
        // The flex drops a step from the tenor (la to sol) on the last syllable.
        assert!(out.starts_with("A(f) gainst(gh) thee(h)"), "{out}");
        assert!(out.contains("I(h) sin(h) ned,(g) †()"), "{out}");
        let eng = neuma::parse(&s.gabc).score.engrave(&ApproxMeasure, &StyleOptions::default());
        let map = eng.layout(800.0).timeline();
        assert_eq!(map.notes.len(), s.notes.len());
        // Every note's role points at its syllable in the pointed text.
        for (n, r) in map.notes.iter().zip(&s.notes) {
            let src = text[r.span.clone()].replace('-', "");
            assert!(src.contains(n.syllable_text.as_str()), "{src} vs {}", n.syllable_text);
        }
        // The second verse isn't intoned.
        let v2: Vec<_> = s.notes.iter().filter(|r| r.verse == 1).collect();
        assert!(v2.iter().all(|r| r.role != ToneRole::Intonation));
        assert_eq!(v2[0].number, Some(5));
        // The verse ends on a full bar and the psalm on a double bar.
        assert!(s.gabc.contains("(:)") && s.gabc.trim_end().ends_with("(::)"), "{}", s.gabc);
    }

    #[test]
    fn trailing_dashes_and_odd_tones() {
        // The dashes after "Dá-vid," hold its last syllable for the second accent and the end.
        let tone = Tone::named("7.a").unwrap();
        let held = psalm(
            "Lord, remember · Dávid, – – * and · áll his tróu-ble.",
            tone,
            &PsalmOptions::default(),
        );
        assert!(held.diagnostics.is_empty(), "{:?}", held.diagnostics);
        let plain = psalm("Lord, remember · Dávid, * and · áll his tróu-ble.", tone, &PsalmOptions::default());
        assert_ne!(held.gabc, plain.gabc);
        assert!(held.gabc.contains("vid,(jij)") || held.gabc.contains("vid,(j)"), "{}", held.gabc);
        // A hand-built tone ending on its accent still gives every syllable a note.
        let t = Tone::parse("name: x\nmediant: jr 'k\ntermination: jr 'k 'j").unwrap();
        let s = psalm("The Lord is · Kíng and · glad * and · práise him", &t, &PsalmOptions::default());
        assert!(!s.gabc.split("()").any(|x| x.ends_with(char::is_alphabetic)), "{}", s.gabc);
        let eng = neuma::parse(&s.gabc).score.engrave(&ApproxMeasure, &StyleOptions::default());
        let map = eng.layout(800.0).timeline();
        assert_eq!(map.notes.len(), s.notes.len());
        assert!(
            psalm("", tone, &PsalmOptions::default())
                .diagnostics
                .iter()
                .any(|d| d.code == "apply::empty")
        );
    }

    #[test]
    fn mismatched_pointing_is_reported() {
        let codes = |tone: &str, t: &str| set(tone, t).diagnostics.iter().map(|d| d.code).collect::<Vec<_>>();
        // 1.D's mediant has two accents.
        assert!(codes("1.D", "a b · cé d * e · fé g").contains(&"apply::missing-accent"));
        let manual = PsalmOptions::default().with_auto_point(false);
        let plain = psalm("a b c d * e f g h", Tone::named("8.G").unwrap(), &manual);
        assert!(plain.diagnostics.iter().any(|d| d.code == "apply::no-accent"));
        // Pointed automatically, with no complaint about missing marks.
        assert!(!codes("8.G", "a b c d * e f g h").iter().any(|c| c.starts_with("apply::")));
        assert!(codes("8.G", "a b · c dé * e · fé g").contains(&"apply::few-preparatory"));
        assert!(codes("8.G", "a b c dé * · e f g hé i").contains(&"apply::extra-preparatory"));
    }

    #[test]
    fn spans_count_the_text() {
        let text = "1 O praise the Lord, all ye · héathen * praise him, · all ye nátions.\n\
                    2 For his merciful kindness * and the truth of the Lord endureth for ever.";
        let s = set("8.G", text);
        let chant = neuma::Chant::from_score(s.score.clone(), text, s.diagnostics.clone(), neuma::ChantOptions::default());
        assert_eq!(chant.source(), text);
        let layout = chant.layout(600.0);
        let timeline = layout.timeline();
        assert_eq!(timeline.notes.len(), s.notes.len());
        for (note, role) in timeline.notes.iter().zip(&s.notes) {
            assert_eq!(note.span, role.span);
        }
        assert_eq!(&text[s.notes[0].span.clone()], "O");
        // A click on a note finds its syllable in the text.
        let first = &timeline.notes[0];
        let hit = layout.source_at(first.cx, first.cy).unwrap();
        assert_eq!(&text[hit.span.clone()], "O");
        let map = layout.source_map();
        for e in map.syllables.iter().chain(&map.bars) {
            assert!(text.get(e.span.clone()).is_some(), "{e:?}");
        }
        assert!(map.bars.iter().all(|b| b.span.is_empty() && b.span.start > 0));
    }
}
