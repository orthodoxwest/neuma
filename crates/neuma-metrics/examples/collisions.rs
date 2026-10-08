//! Looks for ink that collides in neuma's output, with the lyric font's real outlines:
//!
//! ```text
//! collisions --regular R.ttf --italic I.ttf [--widths 300,500,800,1200] [--scale 6]
//!            [--detail] FILE_OR_DIR...
//! ```
//!
//! Every `.gabc` file named, or found in a directory named, is engraved with the built-in
//! Google Fonts metrics and laid out at each width. Then its display list is checked for:
//!
//! - `NT`: a note, stem, ledger line or other musical ink overlapping a lyric's letter, or a
//!   staff line crossing the initial (a Q's tail reaching the staff below);
//! - `TT`: two texts' letters overlapping, a hyphen included (syllables of a word that touch
//!   by their advances are left out: their letters meet as in any word);
//! - `OOB`: ink outside the page;
//! - `BAR`: a line that starts with a bar, where the score writes no break (`z`, `Z`) before
//!   it and the bar's text doesn't lead into what follows (a ℣ or ℟, a verse number, or a
//!   rubric such as `Ps.`). The line breaker does this only when every other way sets a line
//!   extremely loose, so this is a count to watch rather than a fault.
//!
//! Boxes overlap when they share more than a twentieth of a staff space both ways. Letters are
//! boxed by their outlines in the fonts given, which should be the ones the metrics describe.
//! One line per width gives the totals, and `--detail` lists the worst findings per file.

use std::path::{Path, PathBuf};

use neuma::glyphs::{GlyphId, UNITS_PER_SPACE};
use neuma::{Ink, Item, LayoutOptions, LyricFont, StyleOptions, TextMeasure, TextRole};
use rustybuzz::ttf_parser::Face;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Music,
    Text,
    /// A staff line: an obstacle for the initial only, as lyrics sit off the staff and
    /// everything else is drawn on it.
    Staff,
}

struct Box {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    kind: Kind,
    /// The display item it comes from.
    item: usize,
    label: String,
}

#[derive(Default, Clone, Copy)]
struct Counts {
    nt: usize,
    tt: usize,
    oob: usize,
    bar: usize,
}

fn usage() -> ! {
    eprintln!("usage: collisions --regular R.ttf --italic I.ttf [--widths W,W..] [--scale S] [--detail] FILE_OR_DIR...");
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut regular, mut italic) = (None, None);
    let mut widths = vec![300.0, 500.0, 800.0, 1200.0];
    let mut scale = 6.0;
    let mut detail = false;
    let mut files = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--regular" => regular = args.next(),
            "--italic" => italic = args.next(),
            "--widths" => {
                let list = args.next().unwrap_or_else(|| usage());
                widths = list.split(',').map(|w| w.parse().unwrap_or_else(|_| usage())).collect();
            }
            "--scale" => scale = args.next().and_then(|s| s.parse().ok()).unwrap_or_else(|| usage()),
            "--detail" => detail = true,
            _ => collect(Path::new(&a), &mut files),
        }
    }
    let (Some(regular), Some(italic)) = (regular, italic) else {
        usage()
    };
    let read = |p: &str| std::fs::read(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    let (regular, italic) = (read(&regular), read(&italic));
    files.sort();

    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<(usize, Vec<Counts>, String)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let faces = [
                        Face::parse(&regular, 0).expect("regular font"),
                        Face::parse(&italic, 0).expect("italic font"),
                    ];
                    let mut out = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(file) = files.get(k) else { break };
                        let Ok(src) = std::fs::read_to_string(file) else { continue };
                        let (counts, report) = check(&src, &file.display().to_string(), &widths, scale, &faces, detail);
                        out.push((k, counts, report));
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap()).collect()
    });
    let mut results = results;
    results.sort_by_key(|r| r.0);
    let mut totals = vec![(Counts::default(), [0usize; 4]); widths.len()];
    for (_, counts, report) in &results {
        print!("{report}");
        for (t, c) in totals.iter_mut().zip(counts) {
            t.0.nt += c.nt;
            t.0.tt += c.tt;
            t.0.oob += c.oob;
            t.0.bar += c.bar;
            t.1[0] += usize::from(c.nt > 0);
            t.1[1] += usize::from(c.tt > 0);
            t.1[2] += usize::from(c.oob > 0);
            t.1[3] += usize::from(c.bar > 0);
        }
    }
    for (w, (c, f)) in widths.iter().zip(&totals) {
        println!(
            "width {w}: {} files, NT {} ({} files), TT {} ({} files), OOB {} ({} files), BAR {} ({} files)",
            results.len(),
            c.nt,
            f[0],
            c.tt,
            f[1],
            c.oob,
            f[2],
            c.bar,
            f[3]
        );
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for e in std::fs::read_dir(path).into_iter().flatten().flatten() {
            collect(&e.path(), out);
        }
    } else if path.extension().is_some_and(|e| e == "gabc") {
        out.push(path.to_path_buf());
    }
}

/// A letter's outline box, in ems: left, top, right, bottom, y down.
fn letter_box(face: &Face, c: char) -> Option<[f32; 4]> {
    let upem = f32::from(face.units_per_em());
    let bb = face.glyph_bounding_box(face.glyph_index(c)?)?;
    Some([
        f32::from(bb.x_min) / upem,
        -f32::from(bb.y_max) / upem,
        f32::from(bb.x_max) / upem,
        -f32::from(bb.y_min) / upem,
    ])
}

fn check(src: &str, name: &str, widths: &[f32], scale: f32, faces: &[Face; 2], detail: bool) -> (Vec<Counts>, String) {
    let metrics = LyricFont::Google.metrics();
    let eng = neuma::parse(src).score.engrave(metrics, &StyleOptions::default());
    let mut report = String::new();
    let mut all = Vec::new();
    for &w in widths {
        let layout = eng.layout_with(w, &LayoutOptions::default().with_scale(scale));
        let bar_led = lines_led_by_a_bar(src, &layout);
        let list = layout.display();
        let sp = list.staff_space;
        let mut boxes = Vec::new();
        // Text items by syllable, with their advance extents, to tell touching syllables.
        let mut texts: Vec<(usize, Option<u32>, f32, f32, f32)> = Vec::new();
        for (i, item) in list.items.iter().enumerate() {
            match item {
                Item::Glyph {
                    glyph,
                    x,
                    y,
                    scale: k,
                    role,
                    ..
                } => {
                    let Some(g) = GlyphId::from_id(*glyph) else { continue };
                    let (a, b, c, d) = g.ink();
                    let k = k * UNITS_PER_SPACE;
                    boxes.push(Box {
                        x0: x + a * k,
                        y0: y + b * k,
                        x1: x + c * k,
                        y1: y + d * k,
                        kind: Kind::Music,
                        item: i,
                        label: format!("{role:?}:{g:?}"),
                    });
                }
                Item::Rect { x, y, w, h, role, .. } => {
                    boxes.push(Box {
                        x0: *x,
                        y0: *y,
                        x1: x + w,
                        y1: y + h,
                        kind: if *role == Ink::Staff { Kind::Staff } else { Kind::Music },
                        item: i,
                        label: format!("{role:?}"),
                    });
                }
                Item::Text {
                    x,
                    baseline,
                    size,
                    runs,
                    role,
                    syllable,
                } => {
                    let mut pen = *x;
                    for r in runs {
                        let face = &faces[usize::from(r.style.italic)];
                        let mut prefix = String::new();
                        for c in r.text.chars() {
                            prefix.push(c);
                            // Where `c` starts, kerned against the letter before it.
                            let at = pen + (metrics.advance(&prefix, r.style) - metrics.advance(&c.to_string(), r.style)) * size;
                            // A small capital the font lacks is a capital at 70%.
                            let (c, f) = if r.style.small_caps && c.is_lowercase() {
                                (c.to_uppercase().next().unwrap_or(c), 0.7)
                            } else {
                                (c, 1.0)
                            };
                            if let Some([a, b, cx, d]) = letter_box(face, c) {
                                let k = size * f;
                                boxes.push(Box {
                                    x0: at + a * k,
                                    y0: baseline + b * k,
                                    x1: at + cx * k,
                                    y1: baseline + d * k,
                                    kind: Kind::Text,
                                    item: i,
                                    label: format!("{role:?}:{c}"),
                                });
                            }
                        }
                        pen += metrics.advance(&r.text, r.style) * size;
                    }
                    if matches!(role, TextRole::Lyric | TextRole::Rubric) {
                        texts.push((i, *syllable, *baseline, *x, pen));
                    }
                }
                _ => {}
            }
        }
        // Items of consecutive syllables whose texts touch: one word, set as one.
        texts.sort_by(|a, b| (a.2, a.3).partial_cmp(&(b.2, b.3)).unwrap());
        let mut touching = std::collections::HashSet::new();
        for p in texts.windows(2) {
            let (a, b) = (p[0], p[1]);
            if a.2 == b.2 && (b.3 - a.4).abs() < 0.01 * sp && matches!((a.1, b.1), (Some(s), Some(t)) if t == s + 1) {
                touching.insert((a.0, b.0));
                touching.insert((b.0, a.0));
            }
        }

        let mut c = Counts {
            bar: bar_led.len(),
            ..Counts::default()
        };
        let mut findings: Vec<(f32, String)> = Vec::new();
        for line in &bar_led {
            findings.push((0.0, format!("BAR line {line} starts with a bar")));
        }
        let slack = 0.25 * sp;
        for b in &boxes {
            if b.x0 < -slack || b.y0 < -slack || b.x1 > list.width + slack || b.y1 > list.height + slack {
                c.oob += 1;
                findings.push((
                    f32::INFINITY,
                    format!(
                        "OOB {} [{:.1},{:.1},{:.1},{:.1}] of {:.1}x{:.1}",
                        b.label, b.x0, b.y0, b.x1, b.y1, list.width, list.height
                    ),
                ));
            }
        }
        let mut order: Vec<usize> = (0..boxes.len()).collect();
        order.sort_by(|&a, &b| boxes[a].x0.total_cmp(&boxes[b].x0));
        let tol = 0.05 * sp;
        for (n, &i) in order.iter().enumerate() {
            let a = &boxes[i];
            for &j in &order[n + 1..] {
                let b = &boxes[j];
                if b.x0 > a.x1 {
                    break;
                }
                if a.item == b.item || (a.kind == Kind::Music && b.kind == Kind::Music) || touching.contains(&(a.item, b.item)) {
                    continue;
                }
                // Staff lines run under the notes and beside the lyrics; only the initial,
                // which hangs beside the staves, may not cross one.
                let initial = |x: &Box| x.kind == Kind::Text && x.label.starts_with("Initial");
                if (a.kind == Kind::Staff || b.kind == Kind::Staff) && !(initial(a) || initial(b)) {
                    continue;
                }
                let pen = (a.x1.min(b.x1) - a.x0.max(b.x0)).min(a.y1.min(b.y1) - a.y0.max(b.y0));
                if pen > tol {
                    let cat = if a.kind == b.kind { "TT" } else { "NT" };
                    if cat == "TT" {
                        c.tt += 1
                    } else {
                        c.nt += 1
                    }
                    findings.push((
                        pen / sp,
                        format!(
                            "{cat} {:.2}sp {} [{:.1},{:.1},{:.1},{:.1}] vs {} [{:.1},{:.1},{:.1},{:.1}]",
                            pen / sp,
                            a.label,
                            a.x0,
                            a.y0,
                            a.x1,
                            a.y1,
                            b.label,
                            b.x0,
                            b.y0,
                            b.x1,
                            b.y1
                        ),
                    ));
                }
            }
        }
        if detail {
            findings.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (_, f) in findings.iter().take(8) {
                report.push_str(&format!("{name}\t{w}\t{f}\n"));
            }
        }
        all.push(c);
    }
    (all, report)
}

/// The lines after the first that start with a bar: one drawn before every note on its
/// line, with no syllable of the line before it in the source (a cue of text alone may lead
/// it), no break written between the line before and the bar, and no text under the bar
/// that leads into what follows.
fn lines_led_by_a_bar(src: &str, layout: &neuma::Layout) -> Vec<u32> {
    let map = layout.source_map();
    (1..map.lines.len() as u32)
        .filter(|&line| {
            let first_note = map
                .notes
                .iter()
                .filter(|e| e.line == line)
                .map(|e| e.x)
                .fold(f32::INFINITY, f32::min);
            let Some(bar) = map.bars.iter().filter(|b| b.line == line).min_by(|a, b| a.x.total_cmp(&b.x)) else {
                return false;
            };
            let text_first = map.syllables.iter().any(|s| s.line == line && s.span.end <= bar.span.start);
            let before = map
                .syllables
                .iter()
                .filter(|s| s.line == line - 1)
                .map(|s| s.span.start)
                .max()
                .unwrap_or(0);
            let leads = map
                .syllables
                .iter()
                .find(|s| s.span.start <= bar.span.start && bar.span.end <= s.span.end)
                .is_some_and(|s| text_leads(src.get(s.span.start..bar.span.start).unwrap_or("")));
            bar.x < first_note && !text_first && !leads && !written_break(src.get(before..bar.span.start).unwrap_or(""))
        })
        .collect()
}

/// Whether a bar's syllable text, as written, leads into what follows: a ℣ or ℟
/// (`<sp>V/</sp>`), a verse number (`2.`), or a rubric (`Ps.`, `Ant.`, `T. P.`), as the line
/// breaker reads it.
fn text_leads(gabc: &str) -> bool {
    let text = gabc.split('(').next().unwrap_or("");
    // Without tags, the text above the staff (`<alt>`) or centering braces.
    let text = match (text.find("<alt>"), text.find("</alt>")) {
        (Some(a), Some(b)) if a < b => format!("{}{}", &text[..a], &text[b..]),
        _ => text.to_string(),
    };
    let mut plain = String::new();
    let mut tag = false;
    for c in text.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            '{' | '}' => {}
            _ if !tag => plain.push(c),
            _ => {}
        }
    }
    let plain = plain.trim_start();
    let numbered = plain.trim_start_matches(|c: char| c.is_ascii_digit());
    ["V/", "R/", "℣", "℟", "Ps.", "Ant.", "T. P.", "T.P."]
        .iter()
        .any(|p| plain.starts_with(p))
        || numbered.len() < plain.len() && numbered.starts_with('.')
}

/// Whether GABC holds a line break in its notation: a `z` or `Z` (not the custos `z0`) in
/// parentheses, before any NABC.
fn written_break(gabc: &str) -> bool {
    let (mut inside, mut nabc) = (false, false);
    let mut chars = gabc.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' => (inside, nabc) = (true, false),
            ')' => inside = false,
            '|' if inside => nabc = true,
            'z' | 'Z' if inside && !nabc && chars.peek() != Some(&'0') => return true,
            _ => {}
        }
    }
    false
}
