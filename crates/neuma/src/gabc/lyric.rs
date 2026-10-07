//! Syllable text: style tags, special characters, centering braces and escapes.

use super::{LyricState, TextMap};
use std::ops::Range;

use crate::diag::{Fix, Sink};
use crate::score::{Lyric, LyricRun, TextStyle};

/// Parses one syllable's text. `map` gives where its bytes are in the source, for spans.
pub(super) fn parse(text: &str, map: &TextMap, state: &mut LyricState, sink: &mut Sink) -> Lyric {
    let mut out = Builder::default();
    // Where text runs on to the syllable's end (an unended translation, `<` or verbatim tag),
    // so a closer appended there would be taken into it; and a style closer left without its
    // `>` at the end, as `</i`.
    let mut swallow: Option<usize> = None;
    let mut half_closer: Option<&str> = None;
    let mut i = 0;
    let b = text.as_bytes();
    while i < b.len() {
        let c = text[i..].chars().next().unwrap_or('\0');
        match c {
            '$' => {
                let next = text[i + 1..].chars().next();
                if let Some(n) = next {
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
                let end = text[i..].find(']').map_or_else(
                    || {
                        swallow.get_or_insert(i);
                        text.len()
                    },
                    |n| i + n + 1,
                );
                if &text[i..end] != "[/]" {
                    sink.info(map.span(i, end), "gabc::translation-ignored", "translation text isn't drawn yet");
                }
                i = end;
            }
            '~' => {
                sink.info(map.span(i, i + 1), "gabc::lyric-tie", "lyric ties are drawn as a space for now");
                out.push(' ', style(state), false);
                i += 1;
            }
            '<' => {
                let Some(close) = text[i..].find('>') else {
                    swallow.get_or_insert(i);
                    half_closer = text[i + 1..]
                        .strip_prefix('/')
                        .and_then(|name| STYLE_TAGS.into_iter().find(|t| *t == name));
                    out.push('<', style(state), state.elision > 0);
                    i += 1;
                    continue;
                };
                let tag = &text[i + 1..i + close];
                let after = i + close + 1;
                track_style(state, tag, map.span(i, after), map.end(text.len()));
                // `<sp>`, `<v>` and `<alt>` run to their closer, or to the end of the syllable.
                let verbatim = ["sp", "v", "alt"].into_iter().find(|t| *t == tag);
                if let Some(t) = verbatim
                    && !text[after..].contains(&format!("</{t}>"))
                {
                    sink.warn(
                        map.span(i, after),
                        "gabc::unclosed-tag",
                        format!("`<{t}>` has no `</{t}>` in its syllable, so it runs to the syllable's end"),
                    );
                    let end = map.end(text.len());
                    let half = format!("</{t}");
                    if !state.verbatim_unclosed.contains(&t) {
                        state.verbatim_unclosed.push(t);
                        // A closer left without its `>` (`<sp>ae</sp`) is completed; another
                        // would only be taken into the text.
                        match text[after..].find(&half) {
                            Some(k) => {
                                let at = after + k + half.len();
                                sink.fix(Fix::new(map.span(at, at), ">", format!("Complete `</{t}>`")));
                            }
                            None => sink.fix(Fix::new(
                                end..end,
                                format!("</{t}>"),
                                format!("Close `<{t}>` at the end of its syllable"),
                            )),
                        }
                    }
                    swallow.get_or_insert(i);
                }
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
                        sink.info(map.span(i, after), "gabc::teletype", "teletype text is set in the lyric face");
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
                        // Without its closer, as `<sp>ae</sp`, the text runs to the syllable's
                        // end; a closer missing only its `>` isn't part of the name.
                        let inner = &text[after..end];
                        let inner = inner.find("</sp").map_or(inner, |k| &inner[..k]);
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
                                map.span(after, after + inner.len()),
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
                            sink.warn(map.span(i, end), "gabc::verbatim-dropped", "verbatim TeX is dropped");
                        }
                        (end + "</v>".len()).min(text.len())
                    }
                    "alt" => {
                        let end = text[after..].find("</alt>").map_or(text.len(), |n| after + n);
                        sink.info(map.span(i, end), "gabc::above-lines-text", "above-lines text isn't drawn yet");
                        (end + "</alt>".len()).min(text.len())
                    }
                    _ => {
                        sink.warn(
                            map.span(i, after),
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
    settle_fixes(state, text, map, swallow, half_closer);
    out.finish()
}

/// A style tag not yet closed, and how to close it in its syllable, if that can be done.
#[derive(Debug)]
pub(crate) struct OpenTag {
    pub name: &'static str,
    pub span: Range<usize>,
    syllable_end: usize,
    pub fix: Option<Fix>,
}

/// Points the fixes of tags opened in a syllable past which text runs to its end at where that
/// text starts, or completes a closer missing its `>`, or drops the fix when neither will do.
fn settle_fixes(state: &mut LyricState, text: &str, map: &TextMap, swallow: Option<usize>, half_closer: Option<&str>) {
    let end = map.end(text.len());
    for tag in state.open.iter_mut().filter(|t| t.syllable_end == end) {
        tag.fix = if half_closer == Some(tag.name) {
            Some(Fix::new(end..end, ">", format!("Complete `</{}>`", tag.name)))
        } else {
            match swallow {
                None => continue,
                Some(at) if map.at(at) >= tag.span.end => {
                    let c = text[at..].chars().next().unwrap_or('[');
                    Some(Fix::new(
                        map.span(at, at),
                        format!("</{}>", tag.name),
                        format!("Close `<{}>` before `{c}`", tag.name),
                    ))
                }
                Some(_) => None,
            }
        };
    }
}

/// The style tags, which stay open across syllables until closed.
const STYLE_TAGS: [&str; 6] = ["i", "b", "sc", "ul", "c", "e"];

/// Keeps `state.open` in step with a style tag opening or closing.
fn track_style(state: &mut LyricState, tag: &str, span: Range<usize>, syllable_end: usize) {
    if let Some(name) = STYLE_TAGS.into_iter().find(|t| *t == tag) {
        let fix = Fix::new(
            syllable_end..syllable_end,
            format!("</{name}>"),
            format!("Close `<{name}>` at the end of its syllable"),
        );
        state.open.push(OpenTag {
            name,
            span,
            syllable_end,
            fix: Some(fix),
        });
    } else if let Some(name) = tag.strip_prefix('/')
        && let Some(k) = state.open.iter().rposition(|o| o.name == name)
    {
        state.open.remove(k);
    }
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
