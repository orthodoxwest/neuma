//! Turns pieces into blocks: one per score line or text line, each with its height and
//! whether it must stay on the page with the block after it.

use neuma::display::{DisplayList, Item, TextRole};
use neuma::{Diagnostic, Initial, LastLine, LayoutOptions, Severity, StyleOptions};
use neuma_tones::{Intone, Options, Tone};

use crate::book::{Book, Piece, Psalm, PsalmSet, Settings, Source};
use crate::font::Fonts;
use crate::page::{Color, Op};
use crate::text::{self, Align, Para, Span, Style};

/// The Gloria Patri, as the Book of Common Prayer prints it after the psalms.
pub const GLORIA: &str = "Glory be to the Father, and to the Son, * and to the Holy Ghost;\n\
                          As it was in the beginning, is now, and ever shall be, * world without end. Amen.";

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Content,
    /// Starts a new page.
    Break,
    /// Sets the running header.
    Header(String),
    /// A title: its page has no running header.
    Title,
}

/// A vertical slice of content: ops relative to its top-left corner at the text block's
/// left edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub height: f32,
    /// Space above it, dropped at the top of a page.
    pub space_before: f32,
    pub ops: Vec<Op>,
    /// Stays on the same page as the next block.
    pub keep_with_next: bool,
    pub kind: Kind,
}

impl Block {
    fn marker(kind: Kind) -> Block {
        Block {
            height: 0.0,
            space_before: 0.0,
            ops: Vec::new(),
            keep_with_next: false,
            kind,
        }
    }
}

/// A problem found while setting a piece.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    /// The piece's index in the book.
    pub piece: usize,
    pub diagnostic: Diagnostic,
}

/// Sizes derived from the settings.
struct Metrics {
    /// Points per staff space.
    scale: f32,
    /// Lyric size in staff spaces.
    lyric: f32,
    width: f32,
    /// Spread the last line across the width (for the tone, which has no words to space it).
    justify_last: bool,
}

impl Metrics {
    fn new(s: &Settings) -> Metrics {
        // The staff's four lines span six staff spaces.
        let scale = s.staff_size / 6.0;
        Metrics {
            scale,
            lyric: s.lyric_size.unwrap_or(s.text_size) / scale,
            width: s.width - s.margins[1] - s.margins[3],
            justify_last: false,
        }
    }
}

/// Sets every piece of the book.
pub fn blocks(book: &Book, fonts: &Fonts) -> (Vec<Block>, Vec<Problem>) {
    let s = &book.settings;
    let m = Metrics::new(s);
    let mut out: Vec<Block> = Vec::new();
    let mut problems = Vec::new();
    let size = s.text_size;
    for (i, piece) in book.pieces.iter().enumerate() {
        let mut diags = Vec::new();
        let start = out.len();
        match piece {
            Piece::Title(t) => {
                // A title also becomes the running header, until a `header:` changes it.
                out.push(Block::marker(Kind::Header(t.clone())));
                out.push(Block {
                    keep_with_next: true,
                    ..Block::marker(Kind::Title)
                });
                let p = para(fonts, t, Style::default(), size * 2.0, Align::Center, 1.15);
                push_lines(&mut out, fonts, &p, m.width, size * 1.5, 2);
                if let Some(b) = out.last_mut() {
                    b.keep_with_next = true;
                }
                out.push(spacer(size * 0.8));
            }
            Piece::Heading(t) => {
                let p = para(
                    fonts,
                    t,
                    Style {
                        small_caps: false,
                        red: true,
                        ..Style::default()
                    },
                    size * 1.35,
                    Align::Center,
                    1.2,
                );
                push_lines(&mut out, fonts, &p, m.width, size * 1.4, 2);
                if let Some(b) = out.last_mut() {
                    b.keep_with_next = true;
                }
            }
            Piece::Rubric(t) => {
                let p = para(
                    fonts,
                    t,
                    Style {
                        italic: true,
                        red: true,
                        ..Style::default()
                    },
                    size * 0.92,
                    Align::Left,
                    s.leading,
                );
                push_lines(&mut out, fonts, &p, m.width, size * 0.5, 2);
                if let Some(b) = out.last_mut() {
                    b.keep_with_next = true;
                }
            }
            Piece::Text {
                text,
                dropcap,
                center,
                lines,
            } => {
                let align = match (center, lines) {
                    (true, _) => Align::Center,
                    (false, true) => Align::Left,
                    (false, false) => Align::Justify,
                };
                for (k, paragraph) in text.split("\n\n").filter(|p| !p.trim().is_empty()).enumerate() {
                    if *lines {
                        // Verse: each line its own, the stanza kept together.
                        let start = out.len();
                        for (j, l) in paragraph.lines().enumerate() {
                            let mut p = para(fonts, l, Style::default(), size, align, s.leading);
                            p.dropcap = *dropcap && k == 0 && j == 0;
                            p.first = 0.0;
                            push_lines(&mut out, fonts, &p, m.width, if j == 0 { size * 0.6 } else { 0.0 }, usize::MAX);
                        }
                        let n = out.len();
                        for b in &mut out[start..n.saturating_sub(1)] {
                            b.keep_with_next = true;
                        }
                    } else {
                        let mut p = para(fonts, paragraph, Style::default(), size, align, s.leading);
                        p.dropcap = *dropcap && k == 0;
                        push_lines(&mut out, fonts, &p, m.width, size * 0.6, 2);
                    }
                }
            }
            Piece::Score { source, initial } => {
                let src = source_text(source);
                let parsed = neuma::parse(src);
                diags.extend(parsed.diagnostics);
                let lines = initial.unwrap_or(s.initial);
                out.extend(score_blocks(&parsed.score, fonts, &m, lines, m.width, 0.0, size * 0.7, &mut diags));
            }
            Piece::Psalm(ps) => out.extend(psalm_blocks(ps, s, fonts, &m, &mut diags)),
            Piece::Header(h) => out.push(Block::marker(Kind::Header(h.clone()))),
            Piece::Break => out.push(Block::marker(Kind::Break)),
            Piece::Space(v) => out.push(spacer(*v)),
        }
        // An antiphon stays with the psalm after it, so it is sung from one page.
        if let (Piece::Score { .. }, Some(Piece::Psalm(_))) = (piece, book.pieces.get(i + 1))
            && let Some(b) = out[start..].last_mut()
        {
            b.keep_with_next = true;
        }
        problems.extend(diags.into_iter().map(|diagnostic| Problem { piece: i, diagnostic }));
    }
    (out, problems)
}

fn source_text(s: &Source) -> &str {
    match s {
        Source::Inline(t) => t,
        // Unresolved paths have no text; Book::resolve reads them first.
        Source::Path(_) => "",
    }
}

fn spacer(h: f32) -> Block {
    Block {
        height: 0.0,
        space_before: h,
        ops: Vec::new(),
        keep_with_next: false,
        kind: Kind::Content,
    }
}

fn para(fonts: &Fonts, t: &str, style: Style, size: f32, align: Align, leading: f32) -> Para {
    Para {
        spans: text::markup(t, style, fonts),
        size,
        leading,
        align,
        indent: 0.0,
        first: 0.0,
        dropcap: false,
    }
}

/// Adds a paragraph's lines as blocks, keeping its first `orphans` lines and last two
/// together, so no line is left alone at the top or bottom of a page.
fn push_lines(out: &mut Vec<Block>, fonts: &Fonts, p: &Para, width: f32, space_before: f32, orphans: usize) {
    let lines = text::set(fonts, p, width);
    let n = lines.len();
    for (k, l) in lines.into_iter().enumerate() {
        out.push(Block {
            height: l.height,
            space_before: if k == 0 { space_before } else { 0.0 },
            ops: l.ops,
            keep_with_next: k + 1 < n && (k + 1 < orphans || k + 2 >= n),
            kind: Kind::Content,
        });
    }
}

/// Engraves a score at `width` and cuts it into one block per staff line. Line 0 keeps with
/// line 1 and the last two lines stay together, so no staff is left alone on a page.
#[allow(clippy::too_many_arguments)]
fn score_blocks(
    score: &neuma::Score,
    fonts: &Fonts,
    m: &Metrics,
    initial: u8,
    width: f32,
    indent: f32,
    space_before: f32,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Block> {
    let style = StyleOptions {
        lyric_size: m.lyric,
        initial: if initial == 0 { Initial::None } else { Initial::Lines(initial) },
        ..StyleOptions::default()
    };
    let eng = score.engrave(fonts, &style);
    diags.extend(eng.diagnostics.iter().cloned());
    let layout = eng.layout(
        width,
        &LayoutOptions {
            scale: m.scale,
            last_line: if m.justify_last { LastLine::Justified } else { LastLine::Ragged },
            ..LayoutOptions::default()
        },
    );
    let dl = layout.display();
    let mut blocks = split_lines(&dl, fonts);
    let n = blocks.len();
    let initial_lines = initial as usize;
    for (k, b) in blocks.iter_mut().enumerate() {
        for op in &mut b.ops {
            op.shift(indent, 0.0);
        }
        b.keep_with_next = k + 1 < n && (k == 0 || k + 1 < initial_lines || k + 2 >= n);
    }
    if let Some(b) = blocks.first_mut() {
        b.space_before = space_before;
    }
    blocks
}

/// Cuts a display list into one block per staff line, by where each item sits.
fn split_lines(dl: &DisplayList, fonts: &Fonts) -> Vec<Block> {
    let lines = &dl.lines;
    if lines.is_empty() {
        return Vec::new();
    }
    let tops: Vec<f32> = lines.iter().enumerate().map(|(i, l)| if i == 0 { 0.0 } else { l.top }).collect();
    let line_of = |y: f32| -> usize {
        let mut best = 0;
        for (i, l) in lines.iter().enumerate() {
            if y >= l.top - 0.01 {
                best = i;
            }
            if y <= l.bottom {
                break;
            }
        }
        best
    };
    let mut ops: Vec<Vec<Op>> = vec![Vec::new(); lines.len()];
    for item in &dl.items {
        let (k, mut op_list) = match item {
            Item::Glyph { glyph, x, y, scale, .. } => (
                line_of(*y),
                vec![Op::Neume {
                    glyph: *glyph,
                    x: *x,
                    y: *y,
                    scale: *scale,
                    color: Color::Black,
                }],
            ),
            Item::Rect { x, y, w, h, .. } => (
                line_of(y + h / 2.0),
                vec![Op::Rect {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                    color: Color::Black,
                }],
            ),
            Item::Text {
                x,
                baseline,
                size,
                runs,
                role,
                ..
            } => {
                let k = match role {
                    TextRole::Initial | TextRole::Annotation => 0,
                    _ => line_of(*baseline),
                };
                let mut out = Vec::new();
                let mut at = *x;
                for r in runs {
                    let run = fonts.shape(&r.text, fonts.resolve(r.style.italic, r.style.bold), r.style.small_caps);
                    let adv = run.width * size;
                    let red = *role == TextRole::Rubric || r.style.rubric;
                    out.push(Op::Text {
                        x: at,
                        baseline: *baseline,
                        size: *size,
                        run,
                        color: if red { Color::Red } else { Color::Black },
                    });
                    at += adv;
                }
                (k, out)
            }
        };
        for op in &mut op_list {
            op.shift(0.0, -tops[k]);
        }
        ops[k].append(&mut op_list);
    }
    ops.into_iter()
        .enumerate()
        .map(|(i, o)| {
            let bottom = tops.get(i + 1).copied().unwrap_or(dl.height);
            Block {
                height: bottom - tops[i],
                space_before: 0.0,
                ops: o,
                keep_with_next: false,
                kind: Kind::Content,
            }
        })
        .collect()
}

fn error(diags: &mut Vec<Diagnostic>, message: String) {
    diags.push(Diagnostic {
        severity: Severity::Error,
        span: 0..0,
        code: "book::psalm",
        message,
    });
}

fn find_tone(ps: &Psalm, diags: &mut Vec<Diagnostic>) -> Option<Tone> {
    let custom = match &ps.tone_file {
        Some(f) => match std::fs::read_to_string(f)
            .map_err(|e| e.to_string())
            .and_then(|s| Tone::parse_all(&s).map_err(|e| e.to_string()))
        {
            Ok(t) => t,
            Err(e) => {
                error(diags, format!("{}: {e}", f.display()));
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    let t = Tone::find(&custom, &ps.tone).or_else(|| Tone::named(&ps.tone)).cloned();
    if t.is_none() {
        error(diags, format!("no psalm tone `{}`; `neuma tones` lists them", ps.tone));
    }
    t
}

/// The tone's intonation, mediant and ending as a small score with no words, the way a
/// pointed psalter prints the tone above the psalm.
pub fn tone_gabc(tone: &Tone) -> String {
    let mut g = format!("({}) ", tone.clef_gabc());
    let half = |c: &neuma_tones::Cadence, intone: bool, g: &mut String| {
        if intone {
            for n in &c.lead {
                g.push_str(&format!("({n}) "));
            }
        }
        g.push_str(&format!("({}) ", c.tenor));
        for s in &c.slots {
            let n = match s {
                neuma_tones::Slot::Fixed(n) | neuma_tones::Slot::Open(n) | neuma_tones::Slot::Accent(n) => n,
            };
            g.push_str(&format!("({n}) "));
        }
    };
    half(&tone.mediant, true, &mut g);
    g.push_str("*(:) ");
    half(&tone.termination, false, &mut g);
    g.push_str("(::)");
    g
}

fn psalm_blocks(ps: &Psalm, s: &Settings, fonts: &Fonts, m: &Metrics, diags: &mut Vec<Diagnostic>) -> Vec<Block> {
    let mut out = Vec::new();
    let Some(tone) = find_tone(ps, diags) else { return out };
    let mut text = source_text(&ps.source).trim_end().to_string();
    if ps.gloria {
        text.push('\n');
        text.push_str(GLORIA);
    }
    let verses: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let set = ps.set.unwrap_or(s.psalms);
    let size = s.text_size;
    let chant_verses = match set {
        PsalmSet::Pointed => 0,
        PsalmSet::First => 1,
        PsalmSet::Chant => verses.len(),
    };
    // Pointed verses: the tone first, as a small score beside its name.
    if chant_verses == 0 && !verses.is_empty() {
        let label = format!("Tone {}", tone.name);
        let style = Style {
            italic: true,
            red: true,
            ..Style::default()
        };
        let lsize = size * 0.85;
        let lw = fonts.width(&label, fonts.resolve(true, false), false) * lsize + size;
        let small = Metrics {
            scale: m.scale * 0.8,
            lyric: m.lyric,
            width: m.width,
            justify_last: true,
        };
        let parsed = neuma::parse(&tone_gabc(&tone));
        let notes = parsed.score.syllables.len() as f32;
        let tone_width = (notes * 3.2 * small.scale).min(m.width - lw);
        let mut tb = score_blocks(&parsed.score, fonts, &small, 0, tone_width, lw, size * 0.5, diags);
        if let Some(b) = tb.first_mut() {
            // The label sits on the staff's middle, between its outer lines.
            let lines: Vec<f32> = b
                .ops
                .iter()
                .filter_map(|o| match o {
                    Op::Rect { y, w, h, .. } if *w > m.width / 4.0 => Some(y + h / 2.0),
                    _ => None,
                })
                .collect();
            let staff_mid = match (lines.iter().copied().reduce(f32::min), lines.iter().copied().reduce(f32::max)) {
                (Some(a), Some(z)) => (a + z) / 2.0,
                _ => b.height / 2.0,
            };
            let run = fonts.shape(&label, fonts.resolve(style.italic, style.bold), false);
            b.ops.push(Op::Text {
                x: 0.0,
                baseline: staff_mid + lsize * 0.3,
                size: lsize,
                run,
                color: Color::Red,
            });
        }
        if let Some(b) = tb.last_mut() {
            b.keep_with_next = true;
        }
        out.extend(tb);
    }
    for (vi, verse) in verses.iter().enumerate().take(chant_verses) {
        let intone = match ps.intone {
            Intone::FirstVerse => {
                if vi == 0 {
                    Intone::FirstVerse
                } else {
                    Intone::Never
                }
            }
            other => other,
        };
        // Verse numbers are for the pointed text; chant verses go without.
        let words = strip_number(verse);
        let setting = neuma_tones::apply_text(
            &tone,
            words,
            &Options {
                intone,
                // The accents place the cadence; under notes they would only clutter.
                strip_accents: true,
                ..Options::default()
            },
        );
        diags.extend(setting.diagnostics);
        let mut b = score_blocks(
            &setting.score,
            fonts,
            m,
            0,
            m.width,
            0.0,
            if vi == 0 { size * 0.5 } else { size * 0.25 },
            diags,
        );
        if vi == 0
            && let Some(last) = b.last_mut()
        {
            // The first verse stays with what follows it only through the orphan rules.
            last.keep_with_next = chant_verses == 1;
        }
        out.extend(b);
    }
    if chant_verses < verses.len() {
        let rest = verses[chant_verses..].join("\n");
        let pointing = neuma_tones::point_text(&tone, &rest);
        diags.extend(pointing.pointed.diagnostics.iter().cloned());
        for h in pointing
            .halves
            .iter()
            .filter(|h| !h.kept && h.confidence < neuma_tones::apply::UNSURE)
        {
            let verse = &pointing.pointed.verses[h.verse];
            diags.push(Diagnostic {
                severity: Severity::Info,
                span: verse.span.clone(),
                code: "point::unsure",
                message: format!(
                    "verse {}: pointed automatically, but only {:.0}% sure: check where the accents fall",
                    verse.number.map_or_else(|| (h.verse + 1).to_string(), |n| n.to_string()),
                    h.confidence * 100.0
                ),
            });
        }
        let nsize = size * 0.9;
        let number_w = fonts.width("00", fonts.resolve(false, false), false) * nsize + size * 0.45;
        let pointed = pointing.text();
        for (k, line) in pointed.lines().enumerate() {
            let line = line.replace("\\-", "-");
            let (num, body) = split_number(&line);
            let mut p = Para {
                spans: pointed_spans(body, fonts),
                size,
                leading: s.leading,
                align: Align::Left,
                indent: number_w,
                first: 0.0,
                dropcap: false,
            };
            p.indent = number_w;
            let mut lines = text::set(fonts, &p, m.width);
            if let (Some(n), Some(first)) = (num, lines.first_mut()) {
                let run = fonts.shape(n, fonts.resolve(false, false), false);
                let w = run.width * nsize;
                first.ops.push(Op::Text {
                    x: number_w - size * 0.45 - w,
                    baseline: first.baseline,
                    size: nsize,
                    run,
                    color: Color::Red,
                });
            }
            let n = lines.len();
            for (j, l) in lines.into_iter().enumerate() {
                out.push(Block {
                    height: l.height,
                    space_before: match (j, k) {
                        (0, 0) if chant_verses > 0 => size * 0.5,
                        (0, k) if k > 0 => size * 0.2,
                        _ => 0.0,
                    },
                    ops: l.ops,
                    // A verse stays whole; the first verse also stays with the tone above it.
                    keep_with_next: j + 1 < n,
                    kind: Kind::Content,
                });
            }
        }
    }
    out
}

fn strip_number(verse: &str) -> &str {
    split_number(verse).1
}

/// A leading verse number and the rest.
fn split_number(line: &str) -> (Option<&str>, &str) {
    let t = line.trim_start();
    let end = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    if end > 0 && t[end..].starts_with(char::is_whitespace) {
        (Some(&t[..end]), t[end..].trim_start())
    } else {
        (None, t)
    }
}

/// Pointed text with its marks in red: `·`, `*`, `†`, the held-note dash, and `[rubrics]`
/// in red italic without their brackets.
fn pointed_spans(body: &str, fonts: &Fonts) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut push = |t: &str, style: Style| match out.last_mut() {
        Some(l) if l.style == style => l.text.push_str(t),
        _ => out.push(Span { text: t.into(), style }),
    };
    let red = Style {
        red: true,
        ..Style::default()
    };
    let mut rest = body;
    while let Some(c) = rest.chars().next() {
        if c == '['
            && let Some(end) = rest.find(']')
        {
            push(
                &rest[1..end],
                Style {
                    italic: true,
                    red: true,
                    ..Style::default()
                },
            );
            rest = &rest[end + 1..];
            continue;
        }
        let mut b = [0u8; 4];
        let t = c.encode_utf8(&mut b);
        // The point and the held-note dash are bold, as hand-pointed psalters print them.
        match c {
            '·' => push(
                t,
                Style {
                    bold: fonts.face(crate::font::BOLD).is_some(),
                    ..red
                },
            ),
            '*' | '†' | '‡' | '–' => push(t, red),
            _ => push(t, Style::default()),
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_as_a_score() {
        let g = tone_gabc(Tone::named("8.G").unwrap());
        assert_eq!(g, "(c4) (g) (h) (j) (k) (j) (j) *(:) (j) (i) (j) (h) (g) (g) (::)");
        let parsed = neuma::parse(&g);
        assert!(parsed.diagnostics.iter().all(|d| d.severity != Severity::Error));
    }

    #[test]
    fn no_staff_is_left_alone() {
        let fonts = Fonts::standard();
        let m = Metrics::new(&Settings::default());
        let parsed = neuma::parse(include_str!("../../neuma/tests/corpus/puer-natus.gabc"));
        let mut diags = Vec::new();
        let blocks = score_blocks(&parsed.score, &fonts, &m, 1, m.width / 2.0, 0.0, 0.0, &mut diags);
        let n = blocks.len();
        assert!(n >= 4, "{n} lines");
        let keeps: Vec<bool> = blocks.iter().map(|b| b.keep_with_next).collect();
        // The first line keeps with the second, the second-last with the last.
        let mut want = vec![false; n];
        want[0] = true;
        want[n - 2] = true;
        assert_eq!(keeps, want);
        assert!(blocks.iter().all(|b| b.height > 0.0 && !b.ops.is_empty()));
    }

    #[test]
    fn headings_and_antiphons_keep_with_what_follows() {
        let fonts = Fonts::standard();
        let book = Book::parse(
            "heading: The Psalms\nscore:\n    (c4) A(g)men.(g) (::)\npsalm tone=8.G:\n    1 O come, let us sing unto the Lord * let us heartily rejoice.\n    2 Let us come * and shew ourselves glad.\n",
        )
        .unwrap();
        let (blocks, problems) = blocks(&book, &fonts);
        assert!(problems.iter().all(|p| p.diagnostic.severity != Severity::Error), "{problems:?}");
        // Heading, antiphon line, tone line and the first verse are one chain; the chain
        // ends with the first verse's last line.
        let chain_end = blocks.iter().position(|b| !b.keep_with_next).unwrap();
        let verse_one = blocks
            .iter()
            .position(|b| {
                b.ops
                    .iter()
                    .any(|o| matches!(o, Op::Text { run, .. } if run.glyphs.iter().map(|g| g.text.as_str()).collect::<String>() == "O"))
            })
            .unwrap();
        assert!(verse_one <= chain_end);
        assert!(chain_end + 1 < blocks.len());
    }

    #[test]
    fn verse_numbers() {
        assert_eq!(split_number("12 O come"), (Some("12"), "O come"));
        assert_eq!(split_number("O come"), (None, "O come"));
        assert_eq!(split_number("1st"), (None, "1st"));
    }

    #[test]
    fn pointed_marks_are_red() {
        let f = Fonts::standard();
        let spans = pointed_spans("unto the · Lórd * [Stand] let us", &f);
        let red: Vec<&str> = spans.iter().filter(|s| s.style.red).map(|s| s.text.as_str()).collect();
        assert_eq!(red, vec!["·", "*", "Stand"]);
    }
}
