//! `neuma-metrics --regular R.otf [--italic I.otf] [--bold B.otf] [--bold-italic BI.otf] -o OUT`
//!
//! Shapes every character, and every pair of characters lyrics are likely to set, with the
//! features renderers use for lyrics (`font-variant-ligatures: none`, kerning on), and writes
//! a neuma metrics table.

use std::process::ExitCode;

use neuma::metrics::{Face, MetricsTable};
use rustybuzz::{Feature, UnicodeBuffer, ttf_parser::Tag};
use sha2::{Digest, Sha256};

/// Characters to record advances for, when the font has them.
fn coverage() -> impl Iterator<Item = char> {
    let ranges: [(u32, u32); 7] = [
        (0x20, 0x7E),
        (0xA0, 0x24F),
        (0x300, 0x36F),
        // Greek and Cyrillic, which some chant texts and initials use.
        (0x370, 0x45F),
        (0x1E00, 0x1EFF),
        (0x2010, 0x205E),
        (0x2100, 0x2135),
    ];
    ranges.into_iter().flat_map(|(a, b)| (a..=b).filter_map(char::from_u32))
}

/// Characters to measure kerning between: letters, digits and punctuation common in lyrics.
fn kerning_set(face: &rustybuzz::Face) -> Vec<char> {
    let mut out: Vec<char> = (0x21u32..=0x7E)
        .chain(0xC0..=0xFF)
        .chain([0x152, 0x153, 0x1FC, 0x1FD, 0x2019, 0x201C, 0x201D, 0x2018])
        .filter_map(char::from_u32)
        .filter(|c| face.glyph_index(*c).is_some())
        .collect();
    out.push(' ');
    out
}

/// The size of the capital a browser draws for a small capital the font lacks.
const SYNTHETIC_SMALL_CAP: f32 = 0.7;

fn features(small_caps: bool) -> Vec<Feature> {
    let mut f: Vec<Feature> = ["liga", "clig", "dlig", "hlig", "calt"]
        .iter()
        .map(|t| Feature::new(Tag::from_bytes_lossy(t.as_bytes()), 0, ..))
        .collect();
    f.push(Feature::new(Tag::from_bytes(b"kern"), 1, ..));
    if small_caps {
        f.push(Feature::new(Tag::from_bytes(b"smcp"), 1, ..));
    }
    f
}

fn shape(face: &rustybuzz::Face, text: &str, feats: &[Feature]) -> i32 {
    let mut buf = UnicodeBuffer::new();
    buf.push_str(text);
    let out = rustybuzz::shape(face, feats, buf);
    out.glyph_positions().iter().map(|p| p.x_advance).sum()
}

fn build_face(path: &str, italic: bool, bold: bool) -> Result<Face, String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let hash: [u8; 32] = Sha256::digest(&data).into();
    let face = rustybuzz::Face::from_slice(&data, 0).ok_or_else(|| format!("{path}: not a font"))?;
    let upem = face.units_per_em() as f32;
    let ascent = face.ascender() as f32 / upem;
    let descent = -(face.descender() as f32) / upem;
    let mut out = Face::new(italic, bold, hash, ascent, descent);
    let plain = features(false);
    let smcp = features(true);
    // Without small capitals of its own, a browser draws a capital at this size instead
    // (Chromium's and WebKit's synthesis).
    let has_smcp = face
        .tables()
        .gsub
        .is_some_and(|g| g.features.find(Tag::from_bytes(b"smcp")).is_some());
    for c in coverage() {
        if face.glyph_index(c).is_none() {
            continue;
        }
        let s = c.to_string();
        out.set_advance(c, shape(&face, &s, &plain) as f32 / upem);
        if c.is_lowercase() {
            let em = if has_smcp {
                shape(&face, &s, &smcp) as f32 / upem
            } else {
                SYNTHETIC_SMALL_CAP * shape(&face, &c.to_uppercase().to_string(), &plain) as f32 / upem
            };
            out.set_small_cap(c, em);
        }
    }
    let set = kerning_set(&face);
    let single: Vec<i32> = set.iter().map(|c| shape(&face, &c.to_string(), &plain)).collect();
    let mut pairs = 0;
    for (i, a) in set.iter().enumerate() {
        for (j, b) in set.iter().enumerate() {
            let both = shape(&face, &format!("{a}{b}"), &plain);
            let k = both - single[i] - single[j];
            if k != 0 {
                out.set_kern(*a, *b, k as f32 / upem);
                pairs += 1;
            }
        }
    }
    eprintln!("{path}: {pairs} kerning pairs");
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut faces = Vec::new();
    let mut out_path = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let style = match a.as_str() {
            "--regular" => Some((false, false)),
            "--italic" => Some((true, false)),
            "--bold" => Some((false, true)),
            "--bold-italic" => Some((true, true)),
            "-o" => {
                out_path = it.next().cloned();
                None
            }
            _ => {
                eprintln!("usage: neuma-metrics --regular R.otf [--italic I.otf] [--bold B.otf] [--bold-italic BI.otf] -o OUT");
                return ExitCode::from(2);
            }
        };
        if let Some((italic, bold)) = style {
            let Some(path) = it.next() else {
                eprintln!("neuma-metrics: {a} needs a font path");
                return ExitCode::from(2);
            };
            match build_face(path, italic, bold) {
                Ok(f) => faces.push(f),
                Err(e) => {
                    eprintln!("neuma-metrics: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    let Some(out_path) = out_path else {
        eprintln!("neuma-metrics: -o OUT is required");
        return ExitCode::from(2);
    };
    let bytes = MetricsTable { faces }.to_bytes();
    if let Err(e) = std::fs::write(&out_path, &bytes) {
        eprintln!("neuma-metrics: {out_path}: {e}");
        return ExitCode::FAILURE;
    }
    eprintln!("wrote {out_path} ({} bytes)", bytes.len());
    ExitCode::SUCCESS
}
