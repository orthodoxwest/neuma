//! Psalm tones: each half-verse's formula of reciting note, preparatory notes, accents and
//! final notes.

use std::fmt;
use std::sync::OnceLock;

use neuma::score::ClefKind;

/// One slot of a cadence formula, as a GABC neume (`"g"`, `"gh"`, `"ixi"`, `"gvFED"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    /// One unaccented syllable.
    Fixed(String),
    /// Any number of unaccented syllables, each on this note; none is fine.
    Open(String),
    /// An accented syllable.
    Accent(String),
}

/// The formula for one half-verse (or the flex).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cadence {
    /// Neumes for the first syllables before the tenor: the intonation, for a first half.
    pub lead: Vec<String>,
    /// The reciting note.
    pub tenor: String,
    /// Everything after the tenor: preparatory, accent, open and final slots.
    pub slots: Vec<Slot>,
}

impl Cadence {
    pub fn accents(&self) -> usize {
        self.slots.iter().filter(|s| matches!(s, Slot::Accent(_))).count()
    }

    /// Preparatory syllables: the fixed slots before the first accent.
    pub fn preparatory(&self) -> usize {
        self.slots
            .iter()
            .take_while(|s| !matches!(s, Slot::Accent(_)))
            .filter(|s| matches!(s, Slot::Fixed(_)))
            .count()
    }

    /// Parses a formula such as `g h jr 'k jr j.`.
    pub fn parse(formula: &str) -> Result<Cadence, ToneError> {
        let mut lead = Vec::new();
        let mut tenor = None;
        let mut slots = Vec::new();
        let words: Vec<&str> = formula.split_whitespace().collect();
        for (k, w) in words.iter().enumerate() {
            // A final `.` only ends the formula.
            let w = if k + 1 == words.len() {
                w.strip_suffix('.').unwrap_or(w)
            } else {
                w
            };
            let (accent, w) = match w.strip_prefix('\'') {
                Some(rest) => (true, rest),
                None => (false, w),
            };
            // A neume starts with a pitch (`a`–`m`, capitals for inclinata) and holds only
            // GABC note characters.
            let note_chars = |c: char| c.is_ascii_graphic() && !"()[]{};:".contains(c);
            if !w.starts_with(|c: char| matches!(c.to_ascii_lowercase(), 'a'..='m')) || !w.chars().all(note_chars) {
                return Err(ToneError(format!("`{formula}`: `{w}` is not a neume")));
            }
            let open = w.len() >= 2 && w.ends_with('r') && !accent;
            let neume = if open { &w[..w.len() - 1] } else { w };
            if tenor.is_none() {
                if open {
                    tenor = Some(neume.to_string());
                } else if accent {
                    return Err(ToneError(format!("`{formula}`: an accent before the reciting note")));
                } else {
                    lead.push(neume.to_string());
                }
                continue;
            }
            slots.push(if accent {
                Slot::Accent(neume.to_string())
            } else if open {
                Slot::Open(neume.to_string())
            } else {
                Slot::Fixed(neume.to_string())
            });
        }
        let tenor = tenor.ok_or_else(|| ToneError(format!("`{formula}`: no reciting note (a neume ending `r`)")))?;
        Ok(Cadence { lead, tenor, slots })
    }
}

impl fmt::Display for Cadence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = self.lead.clone();
        parts.push(format!("{}r", self.tenor));
        for s in &self.slots {
            parts.push(match s {
                Slot::Fixed(n) => n.clone(),
                Slot::Open(n) => format!("{n}r"),
                Slot::Accent(n) => format!("'{n}"),
            });
        }
        f.write_str(&parts.join(" "))
    }
}

/// A psalm tone with one ending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tone {
    /// Such as `8.G` or `1.D2`.
    pub name: String,
    pub clef: ClefKind,
    pub clef_line: u8,
    /// The first half-verse: intonation (its `lead`), tenor and mediant cadence.
    pub mediant: Cadence,
    /// The second half-verse.
    pub termination: Cadence,
    /// The flex. Without a `flex:` line the voice drops a step from the tenor (a minor third
    /// from do or fa) on the syllables after the last accent.
    pub flex: Cadence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToneError(pub String);

impl fmt::Display for ToneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ToneError {}

impl Tone {
    /// Parses one tone block: `name:`, `clef:`, `mediant:`, `termination:` and optionally
    /// `flex:` lines (see the built-in table for the formula syntax).
    pub fn parse(block: &str) -> Result<Tone, ToneError> {
        let mut name = None;
        let mut clef = None;
        let mut mediant = None;
        let mut termination = None;
        let mut flex = None;
        let mut seen = std::collections::HashSet::new();
        for line in block.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                return Err(ToneError(format!("`{line}`: expected `key: value`")));
            };
            let value = value.trim();
            let key = key.trim();
            if !seen.insert(key.to_string()) {
                return Err(ToneError(format!("`{key}:` is given twice")));
            }
            match key {
                "name" => name = Some(value.to_string()),
                "clef" => clef = Some(parse_clef(value)?),
                "mediant" => mediant = Some(Cadence::parse(value)?),
                "termination" => termination = Some(Cadence::parse(value)?),
                "flex" => flex = Some(Cadence::parse(value)?),
                other => return Err(ToneError(format!("unknown key `{other}`"))),
            }
        }
        let name = name.ok_or_else(|| ToneError("a tone needs a `name:`".into()))?;
        let (kind, line) = clef.unwrap_or((ClefKind::Do, 4));
        let mediant = mediant.ok_or_else(|| ToneError(format!("{name}: no `mediant:`")))?;
        let termination = termination.ok_or_else(|| ToneError(format!("{name}: no `termination:`")))?;
        let flex = match flex {
            Some(f) => f,
            None => default_flex(&mediant.tenor, kind, line)
                .ok_or_else(|| ToneError(format!("{name}: the tenor is too low for the usual flex; give a `flex:`")))?,
        };
        if mediant.accents() == 0 || termination.accents() == 0 || flex.accents() == 0 {
            return Err(ToneError(format!("{name}: each half and the flex need at least one accent `'`")));
        }
        Ok(Tone {
            name,
            clef: kind,
            clef_line: line,
            mediant,
            termination,
            flex,
        })
    }

    /// Parses several tone blocks separated by blank lines.
    pub fn parse_all(src: &str) -> Result<Vec<Tone>, ToneError> {
        let mut out = Vec::new();
        let mut block = String::new();
        for line in src.lines().chain(std::iter::once("")) {
            if line.trim().is_empty() {
                if block.lines().any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')) {
                    out.push(Tone::parse(&block)?);
                }
                block.clear();
            } else {
                block.push_str(line);
                block.push('\n');
            }
        }
        Ok(out)
    }

    /// The Solesmes tones and endings (`1.D` … `8.c`, `per`).
    pub fn builtin() -> &'static [Tone] {
        static TONES: OnceLock<Vec<Tone>> = OnceLock::new();
        TONES.get_or_init(|| Tone::parse_all(include_str!("../tones/solesmes.tones")).unwrap_or_default())
    }

    /// A built-in tone by name, ignoring case and a leading `T`/`tone`: `8.G`, `viii.G`
    /// (roman numerals work too) or `per`.
    pub fn named(name: &str) -> Option<&'static Tone> {
        let want = normalize(name);
        Tone::builtin().iter().find(|t| normalize(&t.name) == want)
    }

    /// A tone from `tones` by name, matched as [`Tone::named`] matches.
    pub fn find<'a>(tones: &'a [Tone], name: &str) -> Option<&'a Tone> {
        let want = normalize(name);
        tones.iter().find(|t| normalize(&t.name) == want)
    }

    /// The clef as GABC (`c4`).
    pub fn clef_gabc(&self) -> String {
        let k = match self.clef {
            ClefKind::Do => 'c',
            ClefKind::Fa => 'f',
        };
        format!("{k}{}", self.clef_line)
    }
}

fn parse_clef(s: &str) -> Result<(ClefKind, u8), ToneError> {
    let mut cs = s.chars();
    let kind = match cs.next() {
        Some('c') => ClefKind::Do,
        Some('f') => ClefKind::Fa,
        _ => return Err(ToneError(format!("`{s}` is not a clef"))),
    };
    match cs.as_str().parse::<u8>() {
        Ok(n @ 1..=4) => Ok((kind, n)),
        _ => Err(ToneError(format!("`{s}` is not a clef"))),
    }
}

/// The scale degree of a pitch letter under a clef: 0 for do, 3 for fa.
fn degree(letter: char, kind: ClefKind, line: u8) -> i32 {
    let pos = letter.to_ascii_lowercase() as i32 - 'a' as i32;
    // Line n of the staff is the letter `d` + 2(n − 1).
    let clef_pos = 3 + 2 * (line as i32 - 1);
    let clef_degree = match kind {
        ClefKind::Do => 0,
        ClefKind::Fa => 3,
    };
    (pos - clef_pos + clef_degree).rem_euclid(7)
}

/// A flex a step below the tenor, or a minor third below do and fa.
fn default_flex(tenor: &str, kind: ClefKind, line: u8) -> Option<Cadence> {
    let t = tenor.chars().rev().find(|c| matches!(c, 'a'..='m'))?;
    let drop = if matches!(degree(t, kind, line), 0 | 3) { 2 } else { 1 };
    let f = (t as u8).checked_sub(drop).filter(|&p| p >= b'a')?;
    let f = (f as char).to_string();
    Some(Cadence {
        lead: Vec::new(),
        tenor: tenor.to_string(),
        slots: vec![Slot::Accent(tenor.to_string()), Slot::Open(f.clone()), Slot::Fixed(f)],
    })
}

/// `VIII.g`, `viii g`, `8g`, `Tone 8.G` → `8.g`.
fn normalize(name: &str) -> String {
    let s = name.trim().to_lowercase();
    let s = s.strip_prefix("tone").unwrap_or(&s).trim().to_string();
    let roman = [
        ("viii", "8"),
        ("vii", "7"),
        ("vi", "6"),
        ("iv", "4"),
        ("v", "5"),
        ("iii", "3"),
        ("ii", "2"),
        ("i", "1"),
    ];
    let mut s = s;
    if !s.starts_with("per") {
        for (r, d) in roman {
            if let Some(rest) = s.strip_prefix(r)
                && !rest.starts_with(|c: char| c.is_ascii_alphabetic() && "iv".contains(c))
            {
                s = format!("{d}{rest}");
                break;
            }
        }
    }
    let s = s.replace(' ', ".");
    // `8g` → `8.g`.
    match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, c)) if i > 0 && c != '.' => format!("{}.{}", &s[..i], &s[i..]),
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_tones_parse() {
        let tones = Tone::builtin();
        assert_eq!(tones.len(), 33);
        let t = Tone::named("VIII.G").unwrap();
        assert_eq!(t.name, "8.G");
        assert_eq!(t.mediant.lead, ["g", "h"]);
        assert_eq!(t.mediant.tenor, "j");
        assert_eq!(t.mediant.to_string(), "g h jr 'k jr j");
        assert_eq!(t.termination.preparatory(), 2);
        assert_eq!(t.termination.accents(), 1);
        assert_eq!(Tone::named("1.D").unwrap().mediant.accents(), 2);
        assert_eq!(Tone::named("8g").unwrap().name, "8.G");
        assert_eq!(Tone::named("tone 4 E").unwrap().name, "4.E");
        assert_eq!(Tone::named("per").unwrap().termination.tenor, "g");
        assert!(Tone::named("9.a").is_none());
    }

    #[test]
    fn default_flex_drops_a_step_or_a_third() {
        // Tone 8 recites on do (j in c4): the flex drops a minor third to la.
        assert_eq!(Tone::named("8.G").unwrap().flex.slots[2], Slot::Fixed("h".into()));
        // Tone 1 recites on la (h in c4): a step to sol.
        assert_eq!(Tone::named("1.D").unwrap().flex.slots[2], Slot::Fixed("g".into()));
        // Tone 2 recites on fa (h in f3): a third.
        assert_eq!(Tone::named("2.D").unwrap().flex.slots[2], Slot::Fixed("f".into()));
        // Tone 5 recites on do (h in c3): a third.
        assert_eq!(Tone::named("5.a").unwrap().flex.slots[2], Slot::Fixed("f".into()));
    }

    #[test]
    fn rejects_bad_tones() {
        assert!(Tone::parse("name: x\nmediant: g h 'k\ntermination: jr 'k j").is_err());
        assert!(Tone::parse("name: x\nmediant: jr k\ntermination: jr 'k j").is_err());
        assert!(Tone::parse("name: x\nclef: c5\nmediant: jr 'k\ntermination: jr 'k j").is_err());
        assert!(Tone::parse("mediant: jr 'k\ntermination: jr 'k j").is_err());
        // Not neumes, a key twice, a flex with no accent, and a tenor too low for a flex.
        assert!(Tone::parse("name: x\nmediant: a)x(r 'k j\ntermination: jr 'k j").is_err());
        assert!(Tone::parse("name: x\nmediant: hér 'k j\ntermination: jr 'k j").is_err());
        assert!(Tone::parse("name: x\nmediant: jr 'z j\ntermination: jr 'k j").is_err());
        assert!(Tone::parse("name: x\nname: y\nmediant: jr 'k j\ntermination: jr 'k j").is_err());
        assert!(Tone::parse("name: x\nmediant: jr 'k j\ntermination: jr 'k j\nflex: jr j").is_err());
        assert!(Tone::parse("name: x\nmediant: ar 'b a\ntermination: ar 'b a").is_err());
        assert!(Tone::find(Tone::builtin(), "viii g").is_some_and(|t| t.name == "8.G"));
        let t = Tone::parse("name: mine\nclef: c4\nmediant: f g hr 'g hr h\ntermination: hr g f 'g hr h\nflex: hr 'h hr h").unwrap();
        assert_eq!(
            t.flex.slots,
            [Slot::Accent("h".into()), Slot::Open("h".into()), Slot::Fixed("h".into())]
        );
    }
}
