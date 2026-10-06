//! The booklet's text face: loading, shaping, measuring (for neuma's lyrics too), outlines
//! for text drawn as paths, and TrueType subsetting for the PDF.
//!
//! Text is shaped with rustybuzz the way neuma measures lyrics: ligatures off, kerning on and
//! real small caps (`smcp`), so the PDF draws exactly the advances the layout used. Without a
//! font file, the PDF falls back to the standard Times faces every viewer has, measured
//! approximately.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use neuma::TextMeasure;
use neuma::score::TextStyle;
use rustybuzz::ttf_parser::{GlyphId, OutlineBuilder, Tag};
use rustybuzz::{Feature, UnicodeBuffer};

/// The four faces, in this order everywhere.
pub const REGULAR: usize = 0;
pub const ITALIC: usize = 1;
pub const BOLD: usize = 2;
pub const BOLD_ITALIC: usize = 3;

/// Where EB Garamond 12 is commonly installed (Debian and Ubuntu's fonts-ebgaramond, and a
/// user's own font folder); the TrueType files come first because every PDF viewer reads them.
const SEARCH: &[&str] = &[
    "/usr/share/fonts/truetype/ebgaramond",
    "/usr/share/fonts/opentype/ebgaramond",
    "/usr/local/share/fonts",
    "/Library/Fonts",
    "~/.local/share/fonts",
    "~/.fonts",
    "~/Library/Fonts",
];

/// Font files read into memory: regular, italic, bold and bold italic.
#[derive(Clone, Debug, Default)]
pub struct FontFiles {
    pub faces: [Option<(PathBuf, Vec<u8>)>; 4],
}

impl FontFiles {
    /// Reads the given files; a missing optional face falls back to another at draw time.
    pub fn load(paths: [Option<&Path>; 4]) -> Result<FontFiles, String> {
        let mut out = FontFiles::default();
        for (slot, p) in out.faces.iter_mut().zip(paths) {
            if let Some(p) = p {
                let data = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                if rustybuzz::Face::from_slice(&data, 0).is_none() {
                    return Err(format!("{}: not a TrueType or OpenType font", p.display()));
                }
                *slot = Some((p.to_path_buf(), data));
            }
        }
        Ok(out)
    }

    /// EB Garamond 12 from the usual places, if it is installed.
    pub fn find_default() -> Option<FontFiles> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        for dir in SEARCH {
            let dir = match dir.strip_prefix("~/") {
                Some(rest) => match &home {
                    Some(h) => h.join(rest),
                    None => continue,
                },
                None => PathBuf::from(dir),
            };
            for ext in ["ttf", "otf"] {
                let file = |style: &str| dir.join(format!("EBGaramond12-{style}.{ext}"));
                if file("Regular").is_file() {
                    let opt = |p: PathBuf| p.is_file().then_some(p);
                    let italic = opt(file("Italic"));
                    return FontFiles::load([Some(&file("Regular")), italic.as_deref(), None, None]).ok();
                }
            }
        }
        None
    }
}

/// One loaded face.
pub struct Face<'a> {
    pub data: &'a [u8],
    pub(crate) rb: rustybuzz::Face<'a>,
    pub upem: f32,
    /// Ascent and descent in ems, both positive.
    pub ascent: f32,
    pub descent: f32,
    pub cap_height: f32,
    /// PostScript-style name, letters and digits only.
    pub name: String,
    /// The family name, for SVG.
    pub family: String,
    /// CFF outlines (an `OTTO` file) rather than TrueType.
    pub cff: bool,
    pub italic_angle: f32,
    /// xMin, yMin, xMax, yMax in font units.
    pub bbox: [i16; 4],
}

impl std::fmt::Debug for Face<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Face").field("name", &self.name).finish_non_exhaustive()
    }
}

impl<'a> Face<'a> {
    fn new(data: &'a [u8]) -> Option<Face<'a>> {
        let rb = rustybuzz::Face::from_slice(data, 0)?;
        let upem = rb.units_per_em() as f32;
        let name_of = |id: u16| {
            rb.names()
                .into_iter()
                .filter(|n| n.name_id == id && n.is_unicode())
                .find_map(|n| n.to_string())
        };
        let name: String = name_of(6)
            .or_else(|| name_of(4))
            .unwrap_or_else(|| "Font".into())
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let family = name_of(16).or_else(|| name_of(1)).unwrap_or_else(|| name.clone());
        let bb = rb.global_bounding_box();
        Some(Face {
            data,
            upem,
            ascent: rb.ascender() as f32 / upem,
            descent: -(rb.descender() as f32) / upem,
            cap_height: rb.capital_height().map_or(0.7, |c| c as f32 / upem),
            name: if name.is_empty() { "Font".into() } else { name },
            family,
            cff: data.starts_with(b"OTTO"),
            italic_angle: rb.italic_angle(),
            bbox: [bb.x_min, bb.y_min, bb.x_max, bb.y_max],
            rb,
        })
    }

    pub fn glyph_advance(&self, id: u16) -> f32 {
        self.rb.glyph_hor_advance(GlyphId(id)).unwrap_or(0) as f32 / self.upem
    }

    pub fn has_char(&self, c: char) -> bool {
        self.rb.glyph_index(c).is_some()
    }

    /// The glyph's outline in ems, y up.
    pub fn outline(&self, id: u16) -> Vec<Seg> {
        let mut b = Outline {
            segs: Vec::new(),
            k: 1.0 / self.upem,
            at: (0.0, 0.0),
        };
        self.rb.outline_glyph(GlyphId(id), &mut b);
        b.segs
    }
}

/// A path segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

struct Outline {
    segs: Vec<Seg>,
    k: f32,
    at: (f32, f32),
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.at = (x * self.k, y * self.k);
        self.segs.push(Seg::Move(self.at.0, self.at.1));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.at = (x * self.k, y * self.k);
        self.segs.push(Seg::Line(self.at.0, self.at.1));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = self.at;
        let (qx, qy) = (x1 * self.k, y1 * self.k);
        let (x, y) = (x * self.k, y * self.k);
        let c1 = (x0 + 2.0 / 3.0 * (qx - x0), y0 + 2.0 / 3.0 * (qy - y0));
        let c2 = (x + 2.0 / 3.0 * (qx - x), y + 2.0 / 3.0 * (qy - y));
        self.segs.push(Seg::Cubic(c1.0, c1.1, c2.0, c2.1, x, y));
        self.at = (x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let k = self.k;
        self.at = (x * k, y * k);
        self.segs.push(Seg::Cubic(x1 * k, y1 * k, x2 * k, y2 * k, x * k, y * k));
    }
    fn close(&mut self) {
        self.segs.push(Seg::Close);
    }
}

/// A glyph placed by shaping, in ems.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// The glyph id, or the WinAnsi code for the standard faces.
    pub id: u16,
    pub advance: f32,
    pub dx: f32,
    pub dy: f32,
    /// The text this glyph stands for (empty after the first glyph of a cluster).
    pub text: String,
}

/// A shaped run of one face.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaped {
    /// The face it was set in, after falling back for a missing face.
    pub face: usize,
    pub glyphs: Vec<Glyph>,
    /// Total advance in ems.
    pub width: f32,
    /// Set with the font's small capitals.
    pub small_caps: bool,
}

/// The faces text is set in.
#[derive(Debug)]
pub struct Fonts<'a> {
    /// `None` uses the PDF standard Times faces.
    faces: Option<[Option<Face<'a>>; 4]>,
    widths: RefCell<HashMap<(usize, bool, String), f32>>,
    /// Characters shaped to the missing glyph.
    missing: RefCell<BTreeSet<char>>,
}

/// The standard faces' names, by face index.
pub const STANDARD: [&str; 4] = ["Times-Roman", "Times-Italic", "Times-Bold", "Times-BoldItalic"];

fn features(small_caps: bool) -> Vec<Feature> {
    let mut f: Vec<Feature> = ["liga", "clig", "dlig", "hlig"]
        .iter()
        .map(|t| Feature::new(Tag::from_bytes_lossy(t.as_bytes()), 0, ..))
        .collect();
    f.push(Feature::new(Tag::from_bytes(b"kern"), 1, ..));
    if small_caps {
        f.push(Feature::new(Tag::from_bytes(b"smcp"), 1, ..));
    }
    f
}

impl<'a> Fonts<'a> {
    pub fn new(files: &'a FontFiles) -> Fonts<'a> {
        if files.faces[REGULAR].is_none() {
            return Fonts::standard();
        }
        let faces = [0, 1, 2, 3].map(|i| files.faces[i].as_ref().and_then(|(_, d)| Face::new(d)));
        Fonts {
            faces: Some(faces),
            widths: RefCell::default(),
            missing: RefCell::default(),
        }
    }

    /// The standard Times faces, measured approximately.
    pub fn standard() -> Fonts<'static> {
        Fonts {
            faces: None,
            widths: RefCell::default(),
            missing: RefCell::default(),
        }
    }

    /// Characters the face has no glyph for, seen so far; they print as the missing glyph.
    pub fn missing(&self) -> Vec<char> {
        self.missing.borrow().iter().copied().collect()
    }

    pub fn is_standard(&self) -> bool {
        self.faces.is_none()
    }

    /// The face a style is drawn in: the nearest one loaded.
    pub fn resolve(&self, italic: bool, bold: bool) -> usize {
        let want = match (bold, italic) {
            (false, false) => REGULAR,
            (false, true) => ITALIC,
            (true, false) => BOLD,
            (true, true) => BOLD_ITALIC,
        };
        let Some(faces) = &self.faces else { return want };
        let order: &[usize] = match want {
            BOLD_ITALIC => &[BOLD_ITALIC, ITALIC, BOLD, REGULAR],
            BOLD => &[BOLD, REGULAR],
            ITALIC => &[ITALIC, REGULAR],
            _ => &[REGULAR],
        };
        order.iter().copied().find(|i| faces[*i].is_some()).unwrap_or(REGULAR)
    }

    pub fn face(&self, i: usize) -> Option<&Face<'a>> {
        self.faces.as_ref().and_then(|f| f[i].as_ref())
    }

    /// Ascent and descent in ems.
    pub fn vertical(&self, face: usize) -> (f32, f32) {
        self.face(face).map_or((0.75, 0.25), |f| (f.ascent, f.descent))
    }

    pub fn has_char(&self, c: char) -> bool {
        match self.face(REGULAR) {
            Some(f) => f.has_char(c),
            None => winansi(c).is_some(),
        }
    }

    /// The face a face index is drawn in: itself if loaded, else the nearest loaded one.
    pub fn resolve_index(&self, face: usize) -> usize {
        self.resolve(face == ITALIC || face == BOLD_ITALIC, face == BOLD || face == BOLD_ITALIC)
    }

    /// Shapes `text` in a face, or in the nearest loaded one when that face isn't.
    pub fn shape(&self, text: &str, face: usize, small_caps: bool) -> Shaped {
        let face = self.resolve_index(face);
        let Some(f) = self.face(face) else {
            self.missing
                .borrow_mut()
                .extend(text.chars().filter(|c| !c.is_whitespace() && winansi(*c).is_none()));
            return standard_shape(text, face, small_caps);
        };
        let mut buf = UnicodeBuffer::new();
        buf.push_str(text);
        let out = rustybuzz::shape(&f.rb, &features(small_caps), buf);
        let infos = out.glyph_infos();
        let pos = out.glyph_positions();
        let mut glyphs = Vec::with_capacity(infos.len());
        let mut width = 0.0;
        for (i, (info, p)) in infos.iter().zip(pos).enumerate() {
            let start = info.cluster as usize;
            let first = i == 0 || infos[i - 1].cluster != info.cluster;
            // The cluster runs to the next cluster start in the text, wherever its glyph is:
            // right-to-left runs list clusters in descending order.
            let end = infos
                .iter()
                .map(|n| n.cluster as usize)
                .filter(|c| *c > start)
                .min()
                .unwrap_or(text.len());
            let chunk = if first { text.get(start..end).unwrap_or("") } else { "" };
            if info.glyph_id == 0 && first {
                self.missing.borrow_mut().extend(chunk.chars().filter(|c| !c.is_whitespace()));
            }
            let advance = p.x_advance as f32 / f.upem;
            width += advance;
            glyphs.push(Glyph {
                id: info.glyph_id as u16,
                advance,
                dx: p.x_offset as f32 / f.upem,
                dy: p.y_offset as f32 / f.upem,
                text: chunk.to_string(),
            });
        }
        Shaped {
            face,
            glyphs,
            width,
            small_caps,
        }
    }

    /// The advance of `text` in ems, cached.
    pub fn width(&self, text: &str, face: usize, small_caps: bool) -> f32 {
        let face = self.resolve_index(face);
        let key = (face, small_caps, text.to_string());
        if let Some(w) = self.widths.borrow().get(&key) {
            return *w;
        }
        let w = self.shape(text, face, small_caps).width;
        self.widths.borrow_mut().insert(key, w);
        w
    }
}

impl TextMeasure for Fonts<'_> {
    fn advance(&self, text: &str, style: TextStyle) -> f32 {
        self.width(text, self.resolve(style.italic, style.bold), style.small_caps)
    }

    /// The ascent is padded by a quarter em, so lyrics hang a little further below the staff
    /// than neuma's default: in print, capitals and ascenders otherwise come close to the
    /// bottom line.
    fn vertical(&self, style: TextStyle) -> (f32, f32) {
        let (a, d) = Fonts::vertical(self, self.resolve(style.italic, style.bold));
        (a + LYRIC_PAD, d)
    }

    fn has_face(&self, style: TextStyle) -> bool {
        let want = match (style.bold, style.italic) {
            (false, false) => REGULAR,
            (false, true) => ITALIC,
            (true, false) => BOLD,
            (true, true) => BOLD_ITALIC,
        };
        self.faces.is_none() || self.resolve(style.italic, style.bold) == want
    }
}

/// Extra ascent reported to neuma for lyrics, in ems.
const LYRIC_PAD: f32 = 0.25;

/// The WinAnsi code for `c`, as the standard faces encode text.
pub fn winansi(c: char) -> Option<u8> {
    let u = c as u32;
    if (0x20..0x7F).contains(&u) || (0xA0..=0xFF).contains(&u) {
        return Some(u as u8);
    }
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        _ => return None,
    })
}

fn standard_shape(text: &str, face: usize, small_caps: bool) -> Shaped {
    let style = TextStyle {
        italic: face == ITALIC || face == BOLD_ITALIC,
        bold: face >= BOLD,
        small_caps,
        ..TextStyle::REGULAR
    };
    let m = neuma::ApproxMeasure;
    let mut glyphs = Vec::new();
    let mut width = 0.0;
    for c in text.chars() {
        let mut s = [0u8; 4];
        let advance = m.advance(c.encode_utf8(&mut s), style);
        width += advance;
        glyphs.push(Glyph {
            id: winansi(c).unwrap_or(b'?') as u16,
            advance,
            dx: 0.0,
            dy: 0.0,
            text: c.to_string(),
        });
    }
    Shaped {
        face,
        glyphs,
        width,
        small_caps,
    }
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(o)?, *d.get(o + 1)?]))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*d.get(o)?, *d.get(o + 1)?, *d.get(o + 2)?, *d.get(o + 3)?]))
}

fn checksum(d: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in d.chunks(4) {
        let mut b = [0u8; 4];
        b[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(b));
    }
    sum
}

/// A TrueType font with every glyph outside `keep` (and the components they use) emptied,
/// keeping glyph ids as they are, so a PDF can use them directly. `None` for a font this
/// can't subset (CFF outlines, or a malformed file).
pub fn subset_truetype(data: &[u8], keep: &BTreeSet<u16>) -> Option<Vec<u8>> {
    let num_tables = u16_at(data, 4)? as usize;
    let mut tables: HashMap<[u8; 4], &[u8]> = HashMap::new();
    for i in 0..num_tables {
        let r = 12 + i * 16;
        let tag: [u8; 4] = data.get(r..r + 4)?.try_into().ok()?;
        let off = u32_at(data, r + 8)? as usize;
        let len = u32_at(data, r + 12)? as usize;
        tables.insert(tag, data.get(off..off.checked_add(len)?)?);
    }
    let head = *tables.get(b"head")?;
    let long = u16_at(head, 50)? == 1;
    let num_glyphs = u16_at(tables.get(b"maxp")?, 4)? as usize;
    let loca = *tables.get(b"loca")?;
    let glyf = *tables.get(b"glyf")?;
    let offset = |g: usize| -> Option<usize> {
        if long {
            u32_at(loca, g * 4).map(|v| v as usize)
        } else {
            u16_at(loca, g * 2).map(|v| v as usize * 2)
        }
    };
    let glyph = |g: usize| -> Option<&[u8]> { glyf.get(offset(g)?..offset(g + 1)?) };
    // Close over composite components.
    let mut used: BTreeSet<u16> = keep.iter().copied().filter(|g| (*g as usize) < num_glyphs).collect();
    used.insert(0);
    let mut todo: Vec<u16> = used.iter().copied().collect();
    while let Some(g) = todo.pop() {
        let d = glyph(g as usize)?;
        if d.len() < 10 || (u16_at(d, 0)? as i16) >= 0 {
            continue;
        }
        let mut p = 10;
        loop {
            let flags = u16_at(d, p)?;
            let comp = u16_at(d, p + 2)?;
            if (comp as usize) < num_glyphs && used.insert(comp) {
                todo.push(comp);
            }
            p += 4 + if flags & 1 != 0 { 4 } else { 2 };
            p += if flags & 8 != 0 {
                2
            } else if flags & 0x40 != 0 {
                4
            } else if flags & 0x80 != 0 {
                8
            } else {
                0
            };
            if flags & 0x20 == 0 {
                break;
            }
        }
    }
    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity((num_glyphs + 1) * 4);
    for g in 0..num_glyphs {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        if used.contains(&(g as u16)) {
            new_glyf.extend_from_slice(glyph(g)?);
            while new_glyf.len() % 4 != 0 {
                new_glyf.push(0);
            }
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
    let mut new_head = head.to_vec();
    new_head.get_mut(8..12)?.copy_from_slice(&[0; 4]);
    new_head.get_mut(50..52)?.copy_from_slice(&1u16.to_be_bytes());
    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    for tag in [
        b"OS/2", b"cmap", b"cvt ", b"fpgm", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"prep",
    ] {
        let body = match tag {
            b"glyf" => new_glyf.clone(),
            b"loca" => new_loca.clone(),
            b"head" => new_head.clone(),
            t => match tables.get(t) {
                Some(d) => d.to_vec(),
                None => continue,
            },
        };
        out_tables.push((*tag, body));
    }
    let n = out_tables.len() as u16;
    let mut pow = 1u16;
    let mut log = 0u16;
    while pow * 2 <= n {
        pow *= 2;
        log += 1;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&(pow * 16).to_be_bytes());
    out.extend_from_slice(&log.to_be_bytes());
    out.extend_from_slice(&(n * 16 - pow * 16).to_be_bytes());
    let mut offset = 12 + 16 * out_tables.len();
    let mut head_at = 0;
    for (tag, body) in &out_tables {
        if tag == b"head" {
            head_at = offset;
        }
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(body).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        offset += body.len().div_ceil(4) * 4;
    }
    for (_, body) in &out_tables {
        out.extend_from_slice(body);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let adjust = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
    out.get_mut(head_at + 8..head_at + 12)?.copy_from_slice(&adjust.to_be_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_faces_fall_back_and_encode() {
        let f = Fonts::standard();
        assert!(f.is_standard());
        let s = f.shape("Lórd †", REGULAR, false);
        assert_eq!(s.glyphs.len(), 6);
        assert_eq!(s.glyphs[1].id, 0xF3);
        assert_eq!(s.glyphs[5].id, 0x86);
        assert!(s.width > 1.0);
        assert_eq!(winansi('℣'), None);
    }

    /// A face that isn't loaded falls back to one that is, so nothing is drawn in a face
    /// the PDF doesn't embed.
    #[test]
    fn missing_faces_resolve() {
        let path = Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        if !path.is_file() {
            return;
        }
        let files = FontFiles::load([Some(path), None, None, None]).unwrap();
        let fonts = Fonts::new(&files);
        for face in [ITALIC, BOLD, BOLD_ITALIC] {
            assert_eq!(fonts.shape("Compline", face, false).face, REGULAR);
        }
        assert!(fonts.width("Compline", ITALIC, false) > 0.0);
    }

    /// Right-to-left clusters come in descending order; each glyph still gets its own text,
    /// and characters with no glyph are reported.
    #[test]
    fn rtl_clusters_and_missing_glyphs() {
        let path = Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        if !path.is_file() {
            return;
        }
        let files = FontFiles::load([Some(path), None, None, None]).unwrap();
        let fonts = Fonts::new(&files);
        let s = fonts.shape("abc שָׁלוֹם", REGULAR, false);
        let text: String = s.glyphs.iter().map(|g| g.text.as_str()).collect();
        let mut sorted: Vec<char> = text.chars().collect();
        let mut want: Vec<char> = "abc שָׁלוֹם".chars().collect();
        sorted.sort();
        want.sort();
        assert_eq!(sorted, want);
        assert!(fonts.missing().is_empty());
        let s = fonts.shape("a\u{4E2D}", REGULAR, false);
        assert_eq!(s.glyphs[1].id, 0);
        assert_eq!(fonts.missing(), vec!['\u{4E2D}']);
    }

    /// With EB Garamond installed, shaping kerns and measuring agrees with shaping.
    #[test]
    fn real_font_when_installed() {
        let Some(files) = FontFiles::find_default() else { return };
        let fonts = Fonts::new(&files);
        assert!(!fonts.is_standard());
        let s = fonts.shape("office", REGULAR, false);
        // Ligatures are off: one glyph per letter.
        assert_eq!(s.glyphs.len(), 6);
        assert_eq!(s.glyphs.iter().map(|g| g.text.as_str()).collect::<String>(), "office");
        assert_eq!(fonts.width("office", REGULAR, false), s.width);
        let sc = fonts.shape("abc", REGULAR, true);
        assert_ne!(sc.glyphs[0].id, s.glyphs[0].id);
        let face = fonts.face(REGULAR).unwrap();
        assert!(!face.outline(s.glyphs[0].id).is_empty());
        if !face.cff {
            let keep: BTreeSet<u16> = s.glyphs.iter().map(|g| g.id).collect();
            let sub = subset_truetype(face.data, &keep).unwrap();
            assert!(sub.len() < face.data.len() / 4);
            let parsed = rustybuzz::Face::from_slice(&sub, 0).unwrap();
            assert_eq!(parsed.number_of_glyphs(), face.rb.number_of_glyphs());
            let mut b = Outline {
                segs: Vec::new(),
                k: 1.0,
                at: (0.0, 0.0),
            };
            assert!(parsed.outline_glyph(GlyphId(s.glyphs[0].id), &mut b).is_some());
            assert_eq!(checksum(&sub), 0xB1B0_AFBA);
        }
    }
}
