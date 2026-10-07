//! GABC parsing, following Gregorio's documentation of the format ("The GABC File" in
//! GregorioRef). Parsing never fails: problems become diagnostics and the parser recovers.

mod lyric;
mod notes;
mod reparse;
mod tex;
mod write;

pub(crate) use reparse::{Diff, common_prefix, common_suffix, reparse};

use std::ops::Range;

use crate::diag::{Diagnostic, Fix, Sink};
use crate::score::{Figure, Header, Score, Syllable};

pub(crate) use write::to_gabc;

/// A parsed score and everything the parser had to say about it.
#[derive(Clone, Debug)]
pub struct Parsed {
    pub score: Score,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses GABC source. Never fails.
pub fn parse(src: &str) -> Parsed {
    let mut parsed = parse_unchecked(src);
    check_fixes(src, &mut parsed);
    parsed
}

/// Parses `src` as [`parse`] does, and keeps in `marks` what [`reparse`] needs to parse an
/// edit of it again.
pub(crate) fn parse_keeping(src: &str, marks: &mut ParseMarks) -> Parsed {
    let mut parsed = parse_marked(src, Some(marks));
    check_fixes(src, &mut parsed);
    parsed
}

/// A closer put in for a verbatim tag can be taken by another opener, Gregorio's way, and
/// make text of notes. Each such fix (at most one per tag, as only the first unclosed opener
/// of each gets one) is tried, and kept only if it clears its diagnostic and keeps every
/// note. A style tag's closer can't make text of notes, so those aren't tried.
fn check_fixes(src: &str, parsed: &mut Parsed) {
    let notes = |score: &Score| {
        score
            .syllables
            .iter()
            .flat_map(|s| &s.notation)
            .filter(|f| matches!(f, Figure::Note(_)))
            .count()
    };
    let mut before = None;
    for d in &mut parsed.diagnostics {
        let verbatim = VERBATIM.iter().any(|t| src.get(d.span.clone()) == Some(&format!("<{t}>")[..]));
        if d.code != "gabc::unclosed-tag" || d.fix.is_none() || !verbatim {
            continue;
        }
        #[cfg(test)]
        tests::RECHECKS.with(|n| n.set(n.get() + 1));
        let Some(fixed) = d.fix.as_ref().and_then(|f| f.apply(src)) else {
            d.fix = None;
            continue;
        };
        let after = parse_unchecked(&fixed);
        let cleared = !after.diagnostics.iter().any(|a| a.code == d.code && a.span == d.span);
        let before = *before.get_or_insert_with(|| notes(&parsed.score));
        if !cleared || notes(&after.score) < before {
            d.fix = None;
        }
    }
}

fn parse_unchecked(src: &str) -> Parsed {
    parse_marked(src, None)
}

fn parse_marked(src: &str, keep: Option<&mut ParseMarks>) -> Parsed {
    let mut sink = Sink::default();
    let (header, mut body_start) = parse_header(src, &mut sink);
    // A byte-order mark isn't text; spans still count it, so they index `src`.
    if src[body_start..].starts_with('\u{feff}') {
        body_start += '\u{feff}'.len_utf8();
    }
    // A syllable to each `(`, near enough, so the list is allocated once.
    let groups = src.as_bytes()[body_start..].iter().filter(|&&b| b == b'(').count();
    let mut syllables = Vec::with_capacity(groups);
    let mut state = BodyState::default();
    let mut marks = keep.is_some().then(|| Vec::with_capacity(groups));
    let rest = read_body(
        src,
        body_start,
        0,
        &mut state,
        &mut syllables,
        &mut sink,
        marks.as_mut(),
        None,
        &mut |_, _| false,
    );
    let found = sink.items.len();
    if let Some(rest) = rest {
        end_body(src, body_start, state, rest, &mut sink);
    }
    if let (Some(keep), Some(marks)) = (keep, marks) {
        *keep = ParseMarks {
            body_start,
            marks,
            found: sink.items[..found].to_vec(),
            end: sink.items[found..].to_vec(),
        };
    }
    finish(src, header, syllables, sink)
}

/// The score-wide checks, once the syllables are read.
fn finish(src: &str, header: Header, syllables: Vec<Syllable>, mut sink: Sink) -> Parsed {
    // Without a `%%`, header lines read as text; a clef put before them wouldn't help.
    let headers_unseparated = find_separator(src).is_none() && header_lines(src).is_some();
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

/// How a line at the top of a source with no `%%` reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TopLine {
    /// Blank, or only a comment.
    Empty,
    /// A header field, `name: value;`, with nothing like notes in it.
    Field,
    /// A header field that might be notes: `dixit: a(g) b(h);`, or `name: a (b);`.
    MaybeField,
    Other,
}

fn top_line(line: &str) -> TopLine {
    let l = strip_comment(line).trim_start_matches('\u{feff}').trim();
    if l.is_empty() {
        TopLine::Empty
    } else if !is_header_line(l) {
        TopLine::Other
    } else if !l.contains('(') {
        TopLine::Field
    } else if l.ends_with(';') {
        TopLine::MaybeField
    } else {
        TopLine::Other
    }
}

/// Whether a line holds a `(`…`)` group, as notes do.
fn has_group(line: &str) -> bool {
    line.find('(').is_some_and(|i| line[i..].contains(')'))
}

/// For a source with no `%%`: if its first line reads as a header field, where the run of
/// such lines at its top ends, and whether a `%%` there surely separates them from the notes:
/// the last of them is surely a field, and nothing after them looks like one before the notes
/// start (as `mode: 8; (c4) a(g)`, or fields after a line that breaks the run).
fn header_lines(src: &str) -> Option<(usize, bool)> {
    let first = src.lines().map(top_line).find(|t| *t != TopLine::Empty)?;
    if first == TopLine::Other {
        return None;
    }
    let mut end = 0;
    let mut last = (0, TopLine::Empty);
    // A line in the run that might be notes, under a name that isn't a header field's.
    let mut doubtful = false;
    let mut lines = src.split_inclusive('\n');
    for line in lines.by_ref() {
        match top_line(line) {
            TopLine::Empty => {}
            TopLine::Other => {
                // Unsure if this line, or any up to the notes, looks like a header field.
                let rest = std::iter::once(line).chain(lines);
                let mut before_notes = rest.scan(false, |notes, l| {
                    let was = *notes;
                    *notes = *notes || has_group(strip_comment(l));
                    (!was).then_some(l)
                });
                let fields_after = before_notes.any(|l| is_header_line(strip_comment(l).trim()));
                return Some((last.0, last.1 == TopLine::Field && !fields_after && !doubtful));
            }
            t => {
                if t == TopLine::MaybeField {
                    let l = strip_comment(line).trim_start_matches('\u{feff}');
                    let name = l.split_once(':').map_or("", |(n, _)| n.trim());
                    doubtful |= has_group(l) && !KNOWN_FIELDS.contains(&name.to_ascii_lowercase().as_str());
                }
                last = (end + line.len(), t);
            }
        }
        end += line.len();
    }
    Some((last.0, last.1 == TopLine::Field && !doubtful))
}

/// Header fields Gregorio and the scores in use know, whose values may hold parentheses
/// (`name: Kyrie II. (Rex Magne);`) without being notes.
const KNOWN_FIELDS: &[&str] = &[
    "name",
    "title",
    "annotation",
    "author",
    "arranger",
    "book",
    "commentary",
    "date",
    "gabc-copyright",
    "score-copyright",
    "manuscript",
    "manuscript-reference",
    "manuscript-storage-place",
    "mode",
    "mode-modifier",
    "mode-differentia",
    "occasion",
    "office-part",
    "meter",
    "transcriber",
    "transcription-date",
    "user-notes",
    "def-macro",
    "language",
    "gregoriotex-font",
    "font",
];

/// Lyric styling that stays open across syllables until its closing tag.
#[derive(Clone, Debug, Default, PartialEq)]
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
    /// Where the first opener of each verbatim tag (`v`, `alt`, `sp`) with no closer after it
    /// is in the source. A closer put in for a later one would close that one instead, as
    /// Gregorio reads an opener to the next closer, taking all between as text: only that
    /// opener gets a fix, and none does when it is hidden (in a translation, say).
    pub verbatim_first: [Option<usize>; 3],
}

/// What reading the body carries from one syllable to the next.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BodyState {
    pub lyric: LyricState,
    /// Verbatim tags found to have no closer ahead, so each is searched for once rather than
    /// per opener.
    pub unclosed: [bool; 3],
}

/// Where the body's reading stood after a syllable.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BodyMark {
    /// Where the syllable ends: the next one's text is read from here.
    pub end: usize,
    /// How far reading this syllable and those before it looked: an edit from here on leaves
    /// them as they were. Past the source's end when they read to its end.
    pub read: usize,
    /// How many diagnostics there were.
    pub diagnostics: usize,
    pub state: BodyState,
}

/// What a parse keeps to parse its source again after an edit (see [`reparse`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ParseMarks {
    pub body_start: usize,
    /// One per syllable.
    pub marks: Vec<BodyMark>,
    /// The header's diagnostics and the body's, as read, before those at the body's end and
    /// the score-wide ones.
    pub found: Vec<Diagnostic>,
    /// Those at the body's end: tags never closed, and text after the last notes.
    pub end: Vec<Diagnostic>,
}

/// The text after the last syllable read: where it starts in the body, and the text.
pub(crate) struct Rest {
    text_start: usize,
    text: String,
}

/// Reads the body of `src`, which starts at `start`, from its byte `i` on (0, or where a
/// syllable ends), with `state` carried from the syllables before, onto `syllables`.
/// `before` is how far reading the syllables before looked, when there are any (they need
/// not be in `syllables`). After
/// each syllable, `meet` is told where it ended and the state then; when it returns true
/// reading stops there, and `None` is returned. Otherwise reading goes to the end, and
/// returns the text after the last syllable.
#[allow(clippy::too_many_arguments)]
pub(crate) fn read_body<M: FnMut(usize, &BodyState) -> bool>(
    src: &str,
    start: usize,
    mut i: usize,
    st: &mut BodyState,
    syllables: &mut Vec<Syllable>,
    sink: &mut Sink,
    mut marks: Option<&mut Vec<BodyMark>>,
    before: Option<usize>,
    meet: &mut M,
) -> Option<Rest> {
    let body = &src[start..];
    let bytes = body.as_bytes();
    let mut saw_space = i == 0;
    let mut text_start = i;
    let mut text = String::new();
    // Where each byte of `text` came from in the source: comments are left out of the text and
    // whitespace is read as one space, so text offsets aren't source offsets.
    let mut from: Vec<(usize, usize)> = Vec::new();
    let mut read = before.unwrap_or(0);
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
                let len = 1 + next.map_or(0, char::len_utf8);
                text.push_str(&body[i..i + len]);
                copied(&mut from, start + i, len);
                i += len;
            }
            '<' if let Some(end) = verbatim_end(&body[i..], start + i, &mut st.unclosed, &mut st.lyric.verbatim_first) => {
                // Gregorio reads `<v>`, `<alt>` and `<sp>` to their closing tag, so a `(` inside
                // is text, not notes: `<v>(</v>` prints a parenthesis.
                if text.is_empty() {
                    text_start = i;
                }
                text.push_str(&body[i..i + end]);
                copied(&mut from, start + i, end);
                i += end;
            }
            '<' if st.unclosed.iter().any(|u| *u)
                || body[i..].starts_with("<v>")
                || body[i..].starts_with("<alt>")
                || body[i..].starts_with("<sp>") =>
            {
                // A verbatim opener with no closer ahead: whether it has one depends on the
                // rest of the body, to its end.
                read = src.len() + 1;
                if text.is_empty() {
                    text_start = i;
                }
                text.push('<');
                copied(&mut from, start + i, 1);
                i += 1;
            }
            '(' => {
                let mut close = find_close(body, i + 1);
                // How far this syllable's reading looks: through the `)`, or past the end (as
                // text added there would change it).
                read = read.max(if close >= body.len() { src.len() + 1 } else { start + close + 1 });
                // Another `(` before the `)`: this group was never closed, and text went on
                // after it (`A(fg men(f)`). It ends where that text starts, at its first space.
                let unclosed = body[i + 1..close].contains('(');
                if unclosed {
                    let inner = &body[i + 1..close];
                    let open = inner.find('(').unwrap_or(inner.len());
                    let end = inner[..open].find(char::is_whitespace).unwrap_or(open);
                    close = i + 1 + end;
                }
                let notes_src = &body[i + 1..close];
                let syl_start = start + if text.trim().is_empty() { i } else { text_start };
                let lead = text.len() - text.trim_start().len();
                let trimmed = text.trim().to_string();
                let map = TextMap {
                    from: &from[lead..lead + trimmed.len()],
                    base: start + text_start + lead,
                };
                lint_hyphens(&trimmed, &map, sink);
                let nlba_before = st.lyric.nlba;
                let lyric = lyric::parse(&trimmed, &map, &mut st.lyric, sink);
                let notation = notes::parse(notes_src, start + i + 1, sink);
                if unclosed {
                    let end = start + close;
                    sink.error(
                        start + i..end,
                        "gabc::unclosed-notes",
                        "notes opened with `(` aren't closed before the next `(`",
                    );
                    sink.fix(Fix::new(end..end, ")", "Insert `)`"));
                } else if close >= body.len() {
                    let end = start + body.trim_end().len();
                    sink.error(start + i..end, "gabc::unclosed-notes", "notes opened with `(` never close");
                    sink.fix(Fix::new(end..end, ")", "Insert `)`"));
                }
                let word_start = saw_space || before.is_none() && syllables.is_empty();
                let end = if unclosed { close } else { (close + 1).min(body.len()) };
                syllables.push(Syllable {
                    text: lyric,
                    word_start,
                    notation,
                    span: syl_start..start + end,
                    no_break_before: nlba_before && st.lyric.nlba,
                    no_break_within: st.lyric.nlba,
                    euouae: st.lyric.euouae,
                });
                text.clear();
                from.clear();
                i = end;
                text_start = i;
                saw_space = false;
                if let Some(marks) = marks.as_deref_mut() {
                    marks.push(BodyMark {
                        end: start + i,
                        read,
                        diagnostics: sink.items.len(),
                        state: st.clone(),
                    });
                }
                if meet(start + i, st) {
                    return None;
                }
            }
            c if c.is_whitespace() => {
                if text.trim().is_empty() {
                    saw_space = true;
                    text.clear();
                    from.clear();
                    text_start = i + c.len_utf8();
                } else {
                    text.push(' ');
                    from.push((start + i, start + i + c.len_utf8()));
                }
                i += c.len_utf8();
            }
            _ => {
                if text.is_empty() {
                    text_start = i;
                }
                text.push(c);
                copied(&mut from, start + i, c.len_utf8());
                i += c.len_utf8();
            }
        }
    }
    Some(Rest { text_start, text })
}

/// What the end of the body adds: a warning for each tag never closed, and for text after the
/// last notes.
fn end_body(src: &str, start: usize, mut st: BodyState, rest: Rest, sink: &mut Sink) {
    let body = &src[start..];
    for tag in std::mem::take(&mut st.lyric.open) {
        sink.warn(
            tag.span,
            "gabc::unclosed-tag",
            format!("`<{}>` is never closed, so it styles the rest of the score", tag.name),
        );
        if let Some(fix) = tag.fix {
            sink.fix(fix);
        }
    }
    if !rest.text.trim().is_empty() {
        sink.warn(
            start + rest.text_start..start + body.len(),
            "gabc::trailing-text",
            format!("text `{}` has no notes after it and is dropped", rest.text.trim()),
        );
    }
}

/// The verbatim tags, in the order `verbatim_end` and `LyricState::verbatim_first` index them.
pub(crate) const VERBATIM: [&str; 3] = ["v", "alt", "sp"];

/// For text starting with `<v>`, `<alt>` or `<sp>`, the length through its closing tag. `None`
/// for other text, or when the tag never closes, which `unclosed` remembers per tag, and
/// `first` where that opener (at `at` in the source) is.
fn verbatim_end(text: &str, at: usize, unclosed: &mut [bool; 3], first: &mut [Option<usize>; 3]) -> Option<usize> {
    let k = VERBATIM.into_iter().position(|t| {
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
    if end.is_none() {
        first[k] = Some(at);
    }
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

/// Notes that `len` bytes of text were copied from the source at `at`.
fn copied(from: &mut Vec<(usize, usize)>, at: usize, len: usize) {
    from.extend((at..at + len).map(|b| (b, b + 1)));
}

/// Where a syllable's text came from in the source: for each byte of the text, the source
/// bytes it stands for.
pub(crate) struct TextMap<'a> {
    from: &'a [(usize, usize)],
    /// Where the text starts, for an empty one.
    base: usize,
}

impl TextMap<'_> {
    /// The source offset of the text's byte `k`, or the text's end for `k` past it.
    pub fn at(&self, k: usize) -> usize {
        self.from.get(k).map_or_else(|| self.end(self.from.len()), |f| f.0)
    }

    /// The source offset where the text's first `k` bytes end.
    pub fn end(&self, k: usize) -> usize {
        match k.min(self.from.len()) {
            0 => self.from.first().map_or(self.base, |f| f.0),
            k => self.from[k - 1].1,
        }
    }

    /// The source of the text's bytes `a..b`; for an empty range, where text inserted at `a`
    /// goes (after the text when `a` is its end).
    pub fn span(&self, a: usize, b: usize) -> Range<usize> {
        if b > a {
            self.at(a)..self.end(b)
        } else if a >= self.from.len() {
            self.end(a)..self.end(a)
        } else {
            self.at(a)..self.at(a)
        }
    }
}

fn lint_hyphens(text: &str, map: &TextMap, sink: &mut Sink) {
    let n = text.len();
    // `<sp>-</sp>` is Gregorio's zero-width hyphen and is fine.
    if text.starts_with('-') {
        sink.warn(
            map.span(0, 1),
            "gabc::hyphen-in-syllable",
            "a hyphen at the start of a syllable prints in addition to the hyphen the engine draws; remove it",
        );
        sink.fix(Fix::new(map.span(0, 1), "", "Remove the hyphen"));
    }
    if text.ends_with('-') && !text.ends_with("<sp>-</sp>") && !text.ends_with("$-") {
        sink.warn(
            map.span(n - 1, n),
            "gabc::hyphen-in-syllable",
            "a hyphen at the end of a syllable prints in addition to the hyphen the engine draws; remove it",
        );
        sink.fix(Fix::new(map.span(n - 1, n), "", "Remove the hyphen"));
    }
}

#[cfg(test)]
mod tests;
