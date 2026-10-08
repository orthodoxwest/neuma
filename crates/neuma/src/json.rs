//! JSON for the outputs, written by hand so it adds no dependency and little code to a wasm
//! module: the shapes the browser package returns and the CLI prints (`neuma notes`, `neuma
//! info`). Each writer appends to `out`. Field names are camelCase; offsets named `start`
//! and `end` count UTF-8 bytes and those named `utf16Start` and `utf16End` UTF-16 units.

use std::fmt::Write as _;
use std::ops::Range;

use crate::{
    BarKind, Diagnostic, Element, ElementKind, LineBox, NoteShape, OfficePart, PauseKind, Severity, Summary, Timeline, Utf16Index,
};

/// A JSON string literal for `s`.
#[doc(hidden)]
pub fn string(out: &mut String, s: &str) {
    out.push('"');
    // Most strings need no escapes: copy them whole.
    if !s.bytes().any(|b| b < 0x20 || b == b'"' || b == b'\\') {
        out.push_str(s);
        out.push('"');
        return;
    }
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A finite number with at most three decimals (0 for a non-finite one: JSON has no
/// infinities).
#[doc(hidden)]
pub fn number(out: &mut String, v: f32) {
    if !v.is_finite() {
        out.push('0');
        return;
    }
    let start = out.len();
    crate::decimal::push_fixed(out, v, 3);
    let trimmed = out[start..].trim_end_matches('0').trim_end_matches('.').len();
    out.truncate(start + trimmed);
    if &out[start..] == "-0" {
        out.replace_range(start.., "0");
    }
}

fn integer(out: &mut String, v: i64) {
    if v < 0 {
        out.push('-');
    }
    crate::decimal::push_u64(out, v.unsigned_abs());
}

fn field(out: &mut String, first: &mut bool, name: &str) {
    if !*first {
        out.push(',');
    }
    *first = false;
    string(out, name);
    out.push(':');
}

/// `"start":…,"end":…` in UTF-8 bytes, then `"utf16Start":…,"utf16End":…` in UTF-16 units
/// when the source's index is at hand (fields to go inside an object).
#[doc(hidden)]
pub fn span(out: &mut String, span: &Range<usize>, utf16: Option<&Utf16Index>) {
    named_span(out, ["start", "end", "utf16Start", "utf16End"], span, utf16);
}

/// [`span`] under other names (`sourceStart` and so on).
/// `"sourceStart":…,"sourceEnd":…`, then `"sourceUtf16Start":…,"sourceUtf16End":…` with
/// `utf16`: a note's source, as the timeline names it.
#[doc(hidden)]
pub fn source_span(out: &mut String, span: &Range<usize>, utf16: Option<&Utf16Index>) {
    named_span(out, ["sourceStart", "sourceEnd", "sourceUtf16Start", "sourceUtf16End"], span, utf16);
}

fn named_span(out: &mut String, [start, end, start16, end16]: [&str; 4], span: &Range<usize>, utf16: Option<&Utf16Index>) {
    let _ = write!(out, r#""{start}":{},"{end}":{}"#, span.start, span.end);
    if let Some(idx) = utf16 {
        let r = idx.range_to_utf16(span);
        let _ = write!(out, r#","{start16}":{},"{end16}":{}"#, r.start, r.end);
    }
}

/// Diagnostics: `[{ severity, start, end, utf16Start, utf16End, code, message, fix }]`, where
/// `fix` is `null` or `{ start, end, utf16Start, utf16End, replacement, title }`.
/// `utf16Start` and `utf16End` are there only with `utf16`, the source's index.
pub fn diagnostics(out: &mut String, diags: &[Diagnostic], utf16: Option<&Utf16Index>) {
    out.push('[');
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let severity = match d.severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        let _ = write!(out, r#"{{"severity":"{severity}","#);
        span(out, &d.span, utf16);
        out.push_str(",\"code\":");
        string(out, d.code);
        out.push_str(",\"message\":");
        string(out, &d.message);
        out.push_str(",\"fix\":");
        match &d.fix {
            Some(f) => {
                out.push('{');
                span(out, &f.span, utf16);
                out.push_str(",\"replacement\":");
                string(out, &f.replacement);
                out.push_str(",\"title\":");
                string(out, &f.title);
                out.push('}');
            }
            None => out.push_str("null"),
        }
        out.push('}');
    }
    out.push(']');
}

/// A source map element: `{ kind, index, start, end, utf16Start, utf16End, line, x, y, w, h,
/// cx }`, its box from its top-left corner (`cx` as [`Element::cx`]).
pub fn element(out: &mut String, e: &Element, utf16: &Utf16Index) {
    let kind = match e.kind {
        ElementKind::Note => "note",
        ElementKind::Bar => "bar",
        ElementKind::Syllable => "syllable",
    };
    let _ = write!(out, r#"{{"kind":"{kind}","index":{},"#, e.index);
    span(out, &e.span, Some(utf16));
    let _ = write!(out, r#","line":{},"x":"#, e.line);
    number(out, e.x);
    out.push_str(",\"y\":");
    number(out, e.y);
    out.push_str(",\"w\":");
    number(out, e.w);
    out.push_str(",\"h\":");
    number(out, e.h);
    out.push_str(",\"cx\":");
    number(out, e.cx);
    out.push('}');
}

fn bar_name(b: BarKind) -> &'static str {
    match b {
        BarKind::Virgula => "virgula",
        BarKind::Minimis => "minimis",
        BarKind::Quarter => "quarter",
        BarKind::Half => "half",
        BarKind::Full => "full",
        BarKind::DottedFull => "dotted-full",
        BarKind::Double => "double",
        BarKind::Dominican(_) => "dominican",
    }
}

fn shape_name(s: NoteShape) -> &'static str {
    match s {
        NoteShape::Punctum => "punctum",
        NoteShape::Inclinatum => "inclinatum",
        NoteShape::Virga => "virga",
        NoteShape::VirgaReversa => "virga-reversa",
        NoteShape::Quilisma => "quilisma",
        NoteShape::Oriscus => "oriscus",
        NoteShape::OriscusScapus => "oriscus-scapus",
        NoteShape::Stropha => "stropha",
    }
}

fn pause_name(k: PauseKind) -> &'static str {
    match k {
        PauseKind::Bar(b) => bar_name(b),
        PauseKind::Mediant => "mediant",
        PauseKind::Flex => "flex",
    }
}

fn line(out: &mut String, l: &LineBox) {
    out.push_str("{\"top\":");
    number(out, l.top);
    out.push_str(",\"bottom\":");
    number(out, l.bottom);
    out.push_str(",\"staff\":");
    number(out, l.staff);
    out.push_str(",\"baseline\":");
    number(out, l.baseline);
    out.push('}');
}

/// `{ notes, pauses, lines, duration }`: the playback timeline. Each note gives its notehead's
/// center as `cx` and `cy`, and its source as `sourceStart` and `sourceEnd` (and, with
/// `utf16`, the source's index, `sourceUtf16Start` and `sourceUtf16End`). Each pause gives
/// its bar as `bar`: `{ index, line, x, y, w, h, cx }` as a source map element's box, or
/// null.
pub fn timeline(out: &mut String, map: &Timeline, utf16: Option<&Utf16Index>) {
    out.push_str("{\"notes\":[");
    for (i, n) in map.notes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let mut first = true;
        out.push('{');
        field(out, &mut first, "id");
        integer(out, n.id as i64);
        for (name, v) in [
            ("cx", n.cx),
            ("cy", n.cy),
            ("w", n.w),
            ("h", n.h),
            ("start", n.start),
            ("duration", n.duration),
        ] {
            field(out, &mut first, name);
            number(out, v);
        }
        for (name, v) in [
            ("line", n.line as i64),
            ("syllable", n.syllable as i64),
            ("word", n.word as i64),
            ("verse", n.verse as i64),
            ("half", n.half as i64),
            ("staffPosition", n.staff_position as i64),
            ("degree", n.degree as i64),
            ("semitones", n.semitones as i64),
        ] {
            field(out, &mut first, name);
            integer(out, v);
        }
        out.push(',');
        source_span(out, &n.span, utf16);
        field(out, &mut first, "syllableText");
        string(out, &n.syllable_text);
        field(out, &mut first, "vowel");
        match n.vowel {
            Some(c) => string(out, c.encode_utf8(&mut [0; 4])),
            None => out.push_str("null"),
        }
        field(out, &mut first, "shape");
        string(out, shape_name(n.shape));
        for (name, v) in [
            ("liquescent", n.liquescent),
            ("accent", n.accent),
            ("newSyllable", n.new_syllable),
            ("recitation", n.recitation),
        ] {
            field(out, &mut first, name);
            out.push_str(if v { "true" } else { "false" });
        }
        out.push('}');
    }
    out.push_str("],\"pauses\":[");
    for (i, p) in map.pauses.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"beforeNote\":{},\"kind\":\"{}\",\"duration\":",
            p.before_note,
            pause_name(p.kind)
        );
        number(out, p.duration);
        out.push_str(",\"start\":");
        number(out, p.start);
        out.push_str(",\"bar\":");
        match &p.bar {
            Some(b) => {
                let _ = write!(out, "{{\"index\":{},\"line\":{}", b.index, b.line);
                for (name, v) in [("x", b.x), ("y", b.y), ("w", b.w), ("h", b.h), ("cx", b.cx)] {
                    let _ = write!(out, ",\"{name}\":");
                    number(out, v);
                }
                out.push('}');
            }
            None => out.push_str("null"),
        }
        out.push('}');
    }
    out.push_str("],\"lines\":[");
    for (i, l) in map.lines.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        line(out, l);
    }
    out.push_str("],\"duration\":");
    number(out, map.duration);
    out.push('}');
}

fn opt_string(out: &mut String, v: Option<&str>) {
    match v {
        Some(s) => string(out, s),
        None => out.push_str("null"),
    }
}

fn office_part_name(k: OfficePart) -> &'static str {
    match k {
        OfficePart::Antiphon => "antiphon",
        OfficePart::Introit => "introit",
        OfficePart::Gradual => "gradual",
        OfficePart::Alleluia => "alleluia",
        OfficePart::Tract => "tract",
        OfficePart::Sequence => "sequence",
        OfficePart::Offertory => "offertory",
        OfficePart::Communion => "communion",
        OfficePart::Hymn => "hymn",
        OfficePart::Responsory => "responsory",
        OfficePart::ShortResponsory => "short-responsory",
        OfficePart::Versicle => "versicle",
        OfficePart::Chapter => "chapter",
        OfficePart::Collect => "collect",
        OfficePart::Psalm => "psalm",
        OfficePart::Canticle => "canticle",
        OfficePart::Kyrie => "kyrie",
        OfficePart::Gloria => "gloria",
        OfficePart::Credo => "credo",
        OfficePart::Sanctus => "sanctus",
        OfficePart::Agnus => "agnus",
        OfficePart::Other => "other",
    }
}

/// A score's library entry ([`Summary`]).
pub fn summary(out: &mut String, s: &Summary) {
    let mut first = true;
    out.push('{');
    for (name, v) in [
        ("name", &s.name),
        ("officePart", &s.office_part),
        ("occasion", &s.occasion),
        ("book", &s.book),
        ("language", &s.language),
        ("transcriber", &s.transcriber),
        ("gabcCopyright", &s.gabc_copyright),
        ("scoreCopyright", &s.score_copyright),
        ("commentary", &s.commentary),
    ] {
        field(out, &mut first, name);
        opt_string(out, v.as_deref());
    }
    field(out, &mut first, "kind");
    opt_string(out, s.kind.map(office_part_name));
    field(out, &mut first, "mode");
    match &s.mode {
        Some(m) => {
            out.push_str("{\"number\":");
            match m.number {
                Some(n) => {
                    let _ = write!(out, "{n}");
                }
                None => out.push_str("null"),
            }
            out.push_str(",\"name\":");
            string(out, &m.name);
            out.push_str(",\"modifier\":");
            opt_string(out, m.modifier.as_deref());
            out.push_str(",\"differentia\":");
            opt_string(out, m.differentia.as_deref());
            out.push('}');
        }
        None => out.push_str("null"),
    }
    field(out, &mut first, "annotations");
    out.push('[');
    for (i, a) in s.annotations.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        string(out, a);
    }
    out.push(']');
    field(out, &mut first, "otherHeaders");
    out.push('[');
    for (i, (n, v)) in s.other_headers.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        string(out, n);
        out.push_str(",\"value\":");
        string(out, v);
        out.push('}');
    }
    out.push(']');
    field(out, &mut first, "incipit");
    string(out, &s.incipit);
    field(out, &mut first, "text");
    string(out, &s.text);
    for (name, v) in [("lowest", s.lowest), ("highest", s.highest), ("finalPitch", s.final_pitch)] {
        field(out, &mut first, name);
        match v {
            Some(p) => integer(out, i64::from(p)),
            None => out.push_str("null"),
        }
    }
    let _ = write!(
        out,
        ",\"notes\":{},\"syllables\":{},\"words\":{},\"duration\":",
        s.notes, s.syllables, s.words
    );
    number(out, s.duration);
    out.push('}');
}

/// A layout: `{ width, height, timeline }`, the timeline only if given (see [`timeline`]).
pub fn layout(out: &mut String, size: (f32, f32), timeline: Option<&Timeline>, utf16: Option<&Utf16Index>) {
    out.push_str("{\"width\":");
    number(out, size.0);
    out.push_str(",\"height\":");
    number(out, size.1);
    if let Some(map) = timeline {
        out.reserve(map.notes.len() * 460 + 1024);
        out.push_str(",\"timeline\":");
        self::timeline(out, map, utf16);
    }
    out.push('}');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_numbers() {
        let mut s = String::new();
        string(&mut s, "a\"b\\\u{1}");
        assert_eq!(s, r#""a\"b\\\u0001""#);
        for (v, want) in [(1.0, "1"), (1.25, "1.25"), (-0.0001, "0"), (f32::NAN, "0"), (2.1234, "2.123")] {
            let mut s = String::new();
            number(&mut s, v);
            assert_eq!(s, want);
        }
    }
}
