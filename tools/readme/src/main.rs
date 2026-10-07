//! Draws the README's score images from neuma's own output:
//!
//! ```sh
//! cargo run -p readme-images -- [OUT_DIR] [FONT_DIR]
//! ```
//!
//! `OUT_DIR` defaults to `docs/images`, and `FONT_DIR` to Debian's EB Garamond 12 directory
//! (`fonts-ebgaramond`). The lyrics are measured with neuma's built-in EB Garamond 12 metrics
//! and drawn as outlines from those font files, so the SVGs need no font where they are shown
//! (GitHub shows SVG in `<img>`, which loads no fonts). Each image has a white background so
//! it reads on light and dark pages alike. See `tools/readme/README.md` for the other images.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use neuma::{DisplayList, Engraving, Font, Initial, Item, LayoutOptions, StyleOptions, TextRole, Weights};
use neuma_book::font::{FontFiles, Fonts, ITALIC, REGULAR, Seg};
use neuma_tones::{Options, Role, Tone, apply_text, point_text};

const INK: &str = "#1d1b18";
const RED: &str = "#a3211c";
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| format!("{ROOT}/docs/images")));
    let font_dir = PathBuf::from(args.next().unwrap_or_else(|| "/usr/share/fonts/opentype/ebgaramond".into()));
    let face = |style: &str| font_dir.join(format!("EBGaramond12-{style}.otf"));
    let (regular, italic, bold) = (face("Regular"), face("Italic"), face("Bold"));
    let files = FontFiles::load([Some(&regular), Some(&italic), Some(&bold), None])?;
    let fonts = Fonts::new(&files);
    std::fs::create_dir_all(&out)?;

    let write = |name: &str, svg: String| -> std::io::Result<()> {
        let path = out.join(name);
        std::fs::write(&path, svg)?;
        println!("{}: {} bytes", path.display(), std::fs::metadata(&path)?.len());
        Ok(())
    };
    write("hero.svg", hero(&fonts)?)?;
    write("reflow.svg", reflow(&fonts)?)?;
    write("psalm-tone.svg", psalm(&fonts))?;
    write("timeline.svg", timeline(&fonts)?)?;
    Ok(())
}

fn read(path: &str) -> std::io::Result<String> {
    std::fs::read_to_string(Path::new(ROOT).join(path))
}

/// Parses and engraves a score with the metrics of the font the images are drawn with.
fn engrave(gabc: &str, initial: Initial) -> Engraving {
    let parsed = neuma::parse(gabc);
    for d in &parsed.diagnostics {
        eprintln!("{d}");
    }
    let style = StyleOptions {
        initial,
        ..StyleOptions::default()
    };
    parsed.score.engrave(Font::Garamond12.table(), &style)
}

/// An SVG built up from display lists and text, with every outline in one `<defs>`.
struct Canvas<'a> {
    fonts: &'a Fonts<'a>,
    neumes: BTreeSet<u16>,
    glyphs: BTreeSet<(usize, u16)>,
    body: String,
}

impl<'a> Canvas<'a> {
    fn new(fonts: &'a Fonts<'a>) -> Canvas<'a> {
        Canvas {
            fonts,
            neumes: BTreeSet::new(),
            glyphs: BTreeSet::new(),
            body: String::new(),
        }
    }

    /// Draws a display list's items at (dx, dy). `paint` can color an item; text is red when
    /// it is a rubric, and everything else is ink.
    fn items(&mut self, list: &DisplayList, dx: f32, dy: f32, paint: &dyn Fn(&Item) -> Option<&'static str>) {
        let _ = write!(self.body, r#"<g transform="translate({dx:.2} {dy:.2})">"#);
        for item in &list.items {
            let fill = paint(item);
            let attr = fill.map(|f| format!(r#" fill="{f}""#)).unwrap_or_default();
            match item {
                Item::Glyph { glyph, x, y, scale, .. } => {
                    self.neumes.insert(*glyph);
                    let _ = write!(
                        self.body,
                        r##"<use href="#n{glyph}" transform="translate({x:.2} {y:.2}) scale({scale:.5})"{attr}/>"##
                    );
                }
                Item::Rect { x, y, w, h, .. } => {
                    let _ = write!(self.body, r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"{attr}/>"#);
                }
                Item::Text {
                    x,
                    baseline,
                    size,
                    runs,
                    role,
                    ..
                } => {
                    let mut at = *x;
                    for r in runs {
                        let red = *role == TextRole::Rubric || r.style.rubric;
                        let face = self.fonts.resolve(r.style.italic, r.style.bold);
                        let fill = fill.or(red.then_some(RED));
                        at += self.text(at, *baseline, *size, &r.text, face, r.style.small_caps, fill);
                    }
                }
            }
        }
        self.body.push_str("</g>");
    }

    /// Draws text as outlines and returns its advance.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, x: f32, baseline: f32, size: f32, text: &str, face: usize, small_caps: bool, fill: Option<&str>) -> f32 {
        let run = self.fonts.shape(text, face, small_caps);
        match fill {
            Some(f) => {
                let _ = write!(self.body, r#"<g fill="{f}">"#);
            }
            None => self.body.push_str("<g>"),
        }
        let mut at = x;
        for g in &run.glyphs {
            self.glyphs.insert((run.face, g.id));
            let _ = write!(
                self.body,
                r##"<use href="#t{}-{}" transform="translate({:.2} {:.2}) scale({:.3})"/>"##,
                run.face,
                g.id,
                at + g.dx * size,
                baseline - g.dy * size,
                size
            );
            at += g.advance * size;
        }
        self.body.push_str("</g>");
        run.width * size
    }

    fn width(&self, text: &str, face: usize, size: f32) -> f32 {
        self.fonts.width(text, face, false) * size
    }

    /// The finished document, `w` by `h`: on a white rounded card if `card`.
    fn finish(self, w: f32, h: f32, label: &str, card: bool) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" role="img" aria-label="{label}" fill="{INK}">"#
        );
        if card {
            let _ = write!(out, r##"<rect width="{w:.0}" height="{h:.0}" rx="10" fill="#fff"/>"##);
        }
        out.push_str("<defs>");
        for g in &self.neumes {
            if let Some(o) = neuma::glyph_outline(*g) {
                let _ = write!(out, r#"<path id="n{g}" d="{}"/>"#, o.d);
            }
        }
        for (f, g) in &self.glyphs {
            if let Some(face) = self.fonts.face(*f) {
                let segs = face.outline(*g);
                if !segs.is_empty() {
                    let _ = write!(out, r#"<path id="t{f}-{g}" d="{}"/>"#, seg_d(&segs));
                }
            }
        }
        out.push_str("</defs>");
        out.push_str(&self.body);
        out.push_str("</svg>\n");
        out
    }
}

/// Font outline segments (in ems, y up) as path data, y down.
fn seg_d(segs: &[Seg]) -> String {
    let mut d = String::new();
    let n = |v: f32| {
        let s = format!("{v:.4}");
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s == "-0" { "0".to_string() } else { s }
    };
    for s in segs {
        let _ = match *s {
            Seg::Move(x, y) => write!(d, "M{} {}", n(x), n(-y)),
            Seg::Line(x, y) => write!(d, "L{} {}", n(x), n(-y)),
            Seg::Cubic(a, b, c, e, x, y) => write!(d, "C{} {} {} {} {} {}", n(a), n(-b), n(c), n(-e), n(x), n(-y)),
            Seg::Close => write!(d, "Z"),
        };
    }
    d
}

const PAD: f32 = 24.0;

/// Puer natus est, engraved at one width.
fn hero(fonts: &Fonts) -> std::io::Result<String> {
    let eng = engrave(&read("tools/readme/scores/puer-natus.gabc")?, Initial::Lines(1));
    let list = eng
        .layout(
            780.0,
            &LayoutOptions {
                scale: 7.0,
                ..LayoutOptions::default()
            },
        )
        .display();
    let mut c = Canvas::new(fonts);
    c.items(&list, PAD, PAD, &|_| None);
    Ok(c.finish(
        list.width + 2.0 * PAD,
        list.height + 2.0 * PAD,
        "Puer natus est, engraved by neuma",
        true,
    ))
}

/// One score laid out at a range of widths, shown in turn as if a window were resized.
fn reflow(fonts: &Fonts) -> std::io::Result<String> {
    let eng = engrave(&read("crates/neuma/tests/golden/salve-regina-simple.gabc")?, Initial::Lines(1));
    let widths: Vec<f32> = (0..7).map(|i| 840.0 - 70.0 * i as f32).collect();
    // There and back again, holding a little at each end.
    let mut frames: Vec<f32> = widths.clone();
    frames.extend(widths.iter().rev().skip(1).take(widths.len() - 2));
    let step = 0.6f32;
    let dur = step * frames.len() as f32;
    let lists: Vec<DisplayList> = widths.iter().map(|w| eng.layout(*w, &LayoutOptions::default()).display()).collect();
    let height = lists.iter().map(|l| l.height).fold(0.0, f32::max) + 2.0 * PAD + 18.0;
    let width = widths[0] + 2.0 * PAD;
    let mut c = Canvas::new(fonts);
    for (i, list) in lists.iter().enumerate() {
        // Visible in every frame that shows this width.
        let shown: Vec<usize> = frames
            .iter()
            .enumerate()
            .filter(|(_, w)| **w == widths[i])
            .map(|(k, _)| k)
            .collect();
        let mut times = vec![0.0];
        let mut values = vec![if shown.contains(&0) { "visible" } else { "hidden" }];
        for k in 1..frames.len() {
            let v = if shown.contains(&k) { "visible" } else { "hidden" };
            if *values.last().unwrap_or(&"") != v {
                times.push(k as f32 / frames.len() as f32);
                values.push(v);
            }
        }
        let key: Vec<String> = times.iter().map(|t| format!("{t:.4}")).collect();
        let _ = write!(
            c.body,
            r#"<g visibility="{}"><animate attributeName="visibility" values="{}" keyTimes="{}" calcMode="discrete" dur="{dur:.1}s" repeatCount="indefinite"/>"#,
            values[0],
            values.join(";"),
            key.join(";")
        );
        // The window: a card as wide as the layout, with its width under it.
        let card_w = widths[i] + 2.0 * PAD;
        let card_h = list.height + 2.0 * PAD + 18.0;
        let _ = write!(
            c.body,
            r##"<rect x=".5" y=".5" width="{:.1}" height="{:.1}" rx="10" fill="#fff" stroke="#d0d7de"/>"##,
            card_w - 1.0,
            card_h - 1.0
        );
        let label = format!("{} px", widths[i]);
        let lw = c.width(&label, ITALIC, 14.0);
        c.text(card_w - PAD - lw, card_h - PAD + 4.0, 14.0, &label, ITALIC, false, Some(RED));
        c.items(list, PAD, PAD, &|_| None);
        c.body.push_str("</g>");
    }
    Ok(c.finish(width, height, "Salve Regina reflowing as the width changes", false))
}

/// Psalm 117 (Book of Common Prayer psalter), pointed automatically for tone 8.G and set to
/// it, each note colored by its role in the tone.
fn psalm(fonts: &Fonts) -> String {
    let tone = Tone::named("8.G").expect("built-in tone");
    let text = read("tools/readme/scores/psalm-117.txt").expect("psalm text");
    let pointed = point_text(tone, &text).text();
    let options = Options {
        strip_accents: true,
        ..Options::default()
    };
    let setting = apply_text(tone, &text, &options);
    for d in &setting.diagnostics {
        eprintln!("psalm: {d}");
    }
    let colors = |r: Role| match r {
        Role::Intonation => "#2f6db3",
        Role::Tenor => INK,
        Role::Preparatory => "#2e8b57",
        Role::Accent => RED,
        Role::Ending => "#c77700",
    };
    // Start each verse on a new line (`(Z)`, a ragged break after the bar that ends it).
    // Breaks aren't notes, so `setting.notes` still numbers the notes.
    let parsed = neuma::parse(&setting.gabc.replace(" (:) ", " (:) (Z) "));
    let style = StyleOptions {
        initial: Initial::None,
        ..StyleOptions::default()
    };
    let eng = parsed.score.engrave(Font::Garamond12.table(), &style);
    let width = 880.0;
    let list = eng.layout(width, &LayoutOptions::default()).display();

    let mut c = Canvas::new(fonts);
    // The pointed text, marks in red.
    let size = 17.0;
    let mut y = PAD + size;
    for line in pointed.lines() {
        let mut x = PAD;
        let mut word = String::new();
        let flush = |c: &mut Canvas, word: &mut String, x: &mut f32| {
            if !word.is_empty() {
                *x += c.text(*x, y, size, word, REGULAR, false, None);
                word.clear();
            }
        };
        for (i, ch) in line.char_indices() {
            let verse_number = ch.is_ascii_digit() && line[..i].chars().all(|c| c.is_ascii_digit());
            if matches!(ch, '·' | '*' | '†') || verse_number {
                flush(&mut c, &mut word, &mut x);
                x += c.text(x, y, size, &ch.to_string(), REGULAR, false, Some(RED));
            } else {
                word.push(ch);
            }
        }
        flush(&mut c, &mut word, &mut x);
        y += size * 1.45;
    }
    let top = y;
    c.items(&list, PAD, top, &|item| {
        let note = match item {
            Item::Glyph { note, .. } | Item::Rect { note, .. } => *note,
            Item::Text { .. } => None,
        }?;
        setting.notes.get(note as usize).map(|n| colors(n.role))
    });
    // A legend.
    let mut x = PAD;
    let y = top + list.height + 18.0;
    for (role, name) in [
        (Role::Intonation, "intonation"),
        (Role::Tenor, "tenor"),
        (Role::Preparatory, "preparatory"),
        (Role::Accent, "accent"),
        (Role::Ending, "ending"),
    ] {
        let _ = write!(
            c.body,
            r#"<rect x="{x:.1}" y="{:.1}" width="11" height="11" rx="2" fill="{}"/>"#,
            y - 10.0,
            colors(role)
        );
        x += 17.0;
        x += c.text(x, y, 15.0, name, ITALIC, false, None) + 22.0;
    }
    c.finish(width + 2.0 * PAD, y + PAD - 4.0, "Psalm 117 pointed and set to tone 8.G", true)
}

/// A cursor following the note map's timeline: each note lit for its duration.
fn timeline(fonts: &Fonts) -> std::io::Result<String> {
    let eng = engrave(&read("crates/neuma/tests/golden/regina-caeli-simple.gabc")?, Initial::Lines(1));
    let layout = eng.layout(700.0, &LayoutOptions::default());
    let list = layout.display();
    let map = layout.notes(&Weights::default());
    let pulse = 0.32f32;
    let tail = 2.0f32;
    let dur = map.duration * pulse + tail;
    let t = |units: f32| format!("{:.4}", units * pulse / dur);
    let mut c = Canvas::new(fonts);
    let _ = write!(c.body, r#"<g transform="translate({PAD} {PAD})">"#);
    // One box that jumps from note to note, hidden during pauses and the tail.
    let (mut xs, mut ys, mut ops, mut keys) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut prev_end = 0.0f32;
    for n in &map.notes {
        if n.start > prev_end + 1e-3 {
            // A pause: keep the box where it was, hidden.
            xs.push(xs.last().cloned().unwrap_or_default());
            ys.push(ys.last().cloned().unwrap_or_default());
            ops.push("0");
            keys.push(t(prev_end));
        }
        xs.push(format!("{:.2}", n.x - n.w / 2.0 - 3.0));
        ys.push(format!("{:.2}", n.y - n.h / 2.0 - 3.0));
        ops.push("1");
        keys.push(t(n.start));
        prev_end = n.start + n.duration;
    }
    xs.push(xs.last().cloned().unwrap_or_default());
    ys.push(ys.last().cloned().unwrap_or_default());
    ops.push("0");
    keys.push(t(prev_end));
    if keys.first().map(String::as_str) != Some("0.0000") {
        keys[0] = "0.0000".into();
    }
    let w = map.notes.iter().map(|n| n.w).fold(0.0, f32::max) + 6.0;
    let h = map.notes.iter().map(|n| n.h).fold(0.0, f32::max) + 6.0;
    let anim = |attr: &str, values: &[String]| {
        format!(
            r#"<animate attributeName="{attr}" values="{}" keyTimes="{}" calcMode="discrete" dur="{dur:.2}s" repeatCount="indefinite"/>"#,
            values.join(";"),
            keys.join(";")
        )
    };
    let ops: Vec<String> = ops.iter().map(|s| s.to_string()).collect();
    let _ = write!(
        c.body,
        r##"<rect width="{w:.2}" height="{h:.2}" rx="3" fill="#f0a030" fill-opacity=".55" opacity="0">{}{}{}</rect>"##,
        anim("x", &xs),
        anim("y", &ys),
        anim("opacity", &ops)
    );
    c.body.push_str("</g>");
    c.items(&list, PAD, PAD, &|_| None);
    Ok(c.finish(
        list.width + 2.0 * PAD,
        list.height + 2.0 * PAD,
        "Regina caeli with a cursor following its notes",
        true,
    ))
}
