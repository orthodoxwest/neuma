//! A score's library entry: its descriptive headers, typed, and what can be read off its
//! notes without laying it out, so a library can list and search many scores cheaply.

use crate::engrave::{Engraving, Initial, StyleOptions, strip_tex};
use crate::notes::{PauseKind, Weights, pitch, timed_pauses};
use crate::score::BarKind;
use crate::score::Header;
use crate::text::ApproxMeasure;

/// What a library shows and searches by. Header fields are as written, with TeX markup
/// removed; a field the source leaves out or empty is `None`.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Summary {
    /// The `name` header.
    pub name: Option<String>,
    /// The `office-part` header as written, and what it names.
    pub office_part: Option<String>,
    /// What `office_part` names, if it names a kind neuma knows.
    pub kind: Option<OfficePart>,
    /// The mode headers.
    pub mode: Option<Mode>,
    /// A short label for when the piece is sung; a calendar holds the full list of uses.
    pub occasion: Option<String>,
    /// The `book` header: the book the score is taken from.
    pub book: Option<String>,
    /// The `language` header, as written (`la`, `English`).
    pub language: Option<String>,
    /// The `transcriber` header.
    pub transcriber: Option<String>,
    /// The `gabc-copyright` header: the transcription's copyright.
    pub gabc_copyright: Option<String>,
    /// The `score-copyright` header: the copyright of the score transcribed.
    pub score_copyright: Option<String>,
    /// The `commentary` header: the source of the text, such as a psalm verse.
    pub commentary: Option<String>,
    /// The `annotation` headers, in order (at most two are drawn).
    pub annotations: Vec<String>,
    /// Every other header in source order, so a library can keep its own fields (`source`,
    /// `translation-of`) in the score file. These are raw: values are only trimmed, so TeX
    /// markup and empty values are kept.
    pub other_headers: Vec<(String, String)>,
    /// The opening words: up to the first bar (other than a virgula) at or after the end of
    /// the second word, at most eight. An opening word in capitals (for the initial) is
    /// written in lower case after its first letter, here and in `text`.
    pub incipit: String,
    /// All the sung text, words separated by spaces, for full-text search. Psalm marks and
    /// other signs set as text are left out.
    pub text: String,
    /// The lowest note, in semitones above the clef's do.
    pub lowest: Option<i32>,
    /// The highest note, in semitones above the clef's do.
    pub highest: Option<i32>,
    /// The last note, in semitones above the clef's do.
    pub final_pitch: Option<i32>,
    /// How many notes it has.
    pub notes: u32,
    /// How many syllables carry text.
    pub syllables: u32,
    /// How many words carry text.
    pub words: u32,
    /// The length with the default weights, in pulses (one per plain note).
    pub duration: f32,
}

/// The kind of chant an `office-part` header names, in Latin or English, spelled out or
/// abbreviated (`Antiphona`, `Ant.`, `Introit`, `Resp. breve`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OfficePart {
    /// An antiphon (`Antiphona`, `Ant.`).
    Antiphon,
    /// The introit (`Introitus`, `In.`).
    Introit,
    /// The gradual (`Graduale`, `Gr.`).
    Gradual,
    /// The alleluia (`Alleluia`, `All.`).
    Alleluia,
    /// The tract (`Tractus`, `Tr.`).
    Tract,
    /// The sequence (`Sequentia`, `Seq.`).
    Sequence,
    /// The offertory (`Offertorium`, `Of.`).
    Offertory,
    /// The communion (`Communio`, `Co.`).
    Communion,
    /// A hymn (`Hymnus`, `Hy.`).
    Hymn,
    /// A responsory (`Responsorium`, `R.`).
    Responsory,
    /// A short responsory (`Responsorium breve`, `R. br.`).
    ShortResponsory,
    /// A versicle (`Versiculus`, `V.`).
    Versicle,
    /// The chapter (`Capitulum`).
    Chapter,
    /// The collect (`Oratio`).
    Collect,
    /// A psalm (`Psalmus`, `Ps.`).
    Psalm,
    /// A canticle (`Canticum`).
    Canticle,
    /// The Kyrie of the Mass.
    Kyrie,
    /// The Gloria of the Mass (not the Gloria Patri).
    Gloria,
    /// The Credo.
    Credo,
    /// The Sanctus.
    Sanctus,
    /// The Agnus Dei.
    Agnus,
    /// Something else, such as `Varia`; the header itself is in `office_part`.
    Other,
}

impl OfficePart {
    /// What an `office-part` header names; [`OfficePart::Other`] for anything else.
    #[must_use]
    pub fn parse(value: &str) -> OfficePart {
        use OfficePart as P;
        // The response and versicle signs abbreviate the words they stand for.
        let lower = value.to_lowercase().replace('℟', "r ").replace('℣', "v ");
        let mut words = lower.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty());
        let word = words.next().unwrap_or("");
        match word {
            // The lesser doxology, not the Gloria of the Mass.
            "gloria" if words.next() == Some("patri") => P::Other,
            "responsorium" | "responsoria" | "responsory" | "resp" | "re" | "r"
                if words
                    .next()
                    .is_some_and(|w| matches!(w, "breve" | "brevia" | "brevis" | "br" | "brev")) =>
            {
                P::ShortResponsory
            }
            "short" if lower.contains("resp") => P::ShortResponsory,
            "rb" => P::ShortResponsory,
            // `V` alone is also the numeral five ("V. Ant."), so a kind spelled out or
            // abbreviated after it wins; a response ("V. R.") or a two-letter word ("V. In
            // omnem") does not. Only the next word is read, so this recurses once at most.
            "v" => match words.next() {
                Some(w) if w.len() > 2 && !matches!(w, "resp" | "responsorium" | "responsory" | "response") => match OfficePart::parse(w) {
                    P::Other => P::Versicle,
                    other => other,
                },
                _ => P::Versicle,
            },
            "versiculus" | "versiculi" | "versicle" | "versicles" => P::Versicle,
            "capitulum" | "capitula" | "chapter" | "chapters" | "cap" => P::Chapter,
            "oratio" | "orationes" | "collect" | "collects" | "collecta" | "collectae" | "or" => P::Collect,
            "antiphona" | "antiphonae" | "antiphon" | "antiphons" | "ant" | "an" => P::Antiphon,
            "introitus" | "introit" | "intr" | "in" => P::Introit,
            "graduale" | "gradual" | "grad" | "gr" => P::Gradual,
            "alleluia" | "alleluja" | "allelúia" | "all" | "al" => P::Alleluia,
            "tractus" | "tract" | "tr" => P::Tract,
            "sequentia" | "sequence" | "seq" | "sq" => P::Sequence,
            "offertorium" | "offertory" | "off" | "of" => P::Offertory,
            "communio" | "communion" | "comm" | "co" => P::Communion,
            "hymnus" | "hymni" | "hymn" | "hy" => P::Hymn,
            "responsorium" | "responsoria" | "responsory" | "resp" | "re" | "r" => P::Responsory,
            "psalmus" | "psalmi" | "psalm" | "ps" => P::Psalm,
            "canticum" | "canticle" | "cant" => P::Canticle,
            "kyrie" | "ky" => P::Kyrie,
            "gloria" | "gl" => P::Gloria,
            "credo" | "cr" => P::Credo,
            "sanctus" | "sa" => P::Sanctus,
            "agnus" | "ag" => P::Agnus,
            _ => P::Other,
        }
    }
}

/// The `mode`, `mode-modifier` and `mode-differentia` headers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Mode {
    /// 1 to 8, when the header starts with an arabic or roman number (`8`, `VIII`, `1g`).
    pub number: Option<u8>,
    /// The `mode` header as written, such as `8`, `VIII` or `per`.
    pub name: String,
    /// The `mode-modifier` header (such as `t. irr.`).
    pub modifier: Option<String>,
    /// From `mode-differentia`, else what follows the number (`g` in `1g`).
    pub differentia: Option<String>,
}

impl Mode {
    fn read(header: &Header) -> Option<Mode> {
        let name = field(header, "mode")?;
        let (number, rest) = mode_number(&name);
        let rest = rest.trim().trim_start_matches(['.', ',']).trim();
        Some(Mode {
            number,
            differentia: field(header, "mode-differentia").or_else(|| (number.is_some() && !rest.is_empty()).then(|| rest.to_string())),
            modifier: field(header, "mode-modifier"),
            name,
        })
    }
}

/// A leading 1–8 or I–VIII (upper or lower case), and the text after it. The number must end the
/// word or be followed by one differentia letter (`1g`, `VIIIG`, `Ia`), so `Irregularis` and
/// `IX` are not mode numbers.
fn mode_number(s: &str) -> (Option<u8>, &str) {
    let ends = |rest: &str| {
        let mut chars = rest.chars();
        match chars.next() {
            None => true,
            Some(c) if !c.is_alphabetic() => true,
            Some(c) => matches!(c.to_ascii_lowercase(), 'a'..='g') && chars.next().is_none_or(|d| !d.is_alphabetic()),
        }
    };
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        return match s[..digits].parse::<u8>() {
            Ok(n @ 1..=8) if ends(&s[digits..]) => (Some(n), &s[digits..]),
            _ => (None, s),
        };
    }
    const ROMAN: [&str; 8] = ["VIII", "VII", "VI", "IV", "V", "III", "II", "I"];
    const VALUE: [u8; 8] = [8, 7, 6, 4, 5, 3, 2, 1];
    for (r, v) in ROMAN.iter().zip(VALUE) {
        let head = s.get(..r.len()).unwrap_or("");
        // One case throughout: `VIII` or `viii`, not `Ii`.
        if head == *r || head == r.to_ascii_lowercase() {
            let rest = &s[r.len()..];
            if ends(rest) {
                return (Some(v), rest);
            }
        }
    }
    (None, s)
}

fn field(header: &Header, name: &str) -> Option<String> {
    header
        .get(name)
        .map(strip_tex)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Headers the summary types; the rest go to `other_headers`.
const TYPED: [&str; 13] = [
    "name",
    "office-part",
    "mode",
    "mode-modifier",
    "mode-differentia",
    "occasion",
    "book",
    "language",
    "transcriber",
    "gabc-copyright",
    "score-copyright",
    "commentary",
    "annotation",
];

/// The most words an incipit takes when the score has no early bar.
const INCIPIT_WORDS: usize = 8;

impl Engraving {
    /// The score's library entry. `header` is the parsed score's header ([`crate::Chant::summary`]
    /// keeps it for you).
    #[must_use]
    pub fn summary(&self, header: &Header) -> Summary {
        // Text, word by word. A syllable's text can hold spaces of its own (`℟ Ecce`).
        let mut word_texts: Vec<String> = Vec::new();
        let mut syllables = 0;
        let mut word_of_syllable = Vec::with_capacity(self.syllable_text.len());
        let mut last_word = None;
        for (si, plain) in self.syllable_text.iter().enumerate() {
            let word = self.syllable_word.get(si).copied().unwrap_or(0);
            // Psalm marks (`*`, `†`) and other signs set as text aren't words.
            if plain.chars().any(char::is_alphanumeric) {
                if last_word != Some(word) {
                    word_texts.push(String::new());
                    last_word = Some(word);
                }
                if let Some(w) = word_texts.last_mut() {
                    w.push_str(plain);
                }
                syllables += 1;
            }
            word_of_syllable.push(word_texts.len());
        }
        let words = word_texts.len();

        // The opening word is often in capitals for the initial (`PUER`, `CHristus`). This
        // also lowers an opening acronym (`IHS`).
        if let Some(first) = word_texts.first_mut() {
            let start = first.find(char::is_alphabetic).unwrap_or(0);
            let end = first[start..].find(char::is_whitespace).map_or(first.len(), |e| start + e);
            let token = &first[start..end];
            if token.chars().count() >= 2 && token.chars().take(2).all(char::is_uppercase) {
                let mut chars = token.chars();
                let lowered: String = chars.next().into_iter().chain(chars.flat_map(char::to_lowercase)).collect();
                first.replace_range(start..end, &lowered);
            }
        }
        let text = word_texts.join(" ");

        // The incipit runs to the first bar after its second word (an intonation's `*` or a
        // virgula is too early), within the word cap.
        let bar_words = self
            .pauses
            .iter()
            .filter(|&&(_, kind)| matches!(kind, PauseKind::Bar(b) if b != BarKind::Virgula && b != BarKind::Minimis))
            .filter_map(|&(before, _)| before.checked_sub(1))
            .filter_map(|n| self.notes.get(n as usize))
            .map(|n| word_of_syllable.get(n.syllable as usize).copied().unwrap_or(0))
            .find(|&w| w >= 2.min(words));
        let take = bar_words.unwrap_or(words).min(INCIPIT_WORDS);
        let incipit = word_texts[..take].join(" ");

        let pitches: Vec<i32> = self.notes.iter().map(|n| pitch(n).1).collect();
        let (lowest, highest) = (pitches.iter().min().copied(), pitches.iter().max().copied());
        let weights = Weights::default();
        let duration = self.notes.iter().map(|n| weights.of_note(n)).sum::<f32>()
            + timed_pauses(&self.pauses, &weights).iter().map(|p| p.duration).sum::<f32>();

        let office_part = field(header, "office-part");
        Summary {
            name: field(header, "name"),
            kind: office_part.as_deref().map(OfficePart::parse),
            office_part,
            mode: Mode::read(header),
            occasion: field(header, "occasion"),
            book: field(header, "book"),
            language: field(header, "language"),
            transcriber: field(header, "transcriber"),
            gabc_copyright: field(header, "gabc-copyright"),
            score_copyright: field(header, "score-copyright"),
            commentary: field(header, "commentary"),
            annotations: header
                .get_all("annotation")
                .map(strip_tex)
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
            other_headers: header
                .fields
                .iter()
                .filter(|(n, _)| !TYPED.iter().any(|t| n.eq_ignore_ascii_case(t)))
                .map(|(n, v)| (n.clone(), v.trim().to_string()))
                .collect(),
            incipit,
            text,
            lowest,
            highest,
            final_pitch: pitches.last().copied(),
            notes: self.notes.len() as u32,
            syllables,
            words: words as u32,
            duration,
        }
    }
}

/// Parses `gabc` and summarizes it, without measuring text or laying it out.
#[must_use]
pub fn summarize(gabc: &str) -> Summary {
    let parsed = crate::parse(gabc);
    let style = StyleOptions::default().with_initial(Initial::None).with_annotation(false);
    parsed.score.engrave(&ApproxMeasure, &style).summary(&parsed.score.header)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUER: &str = "name: Puer natus est;\noffice-part: Introitus;\nmode: 7;\noccasion: In Nativitate Domini;\n\
        annotation: Intr.;\n%%\n(c3) PU(g)er(gh) na(h)tus(hi) est(h.) (,) no(h)bis(g) (;) et(h) fí(ji)li(h)us(h) (::)";

    #[test]
    fn reads_headers_and_notes() {
        let s = summarize(PUER);
        assert_eq!(s.name.as_deref(), Some("Puer natus est"));
        assert_eq!(s.kind, Some(OfficePart::Introit));
        assert_eq!(s.mode.as_ref().and_then(|m| m.number), Some(7));
        assert_eq!(s.annotations, ["Intr."]);
        assert!(s.other_headers.is_empty());
        let other = summarize("name: x;\nsource: Vespers booklet, p. 7;\ntranslation-of: puer-natus;\nMode: 7;\n%%\n(c4) a(g)");
        assert_eq!(
            other.other_headers,
            [
                ("source".to_string(), "Vespers booklet, p. 7".to_string()),
                ("translation-of".to_string(), "puer-natus".to_string())
            ]
        );
        assert_eq!(s.text, "Puer natus est nobis et fílius");
        assert_eq!(s.incipit, "Puer natus est");
        assert_eq!((s.words, s.syllables, s.notes), (6, 11, 14));
        // Pitches, range and length agree with the timeline.
        let eng = crate::parse(PUER).score.engrave(&ApproxMeasure, &StyleOptions::default());
        let map = eng.layout(2000.0).timeline();
        let semis: Vec<i32> = map.notes.iter().map(|n| n.semitones).collect();
        assert_eq!((s.lowest, s.highest), (semis.iter().min().copied(), semis.iter().max().copied()));
        assert_eq!(s.final_pitch, semis.last().copied());
        assert_eq!(s.duration, map.duration);
    }

    #[test]
    fn modes() {
        let mode = |h: &str| summarize(&format!("{h}\n%%\n(c4) a(g)")).mode;
        let m = mode("mode: VIII;\nmode-differentia: G;").unwrap();
        assert_eq!((m.number, m.differentia.as_deref()), (Some(8), Some("G")));
        let m = mode("mode: 1g;").unwrap();
        assert_eq!((m.number, m.differentia.as_deref()), (Some(1), Some("g")));
        let m = mode("mode: per;").unwrap();
        assert_eq!((m.number, m.name.as_str()), (None, "per"));
        assert_eq!(mode("mode: iv;").unwrap().number, Some(4));
        assert_eq!(mode("mode: Irregularis;").unwrap().number, None);
        assert_eq!(mode("mode: 9;").unwrap().number, None);
        for v in ["IX", "irr", "Irr", "in", "Vir", "Ii", "1st"] {
            assert_eq!(mode(&format!("mode: {v};")).unwrap().number, None, "{v}");
        }
        let m = mode("mode: IVg;").unwrap();
        assert_eq!((m.number, m.differentia.as_deref()), (Some(4), Some("g")));
        let m = mode("mode: Ia;").unwrap();
        assert_eq!((m.number, m.differentia.as_deref()), (Some(1), Some("a")));
        let m = mode("mode: VIII.;").unwrap();
        assert_eq!((m.number, m.differentia), (Some(8), None));
        assert_eq!(mode("mode: ;"), None);
        assert_eq!(mode(""), None);
    }

    #[test]
    fn office_parts() {
        for (v, k) in [
            ("Antiphona", OfficePart::Antiphon),
            ("Ant.", OfficePart::Antiphon),
            ("Responsorium breve", OfficePart::ShortResponsory),
            ("Responsorium", OfficePart::Responsory),
            ("Short Responsory", OfficePart::ShortResponsory),
            ("Versicle", OfficePart::Versicle),
            ("V.", OfficePart::Versicle),
            ("Capitulum", OfficePart::Chapter),
            ("Chapter", OfficePart::Chapter),
            ("Oratio", OfficePart::Collect),
            ("Collect", OfficePart::Collect),
            ("hymn", OfficePart::Hymn),
            ("Communio", OfficePart::Communion),
            ("Varia", OfficePart::Other),
            ("Ant.ad Magn.", OfficePart::Antiphon),
            ("R. br.", OfficePart::ShortResponsory),
            ("Alleluja", OfficePart::Alleluia),
            ("Allelúia", OfficePart::Alleluia),
            ("Psalmi", OfficePart::Psalm),
            ("Gloria", OfficePart::Gloria),
            ("Gloria Patri", OfficePart::Other),
            ("V. Ant.", OfficePart::Antiphon),
            ("V Antiphona", OfficePart::Antiphon),
            ("℣.", OfficePart::Versicle),
            ("℟. br.", OfficePart::ShortResponsory),
            ("Re. br.", OfficePart::ShortResponsory),
            ("Short Resp.", OfficePart::ShortResponsory),
            ("Responsorium brevis", OfficePart::ShortResponsory),
            ("Versiculi", OfficePart::Versicle),
            ("Capitula", OfficePart::Chapter),
            ("Orationes", OfficePart::Collect),
            ("Collects", OfficePart::Collect),
            ("Versus", OfficePart::Other),
            ("V. R.", OfficePart::Versicle),
            ("℣. ℟.", OfficePart::Versicle),
            ("V. In omnem terram", OfficePart::Versicle),
            ("V. Resp.", OfficePart::Versicle),
        ] {
            assert_eq!(OfficePart::parse(v), k, "{v}");
        }
        // A long run of V is read in one pass.
        for v in ["v ".repeat(50_000), "V. ".repeat(50_000) + "Ant."] {
            assert_eq!(OfficePart::parse(&v), OfficePart::Versicle);
        }
    }

    #[test]
    fn empty_and_textless_scores() {
        let s = summarize("");
        assert_eq!((s.notes, s.words, s.lowest, s.highest, s.final_pitch), (0, 0, None, None, None));
        assert!(s.incipit.is_empty() && s.text.is_empty());
        let s = summarize("%%\n(c4) (g) (h) (::)");
        assert_eq!((s.notes, s.words), (2, 0));
        assert!(s.incipit.is_empty());
        // Psalm marks aren't words, and an intonation's `*` doesn't end the incipit.
        let s = summarize("%%\n(c4) CHri(g)stus(h) *() fa(g)ctus(h) est(g) (,) pro(g) (;) no(g)bis(g) (::)");
        assert_eq!(s.text, "Christus factus est pro nobis");
        assert_eq!(s.incipit, "Christus factus est");
        assert_eq!(s.words, 5);
        // A syllable's own spaces don't split its word.
        let s = summarize("%%\n(c4) <sp>R/</sp> Ec(g)ce(h) quam(g) (,) bo(h)num(g) (::)");
        assert_eq!(s.text, "℟ Ecce quam bonum");
        assert_eq!((s.words, s.incipit.as_str()), (3, "℟ Ecce quam"));
        // Capitals after a sign are lowered too.
        let s = summarize("%%\n(c4) <sp>R/</sp> EC(g)ce(h) (::)");
        assert_eq!(s.text, "℟ Ecce");
    }

    #[test]
    fn incipit_is_capped() {
        let s = summarize("%%\n(c4) a(g) b(g) c(g) d(g) e(g) f(g) g(g) h(g) i(g) j(g) (::)");
        assert_eq!(s.incipit, "a b c d e f g h");
        assert_eq!(s.words, 10);
    }
}
