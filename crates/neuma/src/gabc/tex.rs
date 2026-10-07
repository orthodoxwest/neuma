//! The text of verbatim TeX (`<v>…</v>`): enough of TeX to print what it would print in the
//! common cases (styles, colours, accents, a few symbols), without running it.

use crate::score::TextStyle;

/// Groups nested deeper than this are read as plain text, so input nested without end (a
/// fuzzer's, or a paste gone wrong) can't run the parser out of stack.
const MAX_DEPTH: usize = 32;

/// A character of the text, its style, and whether it counts as a consonant (a sign).
pub(super) type TexChar = (char, TextStyle, bool);

/// The text `inner` prints, starting in `style`, and the first command it couldn't follow
/// (whose text, if any, is kept).
pub(super) fn text(inner: &str, style: TextStyle) -> (Vec<TexChar>, Option<String>) {
    let mut p = Parser {
        chars: inner.chars().collect(),
        i: 0,
        out: Vec::new(),
        stack: vec![style],
        flat: 0,
        unknown: None,
    };
    p.run(0);
    (p.out, p.unknown)
}

struct Parser {
    chars: Vec<char>,
    i: usize,
    out: Vec<TexChar>,
    /// The style of each open group, innermost last.
    stack: Vec<TextStyle>,
    /// Braces open past [`MAX_DEPTH`], read as plain text: they style nothing.
    flat: usize,
    unknown: Option<String>,
}

impl Parser {
    fn style(&mut self) -> &mut TextStyle {
        self.stack.last_mut().expect("the outer group stays open")
    }

    fn push(&mut self, c: char, sign: bool) {
        let st = *self.stack.last().expect("the outer group stays open");
        self.out.push((c, st, sign));
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    /// Reads until the group open at `depth` closes (or the end).
    fn run(&mut self, depth: usize) {
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                '{' if self.stack.len() >= MAX_DEPTH => self.flat += 1,
                '{' => self.stack.push(*self.stack.last().expect("open")),
                '}' if self.flat > 0 => self.flat -= 1,
                '}' => {
                    if self.stack.len() > 1 {
                        self.stack.pop();
                    }
                    if self.stack.len() <= depth {
                        return;
                    }
                }
                '\\' => self.command(),
                '$' => {}
                '~' => self.push(' ', false),
                '-' if self.peek() == Some('-') => {
                    self.i += 1;
                    if self.peek() == Some('-') {
                        self.i += 1;
                        self.push('\u{2014}', false);
                    } else {
                        self.push('\u{2013}', false);
                    }
                }
                c if c.is_whitespace() => {
                    // TeX reads a run of spaces as one.
                    if self.out.last().is_none_or(|l| l.0 != ' ') {
                        self.push(' ', false);
                    }
                    while self.peek().is_some_and(char::is_whitespace) {
                        self.i += 1;
                    }
                }
                c => self.push(c, false),
            }
        }
    }

    /// Reads a braced argument, in a group of its own styled by `f`.
    fn group(&mut self, f: impl FnOnce(&mut TextStyle)) {
        self.skip_spaces();
        if self.stack.len() >= MAX_DEPTH {
            // Too deep to style: the argument's text is read where it stands.
            if self.peek() == Some('{') {
                self.i += 1;
                self.flat += 1;
            }
            return;
        }
        let mut st = *self.stack.last().expect("open");
        f(&mut st);
        if self.peek() == Some('{') {
            self.i += 1;
            self.stack.push(st);
            let depth = self.stack.len() - 1;
            self.run(depth);
        } else if let Some(c) = self.peek() {
            // A single-token argument.
            self.i += 1;
            self.stack.push(st);
            if c == '\\' {
                self.command();
            } else {
                self.push(c, false);
            }
            self.stack.pop();
        }
    }

    /// Skips a braced argument, returning its text.
    fn skip_group(&mut self) -> String {
        self.skip_spaces();
        if self.peek() != Some('{') {
            return self
                .peek()
                .map(|c| {
                    self.i += 1;
                    c.to_string()
                })
                .unwrap_or_default();
        }
        let mut depth = 0;
        let mut text = String::new();
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            if depth > 0 && !(depth == 1 && c == '{') {
                text.push(c);
            }
        }
        text
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    /// Skips a TeX dimension, as after `\kern`: `4pt`, `-0.5em`, `9 mm`.
    fn skip_dimension(&mut self) {
        self.skip_spaces();
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+'))
        {
            self.i += 1;
        }
        self.skip_spaces();
        let rest: String = self.chars[self.i..].iter().take(2).collect();
        if ["pt", "mm", "cm", "em", "ex", "in", "bp", "sp", "pc", "dd", "cc"].contains(&rest.as_str()) {
            self.i += 2;
        }
    }

    fn command(&mut self) {
        let Some(c) = self.peek() else { return };
        if !c.is_ascii_alphabetic() {
            self.i += 1;
            match c {
                '\'' | '`' | '^' | '"' | '~' | '=' | '.' => self.accent(c),
                ' ' => self.push(' ', false),
                '/' | '\\' | ',' | ';' | '!' | '-' => {}
                c => self.push(c, false),
            }
            return;
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        let name: String = self.chars[start..self.i].iter().collect();
        // `\hspace*`
        if self.peek() == Some('*') && name.ends_with("space") {
            self.i += 1;
        }
        // A control word eats the spaces after it.
        let at = self.i;
        self.skip_spaces();
        let glyph = |s: &str| s.chars().next();
        match name.as_str() {
            "textit" | "emph" | "textsl" => self.group(|s| s.italic = true),
            "textbf" => self.group(|s| s.bold = true),
            "textsc" => self.group(|s| s.small_caps = true),
            "textup" | "textrm" | "textnormal" | "textmd" => self.group(|s| {
                s.italic = false;
                s.bold = false;
                s.small_caps = false;
            }),
            "red" => self.group(|s| s.rubric = true),
            "redit" => self.group(|s| {
                s.rubric = true;
                s.italic = true;
            }),
            "black" => self.group(|s| s.rubric = false),
            "textcolor" => {
                let colour = self.skip_group();
                self.group(|s| s.rubric = is_red(&colour));
            }
            "color" => {
                let colour = self.skip_group();
                self.style().rubric = is_red(&colour);
            }
            "itshape" | "it" | "em" | "slshape" | "sl" => self.style().italic = true,
            "bfseries" | "bf" => self.style().bold = true,
            "scshape" | "sc" => self.style().small_caps = true,
            "upshape" | "rmfamily" | "normalfont" | "mdseries" => {
                let s = self.style();
                s.italic = false;
                if name == "normalfont" {
                    s.bold = false;
                    s.small_caps = false;
                }
            }
            "tiny" | "scriptsize" | "footnotesize" | "small" | "normalsize" | "large" | "Large" | "LARGE" | "huge" | "Huge" | "relax"
            | "nobreak" | "noindent" | "par" => {}
            "quad" | "qquad" | "enspace" | "enskip" | "thinspace" => self.push(' ', false),
            "hspace" | "vspace" | "hskip" | "vskip" | "kern" | "hfill" | "hfil" | "vfill" => {
                // Spacing isn't drawn; the text around it is.
                if name.ends_with("space") {
                    self.skip_group();
                } else if name.ends_with("kern") || name.ends_with("skip") {
                    self.skip_dimension();
                }
                self.unknown.get_or_insert_with(|| format!("\\{name}"));
            }
            "gresetspecial" => {
                self.skip_group();
                self.skip_group();
            }
            _ => {
                let sign = match name.as_str() {
                    "greheightstar" | "gresixstar" | "GreStar" | "star" | "textasteriskcentered" => Some('*'),
                    "grecross" | "grealtcross" | "GreDagger" | "gredagger" | "dag" | "dagger" | "textdagger" => Some('†'),
                    "ddag" | "ddagger" | "textdaggerdbl" => Some('‡'),
                    "Vbar" => Some('℣'),
                    "Rbar" => Some('℟'),
                    _ => None,
                };
                if let Some(c) = sign {
                    let mut st = *self.stack.last().expect("open");
                    st.rubric = true;
                    self.out.push((c, st, c != '‡'));
                    return;
                }
                let letter = match name.as_str() {
                    "ae" => "æ",
                    "AE" => "Æ",
                    "oe" => "œ",
                    "OE" => "Œ",
                    "ss" => "ß",
                    "o" => "ø",
                    "O" => "Ø",
                    "i" => "ı",
                    "j" => "ȷ",
                    "P" => "¶",
                    "S" => "§",
                    "copyright" => "©",
                    "guillemotleft" | "guillemetleft" => "«",
                    "guillemotright" | "guillemetright" => "»",
                    "dots" | "ldots" => "…",
                    "textendash" => "\u{2013}",
                    "textemdash" => "\u{2014}",
                    _ => "",
                };
                if let Some(c) = glyph(letter) {
                    self.push(c, false);
                    return;
                }
                if name == "c" {
                    self.i = at;
                    self.accent(',');
                    return;
                }
                // An unknown command: its arguments' text is kept, as when it only styles them.
                self.unknown.get_or_insert_with(|| format!("\\{name}"));
            }
        }
    }

    /// An accent command (`\'e`, `\'{\i}`, `\c{c}`): the accented letter, precomposed where
    /// there is one.
    fn accent(&mut self, accent: char) {
        let from = self.out.len();
        self.group(|_| {});
        if self.out.len() == from {
            return;
        }
        let (base, st, sign) = self.out[from];
        let base = match base {
            'ı' => 'i',
            'ȷ' => 'j',
            c => c,
        };
        let mark = match accent {
            '\'' => '\u{301}',
            '`' => '\u{300}',
            '^' => '\u{302}',
            '"' => '\u{308}',
            '~' => '\u{303}',
            '=' => '\u{304}',
            '.' => '\u{307}',
            _ => '\u{327}',
        };
        match compose(base, mark) {
            Some(c) => self.out[from] = (c, st, sign),
            None => {
                self.out[from].0 = base;
                self.out.insert(from + 1, (mark, st, sign));
            }
        }
    }
}

fn is_red(colour: &str) -> bool {
    let c = colour.trim().to_ascii_lowercase();
    c.contains("red") || c == "gregoriocolor" || c == "rubric"
}

/// The precomposed form of `base` with the combining `mark`, for the letters Latin uses.
fn compose(base: char, mark: char) -> Option<char> {
    const TABLE: &[(char, &str, &str)] = &[
        ('\u{301}', "aeiouyAEIOUYæÆcnsz", "áéíóúýÁÉÍÓÚÝǽǼćńśź"),
        ('\u{300}', "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
        ('\u{302}', "aeiouAEIOU", "âêîôûÂÊÎÔÛ"),
        ('\u{308}', "aeiouyAEIOUY", "äëïöüÿÄËÏÖÜŸ"),
        ('\u{303}', "anoANO", "ãñõÃÑÕ"),
        ('\u{304}', "aeiouAEIOU", "āēīōūĀĒĪŌŪ"),
        ('\u{327}', "cCsS", "çÇşŞ"),
    ];
    let (_, from, to) = TABLE.iter().find(|t| t.0 == mark)?;
    let k = from.chars().position(|c| c == base)?;
    to.chars().nth(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> (String, Option<String>) {
        let (out, unknown) = text(s, TextStyle::REGULAR);
        (out.iter().map(|c| c.0).collect(), unknown)
    }

    #[test]
    fn keeps_what_tex_prints() {
        assert_eq!(plain(r"\textit{v}"), ("v".into(), None));
        assert_eq!(plain(r"\'y"), ("ý".into(), None));
        assert_eq!(plain(r"{\'a}"), ("á".into(), None));
        assert_eq!(plain(r"\'\ae"), ("ǽ".into(), None));
        assert_eq!(plain(r#"\"{e}"#), ("ë".into(), None));
        assert_eq!(plain(r"K\'y"), ("Ký".into(), None));
        assert_eq!(plain(r"\textcolor{red}{S}"), ("S".into(), None));
        assert_eq!(plain(r"{\scriptsize\itshape t.p.\ }"), ("t.p. ".into(), None));
        assert_eq!(
            plain(r"\gresetspecial{S.}{\textcolor{gregoriocolor}{\textit{S.}}}"),
            ("".into(), None)
        );
        assert_eq!(plain(r"\color{red}\textit{Ps. 119.}\color{black}"), ("Ps. 119.".into(), None));
        assert_eq!(plain(r"--\kern4pt"), ("\u{2013}".into(), Some(r"\kern".into())));
        assert_eq!(plain(r"\hspace{0.2em}"), ("".into(), Some(r"\hspace".into())));
        assert_eq!(plain(r"\foo{bar}"), ("bar".into(), Some(r"\foo".into())));
        // Styles reach the letters they apply to.
        let (out, _) = text(r"\textcolor{red}{A}b\emph{c}", TextStyle::REGULAR);
        assert!(out[0].1.rubric && !out[1].1.rubric && out[2].1.italic && !out[1].1.italic);
    }

    #[test]
    fn nesting_without_end_is_read_as_text() {
        let n = 100_000;
        for src in [
            format!("{}a{}", r"\textit{".repeat(n), "}".repeat(n)),
            format!("{} a", r"\textit".repeat(n)),
            format!("{}a{}", "{".repeat(n), "}".repeat(n)),
            format!("{}a", r"\'".repeat(n)),
        ] {
            let (out, _) = text(&src, TextStyle::REGULAR);
            assert!(out.iter().any(|c| c.0 == 'a'), "{}", &src[..40]);
        }
        // Text after deep nesting closes is read as usual.
        let deep = format!("{}a{}b", r"\textit{".repeat(40), "}".repeat(40));
        let (out, _) = text(&deep, TextStyle::REGULAR);
        assert_eq!(out.iter().map(|c| c.0).collect::<String>(), "ab");
        assert!(out[0].1.italic && !out[1].1.italic);
    }
}
