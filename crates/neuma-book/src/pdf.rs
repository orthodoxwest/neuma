//! A small PDF writer: one content stream per page, neume glyphs as form XObjects, and text
//! either in the embedded font (TrueType subset, or the whole OpenType file for CFF fonts)
//! with a ToUnicode map so it can be searched and copied, or drawn as outlines.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::font::{Fonts, STANDARD, Seg, subset_truetype};
use crate::page::{Color, Op, Page};

/// How text is put in the PDF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfOptions {
    /// Draw text as filled outlines instead of embedding the font.
    pub text_as_paths: bool,
    /// The rubric colour.
    pub red: [u8; 3],
}

/// A number with at most three decimals and no negative zero.
pub(crate) fn num(v: f32) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.to_string() }
}

struct Writer {
    buf: Vec<u8>,
    offsets: Vec<usize>,
}

impl Writer {
    fn reserve(&mut self) -> usize {
        self.offsets.push(0);
        self.offsets.len()
    }

    fn object(&mut self, id: usize, body: &str) {
        self.offsets[id - 1] = self.buf.len();
        self.buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        self.offsets[id - 1] = self.buf.len();
        let d = dict.trim_end_matches(">>");
        self.buf
            .extend_from_slice(format!("{id} 0 obj\n{d} /Length {} >>\nstream\n", data.len()).as_bytes());
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
    }
}

/// PDF path operators for neuma's absolute `M L C Z` outline data.
pub(crate) fn neume_path(d: &str) -> String {
    let mut out = String::new();
    let mut cmd = ' ';
    let mut nums: Vec<&str> = Vec::new();
    let flush = |cmd: char, nums: &mut Vec<&str>, out: &mut String| {
        let op = match cmd {
            'M' => "m",
            'L' => "l",
            'C' => "c",
            _ => return,
        };
        let per = if cmd == 'C' { 6 } else { 2 };
        for chunk in nums.chunks(per) {
            if chunk.len() == per {
                let _ = writeln!(out, "{} {op}", chunk.join(" "));
            }
        }
        nums.clear();
    };
    let mut rest = d;
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap_or(' ');
        if matches!(c, 'M' | 'L' | 'C' | 'Z') {
            flush(cmd, &mut nums, &mut out);
            if c == 'Z' {
                out.push_str("h\n");
            }
            cmd = c;
            rest = &rest[1..];
        } else if c == ' ' || c == ',' {
            rest = &rest[1..];
        } else {
            // A number runs until a space, a command or a second sign.
            let end = rest
                .char_indices()
                .skip(1)
                .find(|(_, c)| matches!(c, ' ' | ',' | 'M' | 'L' | 'C' | 'Z' | '-'))
                .map_or(rest.len(), |(i, _)| i);
            nums.push(&rest[..end]);
            rest = &rest[end..];
        }
    }
    flush(cmd, &mut nums, &mut out);
    out
}

/// PDF path operators for a font outline in ems.
fn seg_path(segs: &[Seg]) -> String {
    let mut out = String::new();
    for s in segs {
        let _ = match *s {
            Seg::Move(x, y) => writeln!(out, "{} {} m", num(x), num(y)),
            Seg::Line(x, y) => writeln!(out, "{} {} l", num(x), num(y)),
            Seg::Cubic(a, b, c, d, x, y) => {
                writeln!(out, "{} {} {} {} {} {} c", num(a), num(b), num(c), num(d), num(x), num(y))
            }
            Seg::Close => writeln!(out, "h"),
        };
    }
    out
}

fn color_op(c: Color, red: [u8; 3]) -> String {
    match c {
        Color::Black => "0 g".into(),
        Color::Red => format!(
            "{} {} {} rg",
            num(red[0] as f32 / 255.0),
            num(red[1] as f32 / 255.0),
            num(red[2] as f32 / 255.0)
        ),
    }
}

fn pdf_string(bytes: &[u8]) -> String {
    let mut s = String::from("(");
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                s.push('\\');
                s.push(b as char);
            }
            0x20..=0x7E => s.push(b as char),
            _ => {
                let _ = write!(s, "\\{:03o}", b);
            }
        }
    }
    s.push(')');
    s
}

/// A text string for the document information dictionary (UTF-16 with a byte-order mark).
fn text_string(t: &str) -> String {
    let mut s = String::from("<FEFF");
    for u in t.encode_utf16() {
        let _ = write!(s, "{u:04X}");
    }
    s.push('>');
    s
}

fn utf16_hex(t: &str) -> String {
    t.encode_utf16().map(|u| format!("{u:04X}")).collect()
}

fn to_unicode(map: &BTreeMap<u16, String>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<_> = map.iter().filter(|(_, t)| !t.is_empty()).collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (g, t) in chunk {
            let _ = writeln!(s, "<{g:04X}> <{}>", utf16_hex(t));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// Writes the pages as a PDF.
pub fn write(pages: &[Page], fonts: &Fonts, opts: &PdfOptions, title: Option<&str>) -> Vec<u8> {
    let paths = opts.text_as_paths && !fonts.is_standard();
    // What the pages use.
    let mut neumes: BTreeSet<u16> = BTreeSet::new();
    let mut faces: BTreeSet<usize> = BTreeSet::new();
    let mut glyphs: BTreeMap<usize, BTreeMap<u16, String>> = BTreeMap::new();
    for p in pages {
        for op in &p.ops {
            match op {
                Op::Neume { glyph, .. } => {
                    neumes.insert(*glyph);
                }
                Op::Text { run, .. } => {
                    faces.insert(run.face);
                    let m = glyphs.entry(run.face).or_default();
                    for g in &run.glyphs {
                        let e = m.entry(g.id).or_default();
                        if e.is_empty() {
                            e.clone_from(&g.text);
                        }
                    }
                }
                Op::Rect { .. } => {}
            }
        }
    }
    let mut w = Writer {
        buf: b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec(),
        offsets: Vec::new(),
    };
    let catalog = w.reserve();
    let pages_id = w.reserve();
    let info = w.reserve();
    // Neume glyphs, each a form XObject in glyph units.
    let mut neume_ids = BTreeMap::new();
    for g in &neumes {
        if let Some(o) = neuma::glyph_outline(*g) {
            let id = w.reserve();
            w.stream(
                id,
                "<< /Type /XObject /Subtype /Form /BBox [-10000 -10000 10000 10000] >>",
                format!("{}f\n", neume_path(&o.d)).as_bytes(),
            );
            neume_ids.insert(*g, id);
        }
    }
    // Text glyphs drawn as outlines, each a form XObject in ems.
    let mut glyph_ids: BTreeMap<(usize, u16), usize> = BTreeMap::new();
    let mut font_ids: BTreeMap<usize, usize> = BTreeMap::new();
    if paths {
        for (face, map) in &glyphs {
            let Some(f) = fonts.face(*face) else { continue };
            for g in map.keys() {
                let segs = f.outline(*g);
                if segs.is_empty() {
                    continue;
                }
                let id = w.reserve();
                w.stream(
                    id,
                    "<< /Type /XObject /Subtype /Form /BBox [-10 -10 10 10] >>",
                    format!("{}f\n", seg_path(&segs)).as_bytes(),
                );
                glyph_ids.insert((*face, *g), id);
            }
        }
    } else {
        for face in &faces {
            let id = w.reserve();
            match fonts.face(*face) {
                None => w.object(
                    id,
                    &format!(
                        "<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>",
                        STANDARD[*face]
                    ),
                ),
                Some(f) => {
                    let map = glyphs.get(face).cloned().unwrap_or_default();
                    let keep: BTreeSet<u16> = map.keys().copied().collect();
                    let subset = if f.cff { None } else { subset_truetype(f.data, &keep) };
                    let name = match &subset {
                        Some(_) => format!(
                            "NMBK{}{}+{}",
                            (b'A' + (*face as u8) / 26) as char,
                            (b'A' + (*face as u8) % 26) as char,
                            f.name
                        ),
                        None => f.name.clone(),
                    };
                    let file = w.reserve();
                    match &subset {
                        Some(data) => w.stream(file, &format!("<< /Length1 {} >>", data.len()), data),
                        None if f.cff => w.stream(file, "<< /Subtype /OpenType >>", f.data),
                        None => w.stream(file, &format!("<< /Length1 {} >>", f.data.len()), f.data),
                    }
                    let k = 1000.0 / f.upem;
                    let flags = 32 + 2 + if f.italic_angle != 0.0 { 64 } else { 0 };
                    let descriptor = w.reserve();
                    w.object(
                        descriptor,
                        &format!(
                            "<< /Type /FontDescriptor /FontName /{name} /Flags {flags} /FontBBox [{} {} {} {}] /ItalicAngle {} \
                             /Ascent {} /Descent {} /CapHeight {} /StemV 80 /{} {file} 0 R >>",
                            num(f.bbox[0] as f32 * k),
                            num(f.bbox[1] as f32 * k),
                            num(f.bbox[2] as f32 * k),
                            num(f.bbox[3] as f32 * k),
                            num(f.italic_angle),
                            num(f.ascent * 1000.0),
                            num(-f.descent * 1000.0),
                            num(f.cap_height * 1000.0),
                            if f.cff { "FontFile3" } else { "FontFile2" },
                        ),
                    );
                    let mut widths = String::new();
                    for g in map.keys() {
                        let _ = write!(widths, "{g} [{}] ", num(f.glyph_advance(*g) * 1000.0));
                    }
                    let cid = w.reserve();
                    let (subtype, gidmap) = if f.cff {
                        ("CIDFontType0", "")
                    } else {
                        ("CIDFontType2", " /CIDToGIDMap /Identity")
                    };
                    w.object(
                        cid,
                        &format!(
                            "<< /Type /Font /Subtype /{subtype} /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) \
                             /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /DW 0 /W [{widths}]{gidmap} >>"
                        ),
                    );
                    let tu = w.reserve();
                    w.stream(tu, "<< >>", to_unicode(&map).as_bytes());
                    w.object(
                        id,
                        &format!(
                            "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] \
                             /ToUnicode {tu} 0 R >>"
                        ),
                    );
                }
            }
            font_ids.insert(*face, id);
        }
    }
    let mut resources = String::from("<< /XObject << ");
    for (g, id) in &neume_ids {
        let _ = write!(resources, "/N{g} {id} 0 R ");
    }
    for ((f, g), id) in &glyph_ids {
        let _ = write!(resources, "/T{f}_{g} {id} 0 R ");
    }
    resources.push_str(">> /Font << ");
    for (f, id) in &font_ids {
        let _ = write!(resources, "/F{f} {id} 0 R ");
    }
    resources.push_str(">> >>");
    let resources_id = w.reserve();
    w.object(resources_id, &resources);
    let mut kids = Vec::new();
    for p in pages {
        let content = page_content(p, fonts, opts, paths, &neume_ids, &glyph_ids);
        let cid = w.reserve();
        w.stream(cid, "<< >>", content.as_bytes());
        let pid = w.reserve();
        w.object(
            pid,
            &format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {} {}] /Resources {resources_id} 0 R /Contents {cid} 0 R >>",
                num(p.width),
                num(p.height)
            ),
        );
        kids.push(format!("{pid} 0 R"));
    }
    w.object(
        pages_id,
        &format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), kids.len()),
    );
    w.object(catalog, &format!("<< /Type /Catalog /Pages {pages_id} 0 R >>"));
    let mut info_dict = String::from("<< /Producer (neuma-book) ");
    if let Some(t) = title {
        let _ = write!(info_dict, "/Title {} ", text_string(t));
    }
    info_dict.push_str(">>");
    w.object(info, &info_dict);
    let xref = w.buf.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", w.offsets.len() + 1);
    for o in &w.offsets {
        let _ = writeln!(tail, "{o:010} 00000 n ");
    }
    let _ = write!(
        tail,
        "trailer\n<< /Size {} /Root {catalog} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        w.offsets.len() + 1
    );
    w.buf.extend_from_slice(tail.as_bytes());
    w.buf
}

fn page_content(
    p: &Page,
    fonts: &Fonts,
    opts: &PdfOptions,
    paths: bool,
    neumes: &BTreeMap<u16, usize>,
    glyphs: &BTreeMap<(usize, u16), usize>,
) -> String {
    let mut s = String::new();
    // Everything below is in points from the top-left corner, y down.
    let _ = writeln!(s, "1 0 0 -1 0 {} cm", num(p.height));
    let mut color = Color::Black;
    s.push_str("0 g\n");
    for op in &p.ops {
        let c = match op {
            Op::Neume { color, .. } | Op::Rect { color, .. } | Op::Text { color, .. } => *color,
        };
        if c != color {
            let _ = writeln!(s, "{}", color_op(c, opts.red));
            color = c;
        }
        match op {
            Op::Neume { glyph, x, y, scale, .. } => {
                if neumes.contains_key(glyph) {
                    let _ = writeln!(s, "q {} 0 0 {} {} {} cm /N{glyph} Do Q", num(*scale), num(*scale), num(*x), num(*y));
                }
            }
            Op::Rect { x, y, w, h, .. } => {
                let _ = writeln!(s, "{} {} {} {} re f", num(*x), num(*y), num(*w), num(*h));
            }
            Op::Text {
                x, baseline, size, run, ..
            } => {
                if paths {
                    let mut at = *x;
                    for g in &run.glyphs {
                        if glyphs.contains_key(&(run.face, g.id)) {
                            let _ = writeln!(
                                s,
                                "q {} 0 0 {} {} {} cm /T{}_{} Do Q",
                                num(*size),
                                num(-size),
                                num(at + g.dx * size),
                                num(baseline - g.dy * size),
                                run.face,
                                g.id
                            );
                        }
                        at += g.advance * size;
                    }
                    continue;
                }
                let _ = write!(s, "BT /F{} {} Tf 1 0 0 -1 {} {} Tm ", run.face, num(*size), num(*x), num(*baseline));
                match fonts.face(run.face) {
                    None => {
                        let bytes: Vec<u8> = run.glyphs.iter().map(|g| g.id as u8).collect();
                        let _ = write!(s, "{} Tj", pdf_string(&bytes));
                    }
                    Some(f) if run.glyphs.iter().all(|g| g.dx == 0.0 && g.dy == 0.0) => {
                        s.push('[');
                        for g in &run.glyphs {
                            let _ = write!(s, "<{:04X}>", g.id);
                            let adj = (f.glyph_advance(g.id) - g.advance) * 1000.0;
                            if adj.abs() > 0.05 {
                                let _ = write!(s, " {} ", num(adj));
                            }
                        }
                        s.push_str("] TJ");
                    }
                    Some(_) => {
                        // Marks placed by the font: each glyph where shaping put it.
                        let mut at = *x;
                        for g in &run.glyphs {
                            let _ = write!(
                                s,
                                "1 0 0 -1 {} {} Tm <{:04X}> Tj ",
                                num(at + g.dx * size),
                                num(baseline - g.dy * size),
                                g.id
                            );
                            at += g.advance * size;
                        }
                    }
                }
                s.push_str(" ET\n");
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(2.5), "2.5");
        assert_eq!(num(1.23456), "1.235");
    }

    #[test]
    fn neume_paths_convert() {
        assert_eq!(neume_path("M0 0L10-5C1 2 3 4 5 6Z"), "0 0 m\n10 -5 l\n1 2 3 4 5 6 c\nh\n");
        for id in 0..60u16 {
            if let Some(o) = neuma::glyph_outline(id) {
                let p = neume_path(&o.d);
                assert!(p.ends_with("h\n"), "{id}");
            }
        }
    }

    #[test]
    fn strings_escape() {
        assert_eq!(pdf_string(b"a(b)\\\x86"), "(a\\(b\\)\\\\\\206)");
        assert_eq!(text_string("Ab"), "<FEFF00410062>");
    }
}
