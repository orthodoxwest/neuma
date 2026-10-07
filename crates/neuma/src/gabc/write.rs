//! Writes a [`Score`] back out as GABC.

use std::fmt::Write as _;

use crate::diag::Sink;

use crate::score::{
    AlterationKind, BarKind, ClefKind, CustosRule, Figure, Liquescent, Lyric, NoteShape, Placement, Score, Space, TextStyle,
    position_letter,
};

/// GABC for `score`. Parsing the result gives back an equal score, apart from source spans.
/// GABC can't express a `%` in a header value (it starts a comment), so one is lost, nor a
/// value whose lines would end it early, which is written on one line. Nor can it express an
/// augmented liquescent followed by notes that spell `nlba>`, which read back as the tag, or
/// two spaces or bars in a row that read back as one (`/` then `/0`, `:` then `:`).
pub fn to_gabc(score: &Score) -> String {
    let mut out = String::new();
    for (name, value) in &score.header.fields {
        out.push_str(&header_field(name, value));
    }
    out.push_str("%%\n");
    let mut style = TextStyle::REGULAR;
    let mut nlba = false;
    let mut eu = false;
    for (i, syl) in score.syllables.iter().enumerate() {
        // `<nlba>` spans the syllables that may not break apart, so it opens before the
        // syllable ahead of the first forbidden break and closes once the run ends.
        // The space between words goes before `</nlba>`: after it, the space would be part
        // of the syllable's text and the words would run together.
        if i > 0 && syl.word_start {
            out.push(' ');
        }
        if nlba && !syl.no_break_before && !syl.no_break_within {
            out.push_str("</nlba>");
            nlba = false;
        }
        if !nlba && (syl.no_break_within || score.syllables.get(i + 1).is_some_and(|n| n.no_break_before)) {
            out.push_str("<nlba>");
            nlba = true;
        }
        if syl.euouae != eu {
            out.push_str(if syl.euouae { "<eu>" } else { "</eu>" });
            eu = syl.euouae;
        }
        write_lyric(&mut out, &syl.text, &mut style);
        out.push('(');
        for f in &syl.notation {
            write_figure(&mut out, f);
        }
        out.push(')');
    }
    close_style(&mut out, &mut style, TextStyle::REGULAR);
    if eu {
        out.push_str("</eu>");
    }
    if nlba {
        out.push_str("</nlba>");
    }
    out.push('\n');
    out
}

fn close_style(out: &mut String, cur: &mut TextStyle, want: TextStyle) {
    // Close in reverse of the opening order below, then open.
    if cur.underline && !want.underline {
        out.push_str("</ul>");
    }
    if cur.small_caps && !want.small_caps {
        out.push_str("</sc>");
    }
    if cur.rubric && !want.rubric {
        out.push_str("</c>");
    }
    if cur.bold && !want.bold {
        out.push_str("</b>");
    }
    if cur.italic && !want.italic {
        out.push_str("</i>");
    }
    if want.italic && !cur.italic {
        out.push_str("<i>");
    }
    if want.bold && !cur.bold {
        out.push_str("<b>");
    }
    if want.rubric && !cur.rubric {
        out.push_str("<c>");
    }
    if want.small_caps && !cur.small_caps {
        out.push_str("<sc>");
    }
    if want.underline && !cur.underline {
        out.push_str("<ul>");
    }
    *cur = want;
}

fn special_source(c: char) -> Option<&'static str> {
    Some(match c {
        '℣' => "V/",
        '℟' => "R/",
        '†' => "+",
        'æ' => "ae",
        'œ' => "oe",
        'ǽ' => "'ae",
        _ => return None,
    })
}

/// Whether `text` is made only of characters that `<sp>` produces: a bare `A` counts only
/// as part of `A\u{0336}`, and an acute only after `œ`.
/// Whether some and whether all of `text` is specials that are red by themselves.
fn red_specials(text: &str) -> (bool, bool) {
    let (mut some, mut all) = (false, !text.is_empty());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let red = matches!(c, '℣' | '℟' | '*' | '†') || (c == 'A' && chars.next_if_eq(&'\u{0336}').is_some());
        some |= red;
        all &= red;
    }
    (some, all)
}

fn all_special(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let ok = match c {
            'A' => chars.next_if_eq(&'\u{0336}').is_some(),
            'œ' => chars.next_if_eq(&'\u{0301}').is_some() || special_source(c).is_some(),
            '*' => true,
            _ => special_source(c).is_some(),
        };
        if !ok {
            return false;
        }
    }
    true
}

fn write_lyric(out: &mut String, lyric: &Lyric, style: &mut TextStyle) {
    let mut index = 0;
    for run in &lyric.runs {
        // Special characters were parsed as rubric consonant runs; write them back as `<sp>`.
        // `<sp>V/</sp>`, `R/`, `A/`, `*` and `+` are red without `<c>`, so an elided `*` that
        // isn't red is written as text, not as `<sp>*</sp>`.
        let mut run_style = run.style;
        let (some_red, all_red) = red_specials(&run.text);
        let is_special = run.consonant && all_special(&run.text) && (run.style.rubric || !some_red);
        if is_special && all_red {
            run_style.rubric = false;
        }
        if run.consonant && !is_special {
            run_style.italic = false;
        }
        close_style(out, style, run_style);
        if run.consonant && !is_special {
            out.push_str("<e>");
        }
        let mut chars = run.text.chars().peekable();
        while let Some(c) = chars.next() {
            if lyric.center.as_ref().is_some_and(|r| r.start == index) {
                out.push('{');
            }
            if is_special {
                // Two characters from one `<sp>`: a centering brace closes after both.
                let pair = match (c, chars.peek()) {
                    ('A', Some('\u{0336}')) => Some("<sp>A/</sp>"),
                    ('œ', Some('\u{0301}')) => Some("<sp>'oe</sp>"),
                    _ => None,
                };
                if let Some(sp) = pair {
                    chars.next();
                    out.push_str(sp);
                    index += 2;
                    if lyric.center.as_ref().is_some_and(|r| r.end == index || r.end == index - 1) {
                        out.push('}');
                    }
                    continue;
                }
                if c == '*' {
                    out.push_str("<sp>*</sp>");
                } else if let Some(s) = special_source(c) {
                    let _ = write!(out, "<sp>{s}</sp>");
                }
            } else {
                match c {
                    '(' | ')' | '$' | '{' | '}' | '[' | ']' | '<' | '>' | '%' | '~' => {
                        out.push('$');
                        out.push(c);
                    }
                    _ => out.push(c),
                }
            }
            index += 1;
            if lyric.center.as_ref().is_some_and(|r| r.end == index) {
                out.push('}');
            }
        }
        if run.consonant && !is_special {
            out.push_str("</e>");
        }
    }
}

/// One header field as GABC. A value with a `;` or a line break ends with `;;`; when its lines
/// would read back differently (a line that ends in `;` or looks like the next field), its
/// line breaks become spaces.
fn header_field(name: &str, value: &str) -> String {
    let reads_back = |text: &str| {
        let (header, _) = super::parse_header(&format!("{text}%%\n"), &mut Sink::default());
        header.fields.len() == 1 && header.fields[0].0 == name && header.fields[0].1 == value
    };
    if !value.contains(['\n', ';']) {
        return format!("{name}: {value};\n");
    }
    let text = format!("{name}: {value};;\n");
    if reads_back(&text) {
        return text;
    }
    // As the value reads back, so that writing it again gives the same text.
    let flat = value.replace(['\r', '\n'], " ");
    let end = if flat.contains(';') { ";;" } else { ";" };
    format!("{name}: {flat}{end}\n")
}

fn placement_digit(p: Placement) -> &'static str {
    match p {
        Placement::Auto => "",
        Placement::Below => "0",
        Placement::Above => "1",
    }
}

fn letter(position: i8) -> char {
    position_letter(position).unwrap_or('g')
}

fn write_figure(out: &mut String, f: &Figure) {
    match f {
        Figure::Clef(c) => {
            out.push(if c.kind == ClefKind::Do { 'c' } else { 'f' });
            if c.flat {
                out.push('b');
            }
            let _ = write!(out, "{}", c.line);
        }
        Figure::Note(n) => {
            if n.initio_debilis {
                out.push('-');
            }
            let l = letter(n.position);
            out.push(if n.shape == NoteShape::Inclinatum {
                l.to_ascii_uppercase()
            } else {
                l
            });
            if let Some(lean) = n.lean {
                let _ = write!(out, "{lean}");
            }
            match n.shape {
                NoteShape::Virga => out.push('v'),
                NoteShape::VirgaReversa => out.push('V'),
                NoteShape::Stropha => out.push('s'),
                NoteShape::Quilisma => out.push(if n.quadratum { 'W' } else { 'w' }),
                NoteShape::Oriscus | NoteShape::OriscusScapus => {
                    out.push(if n.shape == NoteShape::Oriscus { 'o' } else { 'O' });
                    if let Some(up) = n.orientation {
                        out.push(if up { '1' } else { '0' });
                    }
                }
                NoteShape::Punctum | NoteShape::Inclinatum => {}
            }
            if n.quadratum && n.shape != NoteShape::Quilisma {
                out.push('q');
            }
            if n.cavum {
                out.push('r');
            }
            if let Some(s) = n.above_sign {
                let _ = write!(out, "r{s}");
            }
            match n.liquescent {
                Liquescent::None => {}
                Liquescent::Deminutus => out.push('~'),
                Liquescent::Augmented => out.push('<'),
                Liquescent::Diminished => out.push('>'),
            }
            if let Some(e) = n.episema {
                out.push('_');
                out.push_str(placement_digit(e.placement));
                if e.no_bridge {
                    out.push('2');
                }
                if let Some(s) = e.small {
                    let _ = write!(out, "{s}");
                }
            }
            if let Some(p) = n.ictus {
                out.push('\'');
                out.push_str(placement_digit(p));
            }
            for k in 0..n.morae {
                out.push('.');
                if k + 1 == n.morae {
                    out.push_str(placement_digit(n.mora_placement));
                }
            }
        }
        Figure::Alteration(a) => {
            out.push(letter(a.position));
            out.push_str(match (a.kind, a.soft) {
                (AlterationKind::Flat, false) => "x",
                (AlterationKind::Flat, true) => "X",
                (AlterationKind::Natural, false) => "y",
                (AlterationKind::Natural, true) => "Y",
                (AlterationKind::Sharp, false) => "#",
                (AlterationKind::Sharp, true) => "##",
            });
            if a.parenthesized {
                out.push('?');
            }
        }
        Figure::Space(s) => match s {
            Space::Zero => out.push('!'),
            Space::Tiny => out.push_str("/!"),
            Space::Half => out.push_str("/0"),
            Space::Small => out.push('/'),
            Space::Medium => out.push_str("//"),
            Space::Large => out.push(' '),
            Space::LargeNoBreak => out.push_str("! "),
            Space::Scaled(f) => {
                let _ = write!(out, "/[{f}]");
            }
        },
        Figure::Bar(b) => {
            let mark = match b.kind {
                BarKind::Virgula => "`",
                BarKind::Minimis => "^",
                BarKind::Minima => ",",
                BarKind::Minor => ";",
                BarKind::Maior => ":",
                BarKind::DottedMaior => ":?",
                BarKind::Finalis => "::",
                BarKind::Dominican(n) => {
                    let _ = write!(out, ";{n}");
                    ""
                }
            };
            out.push_str(mark);
            if b.high {
                out.push('0');
            }
        }
        Figure::Break(b) => {
            out.push(if b.justify { 'z' } else { 'Z' });
            match b.custos {
                CustosRule::Default => {}
                CustosRule::Force => out.push('+'),
                CustosRule::Suppress => out.push('-'),
            }
        }
        Figure::Custos { position, .. } => match position {
            Some(p) => {
                out.push(letter(*p));
                out.push('+');
            }
            None => out.push_str("z0"),
        },
        Figure::NoCustos => out.push_str("[nocustos]"),
    }
}
