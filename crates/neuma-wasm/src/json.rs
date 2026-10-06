//! The JSON the browser package returns, written by hand so the module stays small. The CLI
//! prints the same shapes (`neuma notes`), with the same note ids.

use std::fmt::Write as _;

use neuma::score::{BarKind, NoteShape};
use neuma::{Diagnostic, LineBox, NoteMap, OfficePart, PauseKind, Severity, Summary};

/// A JSON string literal for `s`.
pub fn string(out: &mut String, s: &str) {
    out.push('"');
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
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    out.push_str(if s == "-0" { "0" } else { s });
}

fn field(out: &mut String, first: &mut bool, name: &str) {
    if !*first {
        out.push(',');
    }
    *first = false;
    string(out, name);
    out.push(':');
}

pub fn diagnostics(out: &mut String, diags: &[Diagnostic]) {
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
        let _ = write!(
            out,
            r#"{{"severity":"{severity}","start":{},"end":{},"code":"#,
            d.span.start, d.span.end
        );
        string(out, d.code);
        out.push_str(",\"message\":");
        string(out, &d.message);
        out.push('}');
    }
    out.push(']');
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
        let _ = write!(out, "{}", n.id);
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
            let _ = write!(out, "{v}");
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
