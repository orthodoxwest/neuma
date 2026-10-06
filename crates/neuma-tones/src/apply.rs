//! Sets pointed text to a psalm tone.

use std::ops::Range;

use neuma::score::{Bar, BarKind, Clef, Figure, Lyric};
use neuma::{Diagnostic, Score, ScoreBuilder, Severity};

use crate::pointed::{self, Part, PartKind, Pointed, Syllable};
use crate::syllable::fold;
use crate::tone::{Cadence, Slot, Tone};

/// When the intonation is sung.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Intone {
    /// On the first verse only, as at the Office.
    #[default]
    FirstVerse,
    /// On every verse, as in the Magnificat and Benedictus.
    EveryVerse,
    /// Never: every verse starts on the tenor.
    Never,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub intone: Intone,
    /// Remove the acute accents from the printed text (they still place the accents).
    pub strip_accents: bool,
    /// The score's `name:` header.
    pub name: Option<String>,
}

/// What a note does in the tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
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
pub struct NoteRole {
    /// Index into the pointed text's verses.
    pub verse: usize,
    /// The printed verse number.
    pub number: Option<u32>,
    pub part: PartKind,
    pub role: Role,
    /// The sung syllable's text in the pointed source.
    pub source: Range<usize>,
}

/// A pointed text set to a tone.
#[derive(Clone, Debug)]
pub struct Setting {
    pub score: Score,
    /// The score as GABC.
    pub gabc: String,
    /// Every note's place in the tone: `notes[i]` is note `i` of the engraved score.
    pub notes: Vec<NoteRole>,
    /// Problems in the pointing, with spans in the pointed text.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses `pointed` text and sets it to `tone`.
pub fn apply_text(tone: &Tone, pointed: &str, options: &Options) -> Setting {
    let parsed = pointed::parse(pointed);
    let mut setting = apply(tone, &parsed, options);
    let mut diags = parsed.diagnostics;
    diags.append(&mut setting.diagnostics);
    setting.diagnostics = diags;
    setting
}

/// Sets a parsed pointed text to `tone`. The text is split into sung syllables first.
pub fn apply(tone: &Tone, pointed: &Pointed, options: &Options) -> Setting {
    let text = pointed.syllabified();
    let mut b = ScoreBuilder::new();
    if let Some(name) = &options.name {
        b = b.header("name", name);
    }
    b = b.clef(tone.clef, tone.clef_line);
    let mut notes = Vec::new();
    let mut diags = Vec::new();
    if text.verses.is_empty() {
        warn(&mut diags, Severity::Warning, 0..0, "apply::empty", "there is no verse to sing");
    }
    let mut figures = Figures::default();
    let last_verse = text.verses.len().saturating_sub(1);
    for (vi, verse) in text.verses.iter().enumerate() {
        for (pi, part) in verse.parts.iter().enumerate() {
            let cadence = match part.kind {
                PartKind::Flex => &tone.flex,
                PartKind::Mediant => &tone.mediant,
                PartKind::Termination => &tone.termination,
            };
            // The intonation opens the verse, whichever part comes first.
            let intone = pi == 0
                && match options.intone {
                    Intone::FirstVerse => vi == 0,
                    Intone::EveryVerse => true,
                    Intone::Never => false,
                };
            let lead: &[String] = if part.kind == PartKind::Termination {
                &cadence.lead
            } else if intone {
                &tone.mediant.lead
            } else {
                &[]
            };
            let sung = set_part(part, cadence, lead, &mut diags);
            for (s, neumes) in part.syllables.iter().zip(sung) {
                let mut fig = Vec::new();
                for (neume, role) in &neumes {
                    let f = figures.get(neume);
                    let count = f.iter().filter(|f| matches!(f, Figure::Note(_))).count();
                    for _ in 0..count {
                        notes.push(NoteRole {
                            verse: vi,
                            number: verse.number,
                            part: part.kind,
                            role: *role,
                            source: s.span.clone(),
                        });
                    }
                    fig.extend(f);
                }
                let shown = if options.strip_accents {
                    strip_acutes(&s.text)
                } else {
                    s.text.clone()
                };
                b = b.syllable(Lyric::from_plain(&shown), s.starts_word(), fig);
            }
            let (mark, kind) = match part.kind {
                PartKind::Flex => ("†", BarKind::Minima),
                PartKind::Mediant => ("*", BarKind::Maior),
                PartKind::Termination => ("", if vi == last_verse { BarKind::Finalis } else { BarKind::Maior }),
            };
            let bar = vec![Figure::Bar(Bar {
                kind,
                high: false,
                span: 0..0,
            })];
            b = b.syllable(Lyric::from_plain(mark), true, bar);
        }
    }
    let score = b.build();
    Setting {
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
            .map(|s| s.notation.into_iter().filter(|f| !matches!(f, Figure::Clef(Clef { .. }))).collect())
            .unwrap_or_default();
        self.0.push((neume.to_string(), f.clone()));
        f
    }
}

fn strip_acutes(text: &str) -> String {
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

type Sung = Vec<Vec<(String, Role)>>;

fn warn(diags: &mut Vec<Diagnostic>, severity: Severity, span: Range<usize>, code: &'static str, message: &str) {
    diags.push(Diagnostic {
        severity,
        span,
        code,
        message: message.to_string(),
    });
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
        let held: Vec<(String, Role)> = sung.drain(n..).flatten().collect();
        sung[n - 1].extend(held);
        sung
    } else {
        set_cadence(part, cadence, lead, diags)
    };
    // Every syllable sings something: one left without a note takes the note before it.
    for i in 0..out.len() {
        if out[i].is_empty() {
            let prev = if i > 0 { out[i - 1].last().cloned() } else { None };
            out[i].push(prev.unwrap_or_else(|| (cadence.tenor.clone(), Role::Tenor)));
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
        return vec![vec![(cadence.tenor.clone(), Role::Tenor)]; n];
    }

    // Where the cadence starts and which syllables carry its accents.
    let start = syls.iter().rposition(|s| s.cadence);
    let marked: Vec<usize> = (start.unwrap_or(0)..n).filter(|&i| syls[i].accent).collect();
    let (start, accents) = if marked.is_empty() {
        // No accent marked: the flex drops on its last syllable, and other halves guess.
        if part.kind != PartKind::Flex || start.is_some() {
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
        if start.is_none() && part.kind != PartKind::Flex {
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
            Some(neume) => slot.push((neume.clone(), Role::Intonation)),
            None => slot.push((cadence.tenor.clone(), Role::Tenor)),
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
            slot.push((cadence.tenor.clone(), Role::Tenor));
        }
        for (j, f) in fixed.iter().enumerate() {
            out[start + extra + j].push(((*f).clone(), Role::Preparatory));
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
            out[start + j].push(((*f).clone(), Role::Preparatory));
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
        fill(&mut out[start..first], prep, Role::Preparatory);
    }

    // The accents and what follows each.
    for (u, (&at, (accent, after))) in accents.iter().zip(units).enumerate() {
        out[at].push((accent.to_string(), Role::Accent));
        let next = accents.get(u + 1).copied().unwrap_or(n);
        let last = u + 1 == accents.len();
        let gap = next - at - 1;
        if gap == 0 {
            // Nothing between: the notes are sung on the accent. Between two accents that
            // is the held note the pointing marks with a dash; at the end, the open note
            // is dropped.
            for s in *after {
                match s {
                    Slot::Open(o) if !last => out[at].push((o.clone(), Role::Ending)),
                    Slot::Fixed(f) => out[at].push((f.clone(), Role::Ending)),
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
            fill(&mut out[at + 1..next], after, Role::Ending);
        }
    }
    out
}

/// Gives `syls` syllables the slots in order: one each for a fixed slot, and any extra
/// syllables to the open slot. With too few syllables, the leftover fixed notes are sung on the
/// last syllable; with too many and no open slot, the extras repeat the last note.
fn fill(syls: &mut [Vec<(String, Role)>], slots: &[Slot], role: Role) {
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

    fn set(tone: &str, text: &str) -> Setting {
        apply_text(Tone::named(tone).unwrap(), text, &Options::default())
    }

    /// The notes on each syllable, as `text(notes)`.
    fn sung(s: &Setting) -> String {
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
        let map = eng.layout(800.0, &Default::default()).notes(&Default::default());
        assert_eq!(map.notes.len(), s.notes.len());
        // Every note's role points at its syllable in the pointed text.
        for (n, r) in map.notes.iter().zip(&s.notes) {
            let src = text[r.source.clone()].replace('-', "");
            assert!(src.contains(n.syllable_text.as_str()), "{src} vs {}", n.syllable_text);
        }
        // The second verse isn't intoned.
        let v2: Vec<_> = s.notes.iter().filter(|r| r.verse == 1).collect();
        assert!(v2.iter().all(|r| r.role != Role::Intonation));
        assert_eq!(v2[0].number, Some(5));
        // The verse ends on a full bar and the psalm on a double bar.
        assert!(s.gabc.contains("(:)") && s.gabc.trim_end().ends_with("(::)"), "{}", s.gabc);
    }

    #[test]
    fn trailing_dashes_and_odd_tones() {
        // The dashes after "Dá-vid," hold its last syllable for the second accent and the end.
        let tone = Tone::named("7.a").unwrap();
        let held = apply_text(tone, "Lord, remember · Dávid, – – * and · áll his tróu-ble.", &Options::default());
        assert!(held.diagnostics.is_empty(), "{:?}", held.diagnostics);
        let plain = apply_text(tone, "Lord, remember · Dávid, * and · áll his tróu-ble.", &Options::default());
        assert_ne!(held.gabc, plain.gabc);
        assert!(held.gabc.contains("vid,(jij)") || held.gabc.contains("vid,(j)"), "{}", held.gabc);
        // A hand-built tone ending on its accent still gives every syllable a note.
        let t = Tone::parse("name: x\nmediant: jr 'k\ntermination: jr 'k 'j").unwrap();
        let s = apply_text(&t, "The Lord is · Kíng and · glad * and · práise him", &Options::default());
        assert!(!s.gabc.split("()").any(|x| x.ends_with(char::is_alphabetic)), "{}", s.gabc);
        let eng = neuma::parse(&s.gabc).score.engrave(&ApproxMeasure, &StyleOptions::default());
        let map = eng.layout(800.0, &Default::default()).notes(&Default::default());
        assert_eq!(map.notes.len(), s.notes.len());
        assert!(
            apply_text(tone, "", &Options::default())
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
        assert!(codes("8.G", "a b c d * e f g h").contains(&"apply::no-accent"));
        assert!(codes("8.G", "a b · c dé * e · fé g").contains(&"apply::few-preparatory"));
        assert!(codes("8.G", "a b c dé * · e f g hé i").contains(&"apply::extra-preparatory"));
    }
}
