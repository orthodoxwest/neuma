//! The JSON the browser package returns, written by hand so the module stays small. The CLI
//! prints the same shapes (`neuma notes`), with the same note ids.

use std::fmt::Write as _;

use neuma::score::{BarKind, NoteShape};
use std::ops::Range;

use neuma::{Diagnostic, Element, ElementKind, LineBox, NoteMap, OfficePart, PauseKind, Severity, Summary, Utf16Index};

/// A JSON string literal for `s`.
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

/// A finite number with at most three decimals; JSON has no infinities.
pub fn number(out: &mut String, v: f32) {
    if !v.is_finite() {
        out.push('0');
        return;
    }
    let start = out.len();
    neuma::decimal::push_fixed(out, v, 3);
    let trimmed = out[start..].trim_end_matches('0').trim_end_matches('.').len();
    out.truncate(start + trimmed);
    if &out[start..] == "-0" {
        out.replace_range(start.., "0");
    }
}

pub fn integer(out: &mut String, v: i64) {
    if v < 0 {
        out.push('-');
    }
    neuma::decimal::push_u64(out, v.unsigned_abs());
}

fn field(out: &mut String, first: &mut bool, name: &str) {
    if !*first {
        out.push(',');
    }
    *first = false;
    string(out, name);
    out.push(':');
}

/// A psalm setting: `{ gabc, notes: [{ verse, number, part, role, start, end }], diagnostics }`.
/// `notes[i]` describes note `i` of the engraved score.
pub fn setting(out: &mut String, s: &neuma_tones::Setting, text: Option<&Utf16Index>) {
    out.push_str("{\"gabc\":");
    string(out, &s.gabc);
    out.push_str(",\"notes\":[");
    for (i, n) in s.notes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let part = part_name(n.part);
        let role = match n.role {
            neuma_tones::Role::Intonation => "intonation",
            neuma_tones::Role::Tenor => "tenor",
            neuma_tones::Role::Preparatory => "preparatory",
            neuma_tones::Role::Accent => "accent",
            neuma_tones::Role::Ending => "ending",
        };
        let _ = write!(out, r#"{{"verse":{},"number":"#, n.verse);
        match n.number {
            Some(v) => {
                let _ = write!(out, "{v}");
            }
            None => out.push_str("null"),
        }
        let _ = write!(
            out,
            r#","part":"{part}","role":"{role}","start":{},"end":{}}}"#,
            n.source.start, n.source.end
        );
    }
    out.push_str("],\"diagnostics\":");
    diagnostics(out, &s.diagnostics, text);
    out.push('}');
}

fn part_name(k: neuma_tones::PartKind) -> &'static str {
    match k {
        neuma_tones::PartKind::Flex => "flex",
        neuma_tones::PartKind::Mediant => "mediant",
        neuma_tones::PartKind::Termination => "termination",
    }
}

/// A pointing: `{ text, halves: [{ verse, part, confidence, kept }], diagnostics }`.
pub fn pointing(out: &mut String, p: &neuma_tones::Pointing, text: Option<&Utf16Index>) {
    out.push_str("{\"text\":");
    string(out, &p.text());
    out.push_str(",\"halves\":[");
    for (i, h) in p.halves.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, r#"{{"verse":{},"part":"{}","confidence":"#, h.verse, part_name(h.part));
        number(out, h.confidence);
        let _ = write!(out, r#","kept":{}}}"#, h.kept);
    }
    out.push_str("],\"diagnostics\":");
    diagnostics(out, &p.pointed.diagnostics, text);
    out.push('}');
}

/// `"start":…,"end":…` in UTF-8 bytes, then `"from":…,"to":…` in UTF-16 units when the
/// source's index is at hand.
fn span(out: &mut String, span: &Range<usize>, utf16: Option<&Utf16Index>) {
    let _ = write!(out, r#""start":{},"end":{}"#, span.start, span.end);
    if let Some(idx) = utf16 {
        let r = idx.range_to_utf16(span);
        let _ = write!(out, r#","from":{},"to":{}"#, r.start, r.end);
    }
}

/// Diagnostics: `[{ severity, start, end, from, to, code, message, fix }]`, where `fix` is
/// `null` or `{ start, end, from, to, insert, title }`. `from` and `to` (UTF-16 units) are
/// there only with `utf16`.
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
                out.push_str(",\"insert\":");
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

/// A source map element: `{ kind, index, start, end, from, to, line, x, y, w, h }`.
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
    out.push('}');
}

fn bar_name(b: BarKind) -> &'static str {
    match b {
        BarKind::Virgula => "virgula",
        BarKind::Minimis => "minimis",
        BarKind::Minima => "quarter",
        BarKind::Minor => "half",
        BarKind::Maior => "full",
        BarKind::DottedMaior => "dotted-full",
        BarKind::Finalis => "double",
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

pub fn pause_name(k: PauseKind) -> &'static str {
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

/// `{ notes, pauses, lines, duration }`: the playback timeline.
pub fn note_map(out: &mut String, map: &NoteMap) {
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
            ("x", n.x),
            ("y", n.y),
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
            ("spanStart", n.span.start as i64),
            ("spanEnd", n.span.end as i64),
        ] {
            field(out, &mut first, name);
            integer(out, v);
        }
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
            ("quilisma", n.quilisma),
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
            "{{\"beforeNote\":{},\"kind\":\"{}\",\"weight\":",
            p.before_note,
            pause_name(p.kind)
        );
        number(out, p.weight);
        out.push_str(",\"start\":");
        number(out, p.start);
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

pub fn office_part_name(k: OfficePart) -> &'static str {
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

/// A score's catalogue entry (`neuma::Summary`).
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
    field(out, &mut first, "range");
    match s.range {
        Some((lo, hi)) => {
            let _ = write!(out, "[{lo},{hi}]");
        }
        None => out.push_str("null"),
    }
    field(out, &mut first, "finalPitch");
    match s.final_pitch {
        Some(p) => {
            let _ = write!(out, "{p}");
        }
        None => out.push_str("null"),
    }
    let _ = write!(
        out,
        ",\"notes\":{},\"syllables\":{},\"words\":{},\"duration\":",
        s.notes, s.syllables, s.words
    );
    number(out, s.duration);
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
