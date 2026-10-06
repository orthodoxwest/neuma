//! GABC parsing, following Gregorio's documentation of the format ("The GABC File" in
//! GregorioRef). Parsing never fails: problems become diagnostics and the parser recovers.

mod lyric;
mod notes;
mod write;

use crate::diag::{Diagnostic, Sink};
use crate::score::{Header, Score, Syllable};

pub use write::to_gabc;

/// A parsed score and everything the parser had to say about it.
#[derive(Clone, Debug)]
pub struct Parsed {
    pub score: Score,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses GABC source. Never fails.
pub fn parse(src: &str) -> Parsed {
    let mut sink = Sink::default();
    let (header, mut body_start) = parse_header(src, &mut sink);
    // A byte-order mark isn't text; spans still count it, so they index `src`.
    if src[body_start..].starts_with('\u{feff}') {
        body_start += '\u{feff}'.len_utf8();
    }
    let syllables = parse_body(src, body_start, &mut sink);
    // An NABC score has NABC in nearly every syllable, and a score that uses zero-width notes
    // uses them throughout; one diagnostic says each.
    for (code, all) in [
        ("gabc::nabc", "NABC notation isn't supported and is skipped throughout the score"),
        (
            "gabc::zero-width",
            "notes in `{…}` are drawn with their own width throughout the score",
        ),
    ] {
        if sink.items.iter().filter(|d| d.code == code).count() > 1 {
            let mut seen = false;
            sink.items.retain(|d| d.code != code || !std::mem::replace(&mut seen, true));
            if let Some(d) = sink.items.iter_mut().find(|d| d.code == code) {
                d.message = all.into();
            }
        }
    }
    Parsed {
        score: Score { header, syllables },
        diagnostics: sink.items,
    }
}

/// Finds the `%%` line that ends the header. Returns its byte offset and the offset after it.
fn find_separator(src: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']).trim_start_matches('\u{feff}').trim() == "%%" {
            return Some((offset, offset + line.len()));
        }
        offset += line.len();
    }
    None
}

/// Strips a `%` comment from a header line. GABC has no escape in headers, so a `%` always
/// starts a comment.
fn strip_comment(line: &str) -> &str {
    match line.find('%') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn parse_header(src: &str, sink: &mut Sink) -> (Header, usize) {
    let Some((sep, body_start)) = find_separator(src) else {
        // No header: Gregorio requires one, but a bare body is common in snippets.
        if !src.trim().is_empty() && !src.trim_start().starts_with('(') && looks_like_header(src) {
            sink.warn(0..0, "gabc::no-separator", "no `%%` line separates the header from the notes");
        }
        return (Header::default(), 0);
    };
    let mut header = Header::default();
    let text = &src[..sep];
    let mut offset = 0;
    let mut pending: Option<(String, String, usize)> = None;
    for raw in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let line = strip_comment(raw.trim_end_matches(['\r', '\n']));
        if let Some((name, mut value, start)) = pending.take() {
            // Continuing a multi-line value, which ends with `;;`, or like Gregorio, at a `;` that
            // ends a line.
            if is_header_line(line) {
                // A forgotten `;`: the next field starts here, not more of this value.
                sink.warn(
                    start..line_start,
                    "gabc::unterminated-header",
                    format!("header `{name}` has no closing `;` or `;;`"),
                );
                header.fields.push((name, value.trim().to_string()));
            } else {
                let end = line.find(";;").or_else(|| line.trim_end().strip_suffix(';').map(str::len));
                value.push('\n');
                if let Some(end) = end {
                    value.push_str(&line[..end]);
                    header.fields.push((name, value.trim().to_string()));
                } else {
                    value.push_str(line);
                    pending = Some((name, value, start));
                }
                continue;
            }
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some(colon) = line.find(':') else {
            sink.warn(
                line_start..line_start + line.len(),
                "gabc::bad-header",
                format!("header line has no `:`: {}", line.trim()),
            );
            continue;
        };
        // A byte-order mark before the first header isn't part of its name.
        let name = line[..colon].trim().trim_start_matches('\u{feff}').trim().to_string();
        let rest = &line[colon + 1..];
        if let Some(value) = rest.trim_end().strip_suffix(";;") {
            // A one-line value that itself contains `;`, as `to_gabc` writes it.
            header.fields.push((name, value.trim().to_string()));
        } else if let Some(end) = rest.find(';') {
            header.fields.push((name, rest[..end].trim().to_string()));
        } else {
            pending = Some((name, rest.trim().to_string(), line_start));
        }
    }
    if let Some((name, value, start)) = pending {
        sink.warn(
            start..sep,
            "gabc::unterminated-header",
            format!("header `{name}` has no closing `;` or `;;`"),
        );
        header.fields.push((name, value.trim().to_string()));
    }
    for (name, value) in &header.fields {
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("def-m") {
            sink.info(
                0..0,
                "gabc::macro-ignored",
                format!("`{name}` defines TeX, which neuma doesn't run"),
            );
        } else if lower == "oriscus-orientation" && value == "legacy" {
            sink.warn(
                0..0,
                "gabc::legacy-oriscus",
                "legacy oriscus orientation isn't supported; using the default rules",
            );
        } else if lower == "staff-lines" && value.trim() != "4" {
            sink.warn(
                0..0,
                "gabc::staff-lines",
                format!("only four-line staves are supported; `staff-lines: {value}` is drawn on four lines"),
            );
        } else if lower == "nabc-lines" {
            sink.warn(0..0, "gabc::nabc", "NABC notation isn't supported and is skipped");
        }
    }
    (header, body_start)
}

/// A `name:` line, named as Gregorio names header fields.
fn is_header_line(line: &str) -> bool {
    line.split_once(':').is_some_and(|(name, rest)| {
        let name = name.trim();
        !name.is_empty()
            && !rest.starts_with("//")
            && !name.starts_with('-')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

fn looks_like_header(src: &str) -> bool {
    src.lines().next().is_some_and(|l| {
        let l = l.trim();
        l.contains(':') && l.ends_with(';') && !l.contains('(')
    })
}

/// Lyric styling that stays open across syllables until its closing tag.
#[derive(Debug, Default)]
pub(crate) struct LyricState {
    pub italic: u8,
    pub bold: u8,
    pub small_caps: u8,
    pub underline: u8,
    pub color: u8,
    pub elision: u8,
    pub nlba: bool,
    pub euouae: bool,
}

fn parse_body(src: &str, start: usize, sink: &mut Sink) -> Vec<Syllable> {
    let body = &src[start..];
    let bytes = body.as_bytes();
    let mut syllables = Vec::new();
    let mut state = LyricState::default();
    let mut i = 0;
    let mut saw_space = true;
    let mut text_start = 0;
    let mut text = String::new();
    // Tags found to have no closer ahead, so each is searched for once rather than per opener.
    let mut unclosed = [false; 3];
    while i < bytes.len() {
        let c = body[i..].chars().next().unwrap_or('\0');
        match c {
            '%' => {
                // A comment runs to the end of the line and swallows the line's end.
                let end = body[i..].find('\n').map_or(body.len(), |n| i + n + 1);
                i = end;
            }
            '$' => {
                if text.is_empty() {
                    text_start = i;
                }
                let next = body[i + 1..].chars().next();
                text.push('$');
                if let Some(n) = next {
                    text.push(n);
                    i += 1 + n.len_utf8();
                } else {
                    i += 1;
                }
            }
            '<' if let Some(end) = verbatim_end(&body[i..], &mut unclosed) => {
                // Gregorio reads `<v>`, `<alt>` and `<sp>` to their closing tag, so a `(` inside
                // is text, not notes: `<v>(</v>` prints a parenthesis.
                if text.is_empty() {
                    text_start = i;
                }
                text.push_str(&body[i..i + end]);
                i += end;
            }
            '(' => {
                let close = find_close(body, i + 1);
                let notes_src = &body[i + 1..close];
                let syl_start = start + if text.trim().is_empty() { i } else { text_start };
                let trimmed = text.trim_start().to_string();
                let text_offset = start + text_start + (text.len() - trimmed.len());
                let trimmed = trimmed.trim_end().to_string();
                lint_hyphens(&trimmed, text_offset, sink);
                let nlba_before = state.nlba;
                let lyric = lyric::parse(&trimmed, text_offset, &mut state, sink);
                let notation = notes::parse(notes_src, start + i + 1, sink);
                if close >= body.len() {
                    sink.error(
                        start + i..start + body.len(),
                        "gabc::unclosed-notes",
                        "notes opened with `(` never close",
                    );
                }
                let word_start = saw_space || syllables.is_empty();
                syllables.push(Syllable {
                    text: lyric,
                    word_start,
                    notation,
                    span: syl_start..start + (close + 1).min(body.len()),
                    no_break_before: nlba_before && state.nlba,
                    no_break_within: state.nlba,
                    euouae: state.euouae,
                });
                text.clear();
                i = (close + 1).min(body.len());
                text_start = i;
                saw_space = false;
            }
            c if c.is_whitespace() => {
                if text.trim().is_empty() {
                    saw_space = true;
                    text.clear();
                    text_start = i + c.len_utf8();
                } else {
                    text.push(' ');
                }
                i += c.len_utf8();
            }
            _ => {
                if text.is_empty() {
                    text_start = i;
                }
                text.push(c);
                i += c.len_utf8();
            }
        }
    }
    if !text.trim().is_empty() {
        sink.warn(
            start + text_start..start + body.len(),
            "gabc::trailing-text",
            format!("text `{}` has no notes after it and is dropped", text.trim()),
        );
    }
    syllables
}

/// For text starting with `<v>`, `<alt>` or `<sp>`, the length through its closing tag. `None`
/// for other text, or when the tag never closes, which `unclosed` remembers per tag.
fn verbatim_end(text: &str, unclosed: &mut [bool; 3]) -> Option<usize> {
    let k = ["v", "alt", "sp"].into_iter().position(|t| {
        text.strip_prefix('<')
            .and_then(|r| r.strip_prefix(t))
            .is_some_and(|r| r.starts_with('>'))
    })?;
    if unclosed[k] {
        return None;
    }
    let close = ["</v>", "</alt>", "</sp>"][k];
    let end = text.find(close).map(|n| n + close.len());
    unclosed[k] = end.is_none();
    end
}

/// The index of the `)` that closes notes opened before `from`, or the end of the body.
fn find_close(body: &str, from: usize) -> usize {
    body[from..].find(')').map_or(body.len(), |n| from + n)
}

fn lint_hyphens(text: &str, offset: usize, sink: &mut Sink) {
    // `<sp>-</sp>` is Gregorio's zero-width hyphen and is fine.
    if text.starts_with('-') {
        sink.warn(
            offset..offset + 1,
            "gabc::hyphen-in-syllable",
            "a hyphen at the start of a syllable prints in addition to the hyphen the engine draws; remove it",
        );
    }
    if text.ends_with('-') && !text.ends_with("<sp>-</sp>") && !text.ends_with("$-") {
        let end = offset + text.len();
        sink.warn(
            end - 1..end,
            "gabc::hyphen-in-syllable",
            "a hyphen at the end of a syllable prints in addition to the hyphen the engine draws; remove it",
        );
    }
}

#[cfg(test)]
mod tests;
