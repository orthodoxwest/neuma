//! Vowel centering rules, in the vowel-file format GregorioRef documents ("Vowel file"):
//! `alias`, `language`, `vowel`, `prefix`, `suffix` and `secondary` statements, each ending in
//! `;`, with `#` comments.

use std::ops::Range;

/// The rules for one language.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VowelRules {
    pub language: String,
    vowels: Vec<char>,
    prefixes: Vec<Vec<char>>,
    suffixes: Vec<Vec<char>>,
    secondary: Vec<Vec<char>>,
    /// The vowels again, for looking up: the ASCII ones as bits, the others sorted.
    ascii: u128,
    wide: Vec<char>,
}

const LATIN: &str = include_str!("../data/vowels-la.txt");
const ENGLISH: &str = include_str!("../data/vowels-en.txt");

impl VowelRules {
    /// The shipped rules for `language` (a code or alias, as in the `language:` header), or
    /// `None` if neuma has none. Callers fall back to Latin, as Gregorio does.
    pub fn builtin(language: &str) -> Option<VowelRules> {
        // Read once per language: every score without its own rules engraves with these.
        static READ: std::sync::Mutex<Vec<(String, Option<VowelRules>)>> = std::sync::Mutex::new(Vec::new());
        let mut read = READ.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, rules)) = read.iter().find(|(l, _)| l == language) {
            return rules.clone();
        }
        let rules = [LATIN, ENGLISH].iter().find_map(|f| VowelRules::parse(f, language));
        if read.len() < 16 {
            read.push((language.to_string(), rules.clone()));
        }
        rules
    }

    pub fn latin() -> VowelRules {
        VowelRules::builtin("la").unwrap_or_default()
    }

    fn is_vowel(&self, c: char) -> bool {
        match u32::from(c) {
            k @ 0..128 => self.ascii >> k & 1 == 1,
            _ => self.wide.binary_search(&c).is_ok(),
        }
    }

    /// Reads the rules for `language` from a vowel file, following aliases within the file.
    pub fn parse(file: &str, language: &str) -> Option<VowelRules> {
        let text: String = file
            .lines()
            .map(|l| l.split('#').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let statements: Vec<String> = text.split(';').map(|s| s.trim().to_string()).collect();
        parse_statements(&statements, language)
    }

    /// The characters of `chars` that form the syllable's nucleus: the first run of vowels
    /// (after a prefix, plus a suffix), else the first secondary sequence, else `None`.
    pub fn nucleus(&self, chars: &[char]) -> Option<Range<usize>> {
        let is_vowel = |c: char| self.is_vowel(c);
        let matches = |at: usize, pat: &[char]| chars.len() >= at + pat.len() && chars[at..at + pat.len()] == *pat;
        let mut i = 0;
        while i < chars.len() {
            if let Some(p) = self
                .prefixes
                .iter()
                .find(|p| matches(i, p) && chars.get(i + p.len()).is_some_and(|c| is_vowel(*c)))
            {
                // A prefix is consonantal only when a vowel follows it.
                let start = i + p.len();
                return Some(self.extend(chars, start));
            }
            if is_vowel(chars[i]) {
                return Some(self.extend(chars, i));
            }
            i += 1;
        }
        for i in 0..chars.len() {
            if let Some(s) = self.secondary.iter().find(|s| matches(i, s)) {
                return Some(i..i + s.len());
            }
        }
        None
    }

    fn extend(&self, chars: &[char], start: usize) -> Range<usize> {
        let mut end = start;
        while end < chars.len() && (self.is_vowel(chars[end]) || is_mark(chars[end])) {
            end += 1;
        }
        if let Some(s) = self
            .suffixes
            .iter()
            .filter(|s| chars.len() >= end + s.len() && chars[end..end + s.len()] == ***s)
            .max_by_key(|s| s.len())
        {
            end += s.len();
        }
        start..end
    }
}

/// Combining marks belong to the character before them.
fn is_mark(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}')
}

fn parse_statements(statements: &[String], language: &str) -> Option<VowelRules> {
    // Resolve aliases first, then find the language section.
    let mut want = language.to_string();
    for _ in 0..8 {
        let mut next = None;
        for s in statements {
            if let Some(rest) = s.strip_prefix("alias") {
                let names: Vec<&str> = rest
                    .split(['[', ']'])
                    .map(str::trim)
                    .filter(|x| !x.is_empty() && *x != "to")
                    .collect();
                if names.len() == 2 && names[0] == want {
                    next = Some(names[1].to_string());
                }
            }
        }
        match next {
            Some(n) => want = n,
            None => break,
        }
    }
    let mut rules: Option<VowelRules> = None;
    for s in statements {
        let (key, rest) = s.split_once(char::is_whitespace).unwrap_or((s.as_str(), ""));
        match key {
            "language" => {
                if rules.is_some() {
                    break;
                }
                let name = rest.trim().trim_start_matches('[').trim_end_matches(']');
                if name == want {
                    rules = Some(VowelRules {
                        language: want.clone(),
                        ..VowelRules::default()
                    });
                }
            }
            "vowel" => {
                if let Some(r) = rules.as_mut() {
                    r.vowels.extend(rest.chars().filter(|c| !c.is_whitespace()));
                }
            }
            "prefix" | "suffix" | "secondary" => {
                if let Some(r) = rules.as_mut() {
                    let list = rest.split_whitespace().map(|w| w.chars().collect::<Vec<char>>());
                    match key {
                        "prefix" => r.prefixes.extend(list),
                        "suffix" => r.suffixes.extend(list),
                        _ => r.secondary.extend(list),
                    }
                }
            }
            _ => {}
        }
    }
    rules.map(|mut r| {
        for &c in &r.vowels {
            match u32::from(c) {
                k @ 0..128 => r.ascii |= 1 << k,
                _ => r.wide.push(c),
            }
        }
        r.wide.sort_unstable();
        r.wide.dedup();
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nucleus(rules: &VowelRules, s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        rules.nucleus(&chars).map(|r| chars[r].iter().collect()).unwrap_or_default()
    }

    #[test]
    fn latin() {
        let la = VowelRules::builtin("latin").unwrap();
        assert_eq!(nucleus(&la, "Do"), "o");
        assert_eq!(nucleus(&la, "quae"), "ae");
        assert_eq!(nucleus(&la, "iam"), "a");
        assert_eq!(nucleus(&la, "Chri"), "i");
        assert_eq!(nucleus(&la, "laus"), "au");
        assert_eq!(nucleus(&la, "u"), "u");
        assert_eq!(nucleus(&la, "fí"), "í");
        assert_eq!(nucleus(&la, "œ\u{301}"), "œ\u{301}");
        assert_eq!(nucleus(&la, "*"), "");
    }

    #[test]
    fn english() {
        let en = VowelRules::builtin("English").unwrap();
        assert_eq!(nucleus(&en, "yes"), "e");
        assert_eq!(nucleus(&en, "queen"), "ee");
        assert_eq!(nucleus(&en, "new"), "ew");
        assert_eq!(nucleus(&en, "my"), "y");
        assert_eq!(nucleus(&en, "rhythm"), "y");
        assert_eq!(nucleus(&en, "hmm"), "");
        assert!(VowelRules::builtin("klingon").is_none());
    }
}
