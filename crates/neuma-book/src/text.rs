//! Running text: a small inline markup, and paragraphs broken into lines.

use crate::font::Fonts;
use crate::page::{Color, Op};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub italic: bool,
    pub bold: bool,
    pub small_caps: bool,
    pub red: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

/// Marks always printed red: the mediant star, flex dagger, crosses and the versicle and
/// response signs.
pub fn red_mark(c: char) -> bool {
    matches!(c, '*' | '†' | '‡' | '✠' | '℣' | '℟' | '+')
}

/// Reads the inline markup of text pieces: `<i>`, `<b>`, `<sc>` and `<r>` (red; `<c>` as in
/// GABC) with their closing tags, and `V/` and `R/` for the versicle and response signs.
/// Red marks ([`red_mark`]) turn red on their own.
pub fn markup(src: &str, base: Style, fonts: &Fonts) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut st = base;
    let push = |out: &mut Vec<Span>, s: &str, style: Style| {
        if s.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(l) if l.style == style => l.text.push_str(s),
            _ => out.push(Span {
                text: s.to_string(),
                style,
            }),
        }
    };
    let mut rest = src;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
        {
            let tag = &rest[1..end];
            let (name, on) = match tag.strip_prefix('/') {
                Some(n) => (n, false),
                None => (tag, true),
            };
            // A tag can't turn off what the base style turns on.
            let known = match name {
                "i" => Some((&mut st.italic, base.italic)),
                "b" => Some((&mut st.bold, base.bold)),
                "sc" => Some((&mut st.small_caps, base.small_caps)),
                "r" | "c" => Some((&mut st.red, base.red)),
                _ => None,
            };
            if let Some((field, base_on)) = known {
                *field = on || base_on;
                rest = &rest[end + 1..];
                continue;
            }
        }
        let mut matched_sign = false;
        for (sign, glyph, fallback) in [("V/", "℣", "V."), ("R/", "℟", "R.")] {
            if let Some(r) = rest.strip_prefix(sign) {
                let text = if fonts.has_char(glyph.chars().next().unwrap_or(' ')) {
                    glyph
                } else {
                    fallback
                };
                push(&mut out, text, Style { red: true, ..st });
                rest = r;
                matched_sign = true;
            }
        }
        if matched_sign {
            continue;
        }
        let Some(c) = rest.chars().next() else { break };
        let mut b = [0u8; 4];
        let style = if red_mark(c) { Style { red: true, ..st } } else { st };
        push(&mut out, c.encode_utf8(&mut b), style);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Justify,
    Left,
    Center,
}

#[derive(Clone, Debug)]
pub struct Para {
    pub spans: Vec<Span>,
    pub size: f32,
    pub leading: f32,
    pub align: Align,
    /// Left indent of every line.
    pub indent: f32,
    /// Extra indent of the first line (after `indent`).
    pub first: f32,
    pub dropcap: bool,
}

/// One line of a paragraph: ops relative to the line's top-left, and its height.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub ops: Vec<Op>,
    pub height: f32,
    /// The baseline, from the line's top.
    pub baseline: f32,
}

struct Word {
    pieces: Vec<(String, Style)>,
    width: f32,
}

fn words(spans: &[Span], fonts: &Fonts, size: f32) -> Vec<Word> {
    let mut out = Vec::new();
    let mut cur: Vec<(String, Style)> = Vec::new();
    let flush = |cur: &mut Vec<(String, Style)>, out: &mut Vec<Word>| {
        if cur.is_empty() {
            return;
        }
        let pieces = std::mem::take(cur);
        let width = pieces
            .iter()
            .map(|(t, s)| fonts.width(t, fonts.resolve(s.italic, s.bold), s.small_caps) * size)
            .sum();
        out.push(Word { pieces, width });
    };
    for span in spans {
        for (i, part) in span.text.split([' ', '\n', '\t']).enumerate() {
            if i > 0 {
                flush(&mut cur, &mut out);
            }
            if !part.is_empty() {
                cur.push((part.to_string(), span.style));
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn color(s: Style) -> Color {
    if s.red { Color::Red } else { Color::Black }
}

/// Breaks a paragraph into lines `width` wide.
pub fn set(fonts: &Fonts, para: &Para, width: f32) -> Vec<Line> {
    let size = para.size;
    let (asc, desc) = fonts.vertical(0);
    let height = size * para.leading;
    let baseline = (height - (asc + desc) * size) / 2.0 + asc * size;
    let mut spans = para.spans.clone();
    // The drop cap: the first letter, two lines tall, beside the first two lines.
    let mut drop_op: Option<Op> = None;
    let mut drop_w = 0.0;
    if para.dropcap
        && let Some(first) = spans.iter_mut().find(|s| !s.text.trim().is_empty())
    {
        let t = first.text.trim_start().to_string();
        if let Some(c) = t.chars().next().filter(|c| c.is_alphanumeric()) {
            let cap = fonts.face(0).map_or(0.66, |f| f.cap_height);
            let dsize = (height + cap * size) / cap;
            let run = fonts.shape(&c.to_string(), fonts.resolve(false, false), false);
            drop_w = run.width * dsize + 0.3 * size;
            drop_op = Some(Op::Text {
                x: para.indent,
                baseline: baseline + height,
                size: dsize,
                run,
                color: color(first.style),
            });
            first.text = t[c.len_utf8()..].to_string();
        }
    }
    let has_drop = drop_op.is_some();
    let words = words(&spans, fonts, size);
    let space = fonts.width(" ", fonts.resolve(false, false), false) * size;
    let mut lines = Vec::new();
    let mut i = 0;
    while i < words.len() || lines.is_empty() {
        let n = lines.len();
        let left = if has_drop && n < 2 {
            para.indent + drop_w
        } else {
            para.indent + if n == 0 { para.first } else { 0.0 }
        };
        let avail = (width - left).max(size);
        let start = i;
        let mut natural = 0.0;
        while i < words.len() {
            let add = if i == start { words[i].width } else { space + words[i].width };
            if i > start && natural + add > avail {
                break;
            }
            natural += add;
            i += 1;
        }
        let last = i >= words.len();
        let gaps = (i - start).saturating_sub(1) as f32;
        let (mut x, gap) = match para.align {
            Align::Center => (left + (avail - natural) / 2.0, space),
            Align::Justify if !last && gaps > 0.0 => {
                let extra = (avail - natural) / gaps;
                (left, if extra > space * 2.5 { space } else { space + extra })
            }
            _ => (left, space),
        };
        let mut ops: Vec<Op> = drop_op.take().into_iter().collect();
        for (k, w) in words[start..i].iter().enumerate() {
            if k > 0 {
                x += gap;
            }
            for (t, s) in &w.pieces {
                let run = fonts.shape(t, fonts.resolve(s.italic, s.bold), s.small_caps);
                let adv = run.width * size;
                ops.push(Op::Text {
                    x,
                    baseline,
                    size,
                    run,
                    color: color(*s),
                });
                x += adv;
            }
        }
        lines.push(Line { ops, height, baseline });
    }
    // A drop cap on a one-line paragraph still needs its second line.
    if has_drop && lines.len() == 1 {
        lines.push(Line {
            ops: Vec::new(),
            height,
            baseline,
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_styles_and_red_marks() {
        let f = Fonts::standard();
        let s = markup("A <i>b</i> c * V/ d", Style::default(), &f);
        let texts: Vec<(&str, bool, bool)> = s.iter().map(|s| (s.text.as_str(), s.style.italic, s.style.red)).collect();
        assert_eq!(
            texts,
            vec![
                ("A ", false, false),
                ("b", true, false),
                (" c ", false, false),
                ("*", false, true),
                (" ", false, false),
                ("V.", false, true),
                (" d", false, false)
            ]
        );
        // Tags can't turn off what the base style turns on.
        let s = markup(
            "<r>x</r>y",
            Style {
                red: true,
                ..Style::default()
            },
            &f,
        );
        assert_eq!(s.len(), 1);
    }

    fn para(text: &str, align: Align, dropcap: bool) -> Para {
        Para {
            spans: vec![Span {
                text: text.into(),
                style: Style::default(),
            }],
            size: 10.0,
            leading: 1.2,
            align,
            indent: 0.0,
            first: 0.0,
            dropcap,
        }
    }

    fn line_right(l: &Line) -> f32 {
        l.ops.iter().map(Op::right).fold(0.0, f32::max)
    }

    #[test]
    fn lines_fit_and_justify() {
        let f = Fonts::standard();
        let text = "Brethren, be sober, be vigilant; because your adversary the devil, as a roaring lion, walketh about, seeking whom he may devour.";
        let lines = set(&f, &para(text, Align::Justify, false), 250.0);
        assert!(lines.len() >= 3);
        for l in &lines[..lines.len() - 1] {
            let r = line_right(l);
            assert!((r - 250.0).abs() < 0.5, "justified line ends at {r}");
        }
        assert!(line_right(&lines[lines.len() - 1]) < 250.0);
    }

    #[test]
    fn dropcap_indents_two_lines() {
        let f = Fonts::standard();
        let text = "Brethren, be sober, be vigilant; because your adversary the devil, as a roaring lion, walketh about.";
        let lines = set(&f, &para(text, Align::Justify, true), 150.0);
        let big = &lines[0].ops[0];
        let Op::Text { size, run, .. } = big else { panic!() };
        assert!(*size > 20.0);
        assert_eq!(run.glyphs[0].text, "B");
        let left = |l: &Line| {
            l.ops
                .iter()
                .filter_map(|o| match o {
                    Op::Text { x, size, .. } if *size < 11.0 => Some(*x),
                    _ => None,
                })
                .fold(f32::MAX, f32::min)
        };
        assert!(left(&lines[0]) > 10.0);
        assert!(left(&lines[1]) > 10.0);
        assert_eq!(left(&lines[2]), 0.0);
        let short = set(&f, &para("Amen.", Align::Left, true), 150.0);
        assert_eq!(short.len(), 2);
    }

    #[test]
    fn centered() {
        let f = Fonts::standard();
        let lines = set(&f, &para("Compline", Align::Center, false), 200.0);
        let Op::Text { x, .. } = &lines[0].ops[0] else { panic!() };
        assert!(*x > 50.0);
    }
}
