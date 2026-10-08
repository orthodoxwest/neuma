//! A compact table of font metrics, built offline by `neuma-metrics` and read here with no
//! dependencies. It measures text the way the renderers set lyrics: per-character advances,
//! pair kerning, and small-cap advances, with ligatures off
//! (docs/DESIGN.md, "Lyrics and text measurement").
//!
//! Format (little-endian): `b"NMET"`, `u16` version (1), `u16` face count, then per face:
//! `u8` style flags (1 italic, 2 bold), 32-byte SHA-256 of the font file, `f32` ascent and
//! descent in ems, then three sorted arrays, each a `u32` count followed by entries:
//! advances `(u32 char, f32 em)`, small-cap advances `(u32 char, f32 em)` and kerning pairs
//! `(u32 left, u32 right, f32 em)`.

use crate::score::TextStyle;
use crate::text::TextMeasure;

/// One face's metrics in a [`MetricsTable`]. Its constructor and setters are for the table
/// builder (`neuma-metrics`); the stability contract is the `NMET` format version.
#[derive(Clone, Debug)]
pub struct FaceMetrics {
    /// Whether the face is italic.
    pub italic: bool,
    /// Whether the face is bold.
    pub bold: bool,
    /// The SHA-256 of the font file the face was measured from, so a table can be checked
    /// against the fonts a renderer draws with.
    pub sha256: [u8; 32],
    /// The face's ascent above the baseline, in ems.
    pub ascent: f32,
    /// The face's descent below the baseline, in ems (positive).
    pub descent: f32,
    advances: Vec<(u32, f32)>,
    small_caps: Vec<(u32, f32)>,
    kerning: Vec<(u32, u32, f32)>,
    direct: Direct,
}

impl PartialEq for FaceMetrics {
    fn eq(&self, o: &FaceMetrics) -> bool {
        self.italic == o.italic
            && self.bold == o.bold
            && self.sha256 == o.sha256
            && self.ascent == o.ascent
            && self.descent == o.descent
            && self.advances == o.advances
            && self.small_caps == o.small_caps
            && self.kerning == o.kerning
    }
}

/// Code points below this are looked up directly rather than searched for: all of Latin.
const DIRECT: usize = 0x250;

/// The sorted arrays' entries for the first [`DIRECT`] code points, at hand by index, as
/// measuring lyrics looks them up over and over. Empty until the arrays are sorted.
#[derive(Clone, Debug, Default)]
struct Direct {
    /// Each code point's advance, as [`FaceMetrics::advance_of`] finds it.
    advances: Vec<f32>,
    /// Where the kerning pairs with each code point on the left are: `kern[c]..kern[c + 1]`.
    kern: Vec<u32>,
}

impl FaceMetrics {
    #[doc(hidden)]
    pub fn new(italic: bool, bold: bool, sha256: [u8; 32], ascent: f32, descent: f32) -> FaceMetrics {
        FaceMetrics {
            italic,
            bold,
            sha256,
            ascent,
            descent,
            advances: Vec::new(),
            small_caps: Vec::new(),
            kerning: Vec::new(),
            direct: Direct::default(),
        }
    }

    #[doc(hidden)]
    pub fn set_advance(&mut self, c: char, em: f32) {
        self.advances.push((c as u32, em));
        self.direct = Direct::default();
    }

    #[doc(hidden)]
    pub fn set_small_cap(&mut self, c: char, em: f32) {
        self.small_caps.push((c as u32, em));
    }

    #[doc(hidden)]
    pub fn set_kern(&mut self, left: char, right: char, em: f32) {
        self.kerning.push((left as u32, right as u32, em));
        self.direct = Direct::default();
    }

    fn sort(&mut self) {
        self.advances.sort_by_key(|e| e.0);
        self.advances.dedup_by_key(|e| e.0);
        self.small_caps.sort_by_key(|e| e.0);
        self.small_caps.dedup_by_key(|e| e.0);
        self.kerning.sort_by_key(|e| (e.0, e.1));
        self.kerning.dedup_by_key(|e| (e.0, e.1));
        self.direct = Direct::default();
        let advances = (0..DIRECT as u32)
            .map(|c| char::from_u32(c).map_or(0.5, |ch| self.advance_of(ch, false)))
            .collect();
        let kern = (0..=DIRECT as u32)
            .map(|c| self.kerning.partition_point(|e| e.0 < c) as u32)
            .collect();
        self.direct = Direct { advances, kern };
    }

    fn advance_of(&self, c: char, small_caps: bool) -> f32 {
        let key = c as u32;
        if small_caps && let Ok(i) = self.small_caps.binary_search_by_key(&key, |e| e.0) {
            return self.small_caps[i].1;
        }
        if let Some(&a) = self.direct.advances.get(key as usize) {
            return a;
        }
        match self.advances.binary_search_by_key(&key, |e| e.0) {
            Ok(i) => self.advances[i].1,
            // Combining marks take no room; anything else missing gets an average width.
            Err(_) if ('\u{0300}'..='\u{036F}').contains(&c) => 0.0,
            Err(_) => 0.5,
        }
    }

    fn kern(&self, a: char, b: char) -> f32 {
        let i = a as usize;
        if let (Some(&from), Some(&to)) = (self.direct.kern.get(i), self.direct.kern.get(i + 1)) {
            let pairs = &self.kerning[from as usize..to as usize];
            return match pairs.binary_search_by_key(&(b as u32), |e| e.1) {
                Ok(i) => pairs[i].2,
                Err(_) => 0.0,
            };
        }
        let key = (a as u32, b as u32);
        match self.kerning.binary_search_by_key(&key, |e| (e.0, e.1)) {
            Ok(i) => self.kerning[i].2,
            Err(_) => 0.0,
        }
    }

    /// The advance of `text` in this face, in ems: each character's advance, plus kerning
    /// between each pair unless in small caps. A character the face lacks counts 0.5 em, a
    /// combining mark nothing.
    #[must_use]
    pub fn advance(&self, text: &str, small_caps: bool) -> f32 {
        let mut w = 0.0;
        let mut prev: Option<char> = None;
        for c in text.chars() {
            w += self.advance_of(c, small_caps);
            if !small_caps && let Some(p) = prev {
                w += self.kern(p, c);
            }
            prev = Some(c);
        }
        w
    }

    /// [`advance`](Self::advance) of each prefix of `text` that ends a character: its
    /// running sum, pushed onto `out`.
    fn prefix_advances(&self, text: &str, small_caps: bool, out: &mut Vec<f32>) {
        let mut w = 0.0;
        let mut prev: Option<char> = None;
        for c in text.chars() {
            w += self.advance_of(c, small_caps);
            if !small_caps && let Some(p) = prev {
                w += self.kern(p, c);
            }
            prev = Some(c);
            out.push(w);
        }
    }
}

/// Font metrics for measuring lyrics: a [`TextMeasure`] read from a table that
/// `neuma-metrics` builds from font files. The built-in EB Garamond tables are
/// [`LyricFont::metrics`](crate::LyricFont::metrics).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricsTable {
    /// The faces measured, one per style (regular, italic, bold, bold italic) at most.
    pub faces: Vec<FaceMetrics>,
}

/// Why a metrics table can't be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MetricsError {
    /// The bytes don't start with `NMET`.
    BadMagic,
    /// The table is of a format version this reader doesn't know.
    UnsupportedVersion(u16),
    /// The bytes end before the table does.
    Truncated,
}

impl std::fmt::Display for MetricsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricsError::BadMagic => write!(f, "not a neuma metrics table"),
            MetricsError::UnsupportedVersion(v) => write!(f, "unsupported metrics table version {v}"),
            MetricsError::Truncated => write!(f, "metrics table is truncated"),
        }
    }
}

impl std::error::Error for MetricsError {}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], MetricsError> {
        let s = self.b.get(self.at..self.at + n).ok_or(MetricsError::Truncated)?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, MetricsError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, MetricsError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, MetricsError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn f32(&mut self) -> Result<f32, MetricsError> {
        Ok(f32::from_bits(self.u32()?))
    }
}

impl MetricsTable {
    /// Reads a table in the `NMET` format (see the module docs), as `neuma-metrics` writes it.
    ///
    /// # Errors
    ///
    /// [`MetricsError::BadMagic`] if `bytes` isn't a metrics table,
    /// [`MetricsError::UnsupportedVersion`] if it is of a newer format, and
    /// [`MetricsError::Truncated`] if it ends early.
    pub fn from_bytes(bytes: &[u8]) -> Result<MetricsTable, MetricsError> {
        let mut r = Reader { b: bytes, at: 0 };
        if r.take(4)? != b"NMET" {
            return Err(MetricsError::BadMagic);
        }
        let version = r.u16()?;
        if version != 1 {
            return Err(MetricsError::UnsupportedVersion(version));
        }
        let count = r.u16()?;
        let mut faces = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let flags = r.u8()?;
            let mut sha256 = [0u8; 32];
            sha256.copy_from_slice(r.take(32)?);
            let ascent = r.f32()?;
            let descent = r.f32()?;
            let mut face = FaceMetrics::new(flags & 1 != 0, flags & 2 != 0, sha256, ascent, descent);
            for _ in 0..r.u32()? {
                face.advances.push((r.u32()?, r.f32()?));
            }
            for _ in 0..r.u32()? {
                face.small_caps.push((r.u32()?, r.f32()?));
            }
            for _ in 0..r.u32()? {
                face.kerning.push((r.u32()?, r.u32()?, r.f32()?));
            }
            face.sort();
            faces.push(face);
        }
        Ok(MetricsTable { faces })
    }

    #[doc(hidden)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"NMET");
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&(self.faces.len() as u16).to_le_bytes());
        for f in &self.faces {
            let mut f = f.clone();
            f.sort();
            out.push(u8::from(f.italic) | (u8::from(f.bold) << 1));
            out.extend_from_slice(&f.sha256);
            out.extend_from_slice(&f.ascent.to_le_bytes());
            out.extend_from_slice(&f.descent.to_le_bytes());
            out.extend_from_slice(&(f.advances.len() as u32).to_le_bytes());
            for (c, a) in &f.advances {
                out.extend_from_slice(&c.to_le_bytes());
                out.extend_from_slice(&a.to_le_bytes());
            }
            out.extend_from_slice(&(f.small_caps.len() as u32).to_le_bytes());
            for (c, a) in &f.small_caps {
                out.extend_from_slice(&c.to_le_bytes());
                out.extend_from_slice(&a.to_le_bytes());
            }
            out.extend_from_slice(&(f.kerning.len() as u32).to_le_bytes());
            for (a, b, k) in &f.kerning {
                out.extend_from_slice(&a.to_le_bytes());
                out.extend_from_slice(&b.to_le_bytes());
                out.extend_from_slice(&k.to_le_bytes());
            }
        }
        out
    }

    /// The face for `style`, if the table has one.
    fn face(&self, style: TextStyle) -> Option<&FaceMetrics> {
        self.faces.iter().find(|f| f.italic == style.italic && f.bold == style.bold)
    }

    /// The face to measure `style` with: its own, else the same slant without bold, else any.
    fn fallback(&self, style: TextStyle) -> Option<&FaceMetrics> {
        self.face(style)
            .or_else(|| self.faces.iter().find(|f| f.italic == style.italic && !f.bold))
            .or_else(|| self.faces.iter().find(|f| !f.italic && !f.bold))
            .or_else(|| self.faces.first())
    }
}

impl TextMeasure for MetricsTable {
    fn advance(&self, text: &str, style: TextStyle) -> f32 {
        match self.fallback(style) {
            Some(f) => f.advance(text, style.small_caps),
            None => text.chars().count() as f32 * 0.5,
        }
    }

    fn prefix_advances(&self, text: &str, style: TextStyle, out: &mut Vec<f32>) {
        match self.fallback(style) {
            Some(f) => f.prefix_advances(text, style.small_caps, out),
            None => out.extend((1..=text.chars().count()).map(|n| n as f32 * 0.5)),
        }
    }

    fn vertical(&self, style: TextStyle) -> (f32, f32) {
        self.fallback(style).map_or((0.8, 0.25), |f| (f.ascent, f.descent))
    }

    fn has_face(&self, style: TextStyle) -> bool {
        self.face(style).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_advances_are_the_prefixes_advances() {
        let texts = ["", "AVa", "Dóminus", "fflux, Tè\u{301}", "\u{1d11e}ǽ ΚύριΕ"];
        let measures: [&dyn TextMeasure; 3] = [crate::LyricFont::Google.metrics(), &MetricsTable::default(), &crate::ApproxMeasure];
        for m in measures {
            for t in texts {
                let r = TextStyle::REGULAR;
                let styles = [
                    r,
                    TextStyle { italic: true, ..r },
                    TextStyle { small_caps: true, ..r },
                    TextStyle { bold: true, ..r },
                ];
                for style in styles {
                    let mut out = Vec::new();
                    m.prefix_advances(t, style, &mut out);
                    let each: Vec<f32> = t.char_indices().map(|(k, c)| m.advance(&t[..k + c.len_utf8()], style)).collect();
                    assert_eq!(
                        out.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                        each.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                        "{t:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn round_trip_and_measure() {
        let mut f = FaceMetrics::new(false, false, [7; 32], 0.8, 0.25);
        f.set_advance('A', 0.7);
        f.set_advance('V', 0.7);
        f.set_advance('a', 0.45);
        f.set_small_cap('a', 0.55);
        f.set_kern('A', 'V', -0.1);
        let t = MetricsTable { faces: vec![f] };
        let back = MetricsTable::from_bytes(&t.to_bytes()).unwrap();
        assert_eq!(back.faces.len(), 1);
        assert!((back.advance("AV", TextStyle::REGULAR) - 1.3).abs() < 1e-6);
        let sc = TextStyle {
            small_caps: true,
            ..TextStyle::REGULAR
        };
        assert!((back.advance("a", sc) - 0.55).abs() < 1e-6);
        assert!(!back.has_face(TextStyle {
            italic: true,
            ..TextStyle::REGULAR
        }));
        assert_eq!(MetricsTable::from_bytes(b"nope"), Err(MetricsError::BadMagic));
    }
}
