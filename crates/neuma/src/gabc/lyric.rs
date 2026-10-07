//! Syllable text: style tags, special characters, centering braces and escapes.

use super::LyricState;
use crate::diag::Sink;
use crate::score::{Lyric, LyricRun, TextStyle};

/// Parses one syllable's text. `offset` is the text's byte offset in the source, for spans.
pub(super) fn parse(text: &str, offset: usize, state: &mut LyricState, sink: &mut Sink) -> Lyric {
    let mut out = Builder::default();
    let mut i = 0;
    let b = text.as_bytes();
    while i < b.len() {
        let c = text[i..].chars().next().unwrap_or('\0');
        match c {
            '$' => {
                let next = text[i + 1..].chars().next();
                if let Some(n) = next {
                    // An escaped line break or tab is a space, as it would be unescaped: the
                    // writer can't keep it, and a lyric has no use for it.
                    let n = if n.is_whitespace() { ' ' } else { n };
                    out.push(n, style(state), state.elision > 0);
                    i += 1 + n.len_utf8();
                } else {
                    i += 1;
                }
            }
            '{' => {
                out.center_start = Some(out.chars);
                i += 1;
            }
            '}' => {
                out.center_end = Some(out.chars);
                i += 1;
            }
            '[' => {
                // Translation text (E2): skipped for now.
                let end = text[i..].find(']').map_or(text.len(), |n| i + n + 1);
                if &text[i..end] != "[/]" {
                    sink.info(
                        offset + i..offset + end,
                        "gabc::translation-ignored",
                        "translation text isn't drawn yet",
                    );
                }
                i = end;
            }
            '~' => {
                sink.info(
                    offset + i..offset + i + 1,
                    "gabc::lyric-tie",
                    "lyric ties are drawn as a space for now",
                );
                out.push(' ', style(state), false);
                i += 1;
            }
            '<' => {
                let Some(close) = text[i..].find('>') else {
                    out.push('<', style(state), state.elision > 0);
                    i += 1;
                    continue;
                };
                let tag = &text[i + 1..i + close];
                let after = i + close + 1;
                i = match tag {
                    "i" => bump(&mut state.italic, after),
                    "/i" => drop(&mut state.italic, after),
                    "b" => bump(&mut state.bold, after),
                    "/b" => drop(&mut state.bold, after),
                    "sc" => bump(&mut state.small_caps, after),
                    "/sc" => drop(&mut state.small_caps, after),
                    "ul" => bump(&mut state.underline, after),
                    "/ul" => drop(&mut state.underline, after),
                    "c" => bump(&mut state.color, after),
                    "/c" => drop(&mut state.color, after),
                    "e" => bump(&mut state.elision, after),
                    "/e" => drop(&mut state.elision, after),
                    "tt" | "/tt" => {
                        sink.info(
                            offset + i..offset + after,
                            "gabc::teletype",
                            "teletype text is set in the lyric face",
                        );
                        after
                    }
                    "eu" => {
                        state.euouae = true;
                        after
                    }
                    "/eu" => {
                        state.euouae = false;
                        after
                    }
                    "nlba" => {
                        state.nlba = true;
                        after
                    }
                    "/nlba" => {
                        state.nlba = false;
                        after
                    }
                    "clear" | "clear/" => after,
                    t if t == "pr" || t == "pr/" || t.starts_with("pr:") => after,
                    "sp" => {
                        let end = text[after..].find("</sp>").map_or(text.len(), |n| after + n);
                        let inner = &text[after..end];
                        match special(inner) {
                            Some(s) => {
                                let mut st = style(state);
                                if matches!(inner, "V/" | "R/" | "A/" | "*" | "+") {
                                    st.rubric = true;
                                }
                                for ch in s.chars() {
                                    out.push(ch, st, true);
                                }
                            }
                            None => sink.warn(
                                offset + after..offset + end,
                                "gabc::unknown-special",
                                format!("special character `<sp>{inner}</sp>` isn't defined"),
                            ),
                        }
                        (end + "</sp>".len()).min(text.len())
                    }
                    "v" => {
                        let end = text[after..].find("</v>").map_or(text.len(), |n| after + n);
                        let inner = &text[after..end];
                        if let Some(sign) = tex_sign(inner) {
                            let mut st = style(state);
                            st.rubric = true;
                            // As consonants, like `<sp>` characters; `‡` has no `<sp>` form.
                            for ch in sign.chars() {
                                out.push(ch, st, ch != '‡');
                            }
                        } else if !inner.contains(['\\', '{', '}', '$', '~', '^', '_', '%', '&', '#']) {
                            // No TeX in it, so TeX would print it as it stands: `<v>(</v>`, with
                            // any run of spaces and line ends as one space.
                            let mut space = false;
                            for ch in inner.chars() {
                                if ch.is_whitespace() {
                                    space = true;
                                    continue;
                                }
                                if std::mem::take(&mut space) {
                                    out.push(' ', style(state), state.elision > 0);
                                }
                                out.push(ch, style(state), state.elision > 0);
                            }
                            if space {
                                out.push(' ', style(state), state.elision > 0);
                            }
                        } else if inner.chars().any(char::is_alphanumeric) {
                            sink.warn(offset + i..offset + end, "gabc::verbatim-dropped", "verbatim TeX is dropped");
                        }
                        (end + "</v>".len()).min(text.len())
                    }
                    "alt" => {
                        let end = text[after..].find("</alt>").map_or(text.len(), |n| after + n);
                        sink.info(
                            offset + i..offset + end,
                            "gabc::above-lines-text",
                            "above-lines text isn't drawn yet",
                        );
                        (end + "</alt>".len()).min(text.len())
                    }
                    _ => {
                        sink.warn(
                            offset + i..offset + after,
                            "gabc::unknown-tag",
                            format!("unknown tag `<{tag}>` is set as text"),
                        );
                        for ch in text[i..after].chars() {
                            out.push(ch, style(state), false);
                        }
                        after
                    }
                };
            }
            _ => {
                out.push(c, style(state), state.elision > 0);
                i += c.len_utf8();
            }
        }
    }
    out.finish()
}

fn bump(n: &mut u8, after: usize) -> usize {
    *n = n.saturating_add(1);
    after
}

fn drop(n: &mut u8, after: usize) -> usize {
    *n = n.saturating_sub(1);
    after
}

fn style(state: &LyricState) -> TextStyle {
    TextStyle {
        italic: state.italic > 0 || state.elision > 0,
        bold: state.bold > 0,
        small_caps: state.small_caps > 0,
        underline: state.underline > 0,
        rubric: state.color > 0,
    }
}

/// The sign a common verbatim star or cross macro draws, as the matching `<sp>` character.
/// Only a macro counts: a plain word such as `<v>star</v>` is text, which TeX prints as it is.
fn tex_sign(inner: &str) -> Option<&'static str> {
    let name: String = inner.chars().filter(|c| !matches!(c, ' ' | '{' | '}' | '$')).collect();
    if !name.starts_with('\\') {
        return None;
    }
    let name = name.trim_matches('\\');
    Some(match name {
        // GregorioTeX's eight- and six-pointed stars, set where the books print an asterisk.
        "greheightstar" | "gresixstar" | "GreStar" | "star" => "*",
        "grecross" | "grealtcross" | "GreDagger" | "gredagger" | "dag" | "dagger" => "†",
        "ddag" | "ddagger" => "‡",
        _ => return None,
    })
}

/// Gregorio's default special characters.
fn special(inner: &str) -> Option<&'static str> {
    Some(match inner {
        "V/" => "℣",
        "R/" => "℟",
        "A/" => "A\u{0336}",
        "*" => "*",
        "+" => "†",
        "-" => "-",
        "ae" => "æ",
        "oe" => "œ",
        "'ae" | "'æ" => "ǽ",
        "'oe" | "'œ" => "œ\u{0301}",
        "\\" => "\\",
        "&" => "&",
        "#" => "#",
        "_" => "_",
        _ => return None,
    })
}

#[derive(Default)]
struct Builder {
    runs: Vec<LyricRun>,
    chars: usize,
    center_start: Option<usize>,
    center_end: Option<usize>,
}

impl Builder {
    fn push(&mut self, c: char, style: TextStyle, consonant: bool) {
        // A byte-order mark past the start of the file is invisible and isn't lyric text; kept,
        // it would read back as the body's own byte-order mark and vanish.
        if c == '\u{feff}' {
            return;
        }
        match self.runs.last_mut() {
            Some(last) if last.style == style && last.consonant == consonant => last.text.push(c),
            _ => self.runs.push(LyricRun {
                text: c.to_string(),
                style,
                consonant,
            }),
        }
        self.chars += 1;
    }

    fn finish(mut self) -> Lyric {
        // Tags can leave whitespace at either end (`</nlba> <eu>e`); it isn't lyric text.
        while let Some(first) = self.runs.first_mut() {
            let n = first.text.chars().take_while(|c| *c == ' ').count();
            if n == 0 {
                break;
            }
            first.text.drain(..n);
            self.center_start = self.center_start.map(|c| c.saturating_sub(n));
            self.center_end = self.center_end.map(|c| c.saturating_sub(n));
            self.chars -= n;
            if first.text.is_empty() {
                self.runs.remove(0);
            }
        }
        while let Some(last) = self.runs.last_mut() {
            let n = last.text.chars().rev().take_while(|c| *c == ' ').count();
            if n == 0 {
                break;
            }
            let keep = last.text.len() - n;
            last.text.truncate(keep);
            self.chars -= n;
            if last.text.is_empty() {
                self.runs.pop();
            }
        }
        let center = match (self.center_start, self.center_end) {
            (Some(s), Some(e)) if e > s => Some(s..e),
            _ => None,
        };
        Lyric { runs: self.runs, center }
    }
}
