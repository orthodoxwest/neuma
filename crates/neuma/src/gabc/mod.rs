//! GABC parsing, following Gregorio's documentation of the format ("The GABC File" in
//! GregorioRef). Parsing never fails: problems become diagnostics and the parser recovers.

mod lyric;
mod notes;
mod write;

use std::ops::Range;

use crate::diag::{Diagnostic, Fix, Sink};
use crate::score::{Figure, Header, Score, Syllable};

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
    // Without a `%%`, header lines read as text; a clef put before them wouldn't help.
    let headers_unseparated = body_start == 0 && header_lines(src).is_some();
    lint_clef(&syllables, !headers_unseparated, &mut sink);
    // An NABC score has NABC in nearly every syllable, and a score that uses zero-width notes
    // often uses them in many places; one diagnostic says each, with how often it applies.
    for (code, all) in [
        ("gabc::nabc", "NABC notation isn't supported and is skipped throughout the score"),
        (
            "gabc::zero-width",
            "notes in `{…}` are drawn with their own width, in all {n} groups",
        ),
    ] {
        let n = sink.items.iter().filter(|d| d.code == code).count();
        if n > 1 {
            let mut seen = false;
            sink.items.retain(|d| d.code != code || !std::mem::replace(&mut seen, true));
            if let Some(d) = sink.items.iter_mut().find(|d| d.code == code) {
                d.message = all.replace("{n}", &n.to_string());
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
        // No header: Gregorio requires one, but a bare body is common in snippets. Lines at
        // the top that read as headers want a `%%` after them.
        if let Some((end, sure)) = header_lines(src) {
            sink.warn(
                0..src[..end].trim_end().len(),
                "gabc::no-separator",
                "no `%%` line separates the header from the notes",
            );
            if sure {
                let insert = if src[..end].ends_with('\n') { "%%\n" } else { "\n%%\n" };
                sink.fix(Fix::new(end..end, insert, "Insert the `%%` line after the header"));
            }
        }
        return (Header::default(), 0);
    };
    let mut header = Header::default();
    let text = &src[..sep];
    let mut offset = 0;
    // A field still open: its name, value, start, and where its value's text ends so far.
    let mut pending: Option<(String, String, usize, usize)> = None;
    for raw in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let line = strip_comment(raw.trim_end_matches(['\r', '\n']));
        if let Some((name, mut value, start, value_end)) = pending.take() {
            // Continuing a multi-line value, which ends with `;;`, or like Gregorio, at a `;` that
            // ends a line.
            if is_header_line(line) {
                // A forgotten `;`: the next field starts here, not more of this value.
                sink.warn(
                    start..value_end,
                    "gabc::unterminated-header",
                    format!("header `{name}` has no closing `;` or `;;`"),
                );
                sink.fix(Fix::new(value_end..value_end, ";", "Insert `;`"));
                push(&mut header, name, &value, start..value_end);
            } else {
                let end = line
                    .find(";;")
                    .map(|e| (e, e + 2))
                    .or_else(|| line.trim_end().strip_suffix(';').map(|v| (v.len(), v.len() + 1)));
                value.push('\n');
                if let Some((end, close)) = end {
                    value.push_str(&line[..end]);
                    push(&mut header, name, &value, start..line_start + close);
                } else {
                    value.push_str(line);
                    let value_end = if line.trim().is_empty() {
                        value_end
                    } else {
                        line_start + line.trim_end().len()
                    };
                    pending = Some((name, value, start, value_end));
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
        let start = line_start + (line.len() - line.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}').len());
        let rest = &line[colon + 1..];
        if let Some(value) = rest.trim_end().strip_suffix(";;") {
            // A one-line value that itself contains `;`, as `to_gabc` writes it.
            push(&mut header, name, value, start..line_start + colon + 1 + rest.trim_end().len());
        } else if let Some(end) = rest.find(';') {
            push(&mut header, name, &rest[..end], start..line_start + colon + 1 + end + 1);
        } else {
            let value_end = line_start + line.trim_end().len();
            pending = Some((name, rest.trim().to_string(), start, value_end));
        }
    }
    if let Some((name, value, start, value_end)) = pending {
        sink.warn(
            start..value_end,
            "gabc::unterminated-header",
            format!("header `{name}` has no closing `;` or `;;`"),
        );
        sink.fix(Fix::new(value_end..value_end, ";", "Insert `;`"));
        push(&mut header, name, &value, start..value_end);
    }
    for ((name, value), span) in header.fields.iter().zip(&header.spans) {
        let lower = name.to_ascii_lowercase();
        let span = span.clone();
        if lower.starts_with("def-m") {
            sink.info(
                span,
                "gabc::macro-ignored",
                format!("`{name}` defines TeX, which neuma doesn't run"),
            );
        } else if lower == "oriscus-orientation" && value == "legacy" {
            sink.warn(
                span,
                "gabc::legacy-oriscus",
                "legacy oriscus orientation isn't supported; using the default rules",
            );
        } else if lower == "staff-lines" && value.trim() != "4" {
            sink.warn(
                span,
                "gabc::staff-lines",
                format!("only four-line staves are supported; `staff-lines: {value}` is drawn on four lines"),
            );
        } else if lower == "nabc-lines" {
            sink.warn(span, "gabc::nabc", "NABC notation isn't supported and is skipped");
        }
    }
    (header, body_start)
}

fn push(header: &mut Header, name: String, value: &str, span: Range<usize>) {
    header.fields.push((name, value.trim().to_string()));
    header.spans.push(span);
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

/// A line that reads as a whole header field: `name:` and a value that ends with `;`, or
/// holds no `(`, which would start notes (`dixit:(g)`). Comments don't count.
fn whole_header_line(line: &str) -> bool {
    let l = strip_comment(line).trim();
    is_header_line(l) && (l.ends_with(';') || !l.contains('('))
}

/// For a source with no `%%`: if its first line reads as a header field, where the run of
/// such lines at its top ends, and whether a `%%` there surely separates them from the notes
/// (the next line isn't a header field run together with notes, as `mode: 8; (c4) a(g)`).
fn header_lines(src: &str) -> Option<(usize, bool)> {
    let first = src.lines().find(|l| !l.trim().is_empty())?;
    if !whole_header_line(first) {
        return None;
    }
    let mut end = 0;
    let mut last_field = 0;
    for line in src.split_inclusive('\n') {
        if line.trim().is_empty() {
            end += line.len();
            continue;
        }
        if !whole_header_line(line) {
            let sure = !is_header_line(strip_comment(line).trim());
            return Some((last_field, sure));
        }
        end += line.len();
        last_field = end;
    }
    Some((last_field, true))
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
    /// Style tags not yet closed: the tag, its span, and where its syllable's text ends.
    pub open: Vec<lyric::OpenTag>,
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
                    let end = start + body.trim_end().len();
                    sink.error(start + i..end, "gabc::unclosed-notes", "notes opened with `(` never close");
                    sink.fix(Fix::new(end..end, ")", "Insert `)`"));
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
    for tag in std::mem::take(&mut state.open) {
        sink.warn(
            tag.span,
            "gabc::unclosed-tag",
            format!("`<{}>` is never closed, so it styles the rest of the score", tag.name),
        );
        if let Some(fix) = tag.fix {
            sink.fix(fix);
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

/// Notes before any clef are read in `c4`, as a missing clef is usually forgotten. `fix`
/// offers to insert one.
fn lint_clef(syllables: &[Syllable], fix: bool, sink: &mut Sink) {
    for f in syllables.iter().flat_map(|s| &s.notation) {
        match f {
            Figure::Clef(_) => return,
            Figure::Note(n) => {
                sink.warn(
                    n.span.clone(),
                    "gabc::no-clef",
                    "no clef before the first note; the notes are read in a do clef on the fourth line (`c4`)",
                );
                if fix {
                    let at = syllables[0].span.start;
                    sink.fix(Fix::new(at..at, "(c4) ", "Insert a `c4` clef"));
                }
                return;
            }
            _ => {}
        }
    }
}

fn lint_hyphens(text: &str, offset: usize, sink: &mut Sink) {
    // `<sp>-</sp>` is Gregorio's zero-width hyphen and is fine.
    if text.starts_with('-') {
        sink.warn(
            offset..offset + 1,
            "gabc::hyphen-in-syllable",
            "a hyphen at the start of a syllable prints in addition to the hyphen the engine draws; remove it",
        );
        sink.fix(Fix::new(offset..offset + 1, "", "Remove the hyphen"));
    }
    if text.ends_with('-') && !text.ends_with("<sp>-</sp>") && !text.ends_with("$-") {
        let end = offset + text.len();
        sink.warn(
            end - 1..end,
            "gabc::hyphen-in-syllable",
            "a hyphen at the end of a syllable prints in addition to the hyphen the engine draws; remove it",
        );
        sink.fix(Fix::new(end - 1..end, "", "Remove the hyphen"));
    }
}

#[cfg(test)]
mod tests;
