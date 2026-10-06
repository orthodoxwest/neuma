//! One SVG per page. Text is set with the font named by family (each run at the x it was
//! measured for), or drawn as outlines so the file needs no font.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::font::{Fonts, Seg};
use crate::page::{Color, Op, Page};
use crate::pdf::num;

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c >= ' ' || c == '\t' => out.push(c),
            _ => {}
        }
    }
}

fn seg_d(segs: &[Seg]) -> String {
    let mut d = String::new();
    for s in segs {
        // Font outlines are y up; flip them here so `scale(size)` is enough.
        let _ = match *s {
            Seg::Move(x, y) => write!(d, "M{} {}", num(x), num(-y)),
            Seg::Line(x, y) => write!(d, "L{} {}", num(x), num(-y)),
            Seg::Cubic(a, b, c, e, x, y) => write!(d, "C{} {} {} {} {} {}", num(a), num(-b), num(c), num(-e), num(x), num(-y)),
            Seg::Close => write!(d, "Z"),
        };
    }
    d
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// The page as a standalone SVG document, sized in points.
pub fn page(p: &Page, fonts: &Fonts, red: [u8; 3], text_as_paths: bool) -> String {
    let paths = text_as_paths && !fonts.is_standard();
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}pt" height="{h}pt" viewBox="0 0 {w} {h}">"#,
        w = num(p.width),
        h = num(p.height)
    );
    let family = match fonts.face(0) {
        Some(f) => format!("'{}', serif", f.family.replace(['\'', '<', '>', '&', '"', ';', '{', '}'], "")),
        None => "'Times New Roman', Times, serif".into(),
    };
    let _ = write!(
        out,
        "<style>text{{font-family:{family};font-variant-ligatures:none;font-kerning:normal;white-space:pre}}.r{{fill:{}}}</style>",
        hex(red)
    );
    let _ = write!(out, r#"<rect width="{}" height="{}" fill="white"/>"#, num(p.width), num(p.height));
    let mut neumes = BTreeSet::new();
    let mut glyphs = BTreeSet::new();
    for op in &p.ops {
        match op {
            Op::Neume { glyph, .. } => {
                neumes.insert(*glyph);
            }
            Op::Text { run, .. } if paths => {
                for g in &run.glyphs {
                    glyphs.insert((run.face, g.id));
                }
            }
            _ => {}
        }
    }
    out.push_str("<defs>");
    for g in &neumes {
        if let Some(o) = neuma::glyph_outline(*g) {
            let _ = write!(out, r#"<path id="n{g}" d="{}"/>"#, o.d);
        }
    }
    for (f, g) in &glyphs {
        if let Some(face) = fonts.face(*f) {
            let segs = face.outline(*g);
            if !segs.is_empty() {
                let _ = write!(out, r#"<path id="t{f}-{g}" d="{}"/>"#, seg_d(&segs));
            }
        }
    }
    out.push_str("</defs>");
    for op in &p.ops {
        let class = |c: &Color| if *c == Color::Red { r#" class="r""# } else { "" };
        match op {
            Op::Neume { glyph, x, y, scale, color } => {
                let _ = write!(
                    out,
                    r##"<use href="#n{glyph}" transform="translate({} {}) scale({})"{}/>"##,
                    num(*x),
                    num(*y),
                    num(*scale),
                    class(color)
                );
            }
            Op::Rect { x, y, w, h, color } => {
                let _ = write!(
                    out,
                    r#"<rect x="{}" y="{}" width="{}" height="{}"{}/>"#,
                    num(*x),
                    num(*y),
                    num(*w),
                    num(*h),
                    class(color)
                );
            }
            Op::Text {
                x,
                baseline,
                size,
                run,
                color,
            } => {
                if paths {
                    let _ = write!(out, "<g{}>", class(color));
                    let mut at = *x;
                    for g in &run.glyphs {
                        if glyphs.contains(&(run.face, g.id)) {
                            let _ = write!(
                                out,
                                r##"<use href="#t{}-{}" transform="translate({} {}) scale({})"/>"##,
                                run.face,
                                g.id,
                                num(at + g.dx * size),
                                num(baseline - g.dy * size),
                                num(*size)
                            );
                        }
                        at += g.advance * size;
                    }
                    out.push_str("</g>");
                    continue;
                }
                let text: String = run.glyphs.iter().map(|g| g.text.as_str()).collect();
                let italic = run.face == crate::font::ITALIC || run.face == crate::font::BOLD_ITALIC;
                let bold = run.face >= crate::font::BOLD;
                let _ = write!(
                    out,
                    r#"<text x="{}" y="{}" font-size="{}"{}{}{}{}>"#,
                    num(*x),
                    num(*baseline),
                    num(*size),
                    if run.small_caps { r#" font-variant="small-caps""# } else { "" },
                    if italic { r#" font-style="italic""# } else { "" },
                    if bold { r#" font-weight="bold""# } else { "" },
                    class(color)
                );
                escape(&text, &mut out);
                out.push_str("</text>");
            }
        }
    }
    out.push_str("</svg>\n");
    out
}
