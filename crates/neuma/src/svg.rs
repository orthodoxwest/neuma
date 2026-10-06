//! SVG output. Every fill is `currentColor` and every element has a role class, so pages can
//! theme staff, notes and rubrics (and dark mode) with CSS alone. Coordinates are written with
//! exactly two decimals.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::display::{DisplayList, Item, TextRole};
use crate::glyphs::GlyphId;
use crate::layout::Layout;

#[derive(Clone, Debug, PartialEq)]
pub struct SvgOptions {
    /// CSS font-family for lyrics. Characters that could end the declaration or the
    /// `<style>` block (`< > & { } ;` and control characters) are dropped.
    pub font_family: String,
    /// Prefix for ids and classes, so several scores can share a page. Only ASCII letters,
    /// digits, `-` and `_` are kept; an empty result falls back to `neuma`.
    pub prefix: String,
    /// Include the default `<style>` block.
    pub style: bool,
}

impl Default for SvgOptions {
    fn default() -> SvgOptions {
        SvgOptions {
            font_family: "'EB Garamond', serif".into(),
            prefix: "neuma".into(),
            style: true,
        }
    }
}

/// Two decimals, with no negative zero.
fn n(v: f32) -> String {
    let s = format!("{:.2}", v);
    if s == "-0.00" { "0.00".into() } else { s }
}

/// Whether XML 1.0 allows `c` in text.
fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            c if !xml_char(c) => {}
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

impl Layout<'_> {
    pub fn svg(&self, opts: &SvgOptions) -> String {
        self.display().svg(opts)
    }
}

impl DisplayList {
    pub fn svg(&self, opts: &SvgOptions) -> String {
        let p: String = opts
            .prefix
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .collect();
        let p = if p.is_empty() { "neuma" } else { p.as_str() };
        let mut out = String::new();
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" class="{p}" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-label=""#,
            w = n(self.width),
            h = n(self.height)
        );
        escape(&self.alt_text, &mut out);
        out.push_str("\">");
        if opts.style {
            let _ = write!(
                out,
                "<style>.{p}{{fill:currentColor}}.{p} text{{font-family:{f};font-variant-ligatures:none;font-kerning:normal}}.{p} .{p}-rubric{{fill:var(--{p}-rubric,#a3211c)}}</style>",
                f = opts
                    .font_family
                    .chars()
                    .filter(|&c| xml_char(c) && !c.is_control() && !matches!(c, '<' | '>' | '&' | '{' | '}' | ';'))
                    .collect::<String>()
            );
        }
        let used: BTreeSet<u16> = self
            .items
            .iter()
            .filter_map(|i| if let Item::Glyph { glyph, .. } = i { Some(*glyph) } else { None })
            .collect();
        let scale = self
            .items
            .iter()
            .find_map(|i| if let Item::Glyph { scale, .. } = i { Some(*scale) } else { None })
            .unwrap_or(1.0);
        if !used.is_empty() {
            out.push_str("<defs>");
            for id in &used {
                if let Some(g) = GlyphId::from_id(*id) {
                    let _ = write!(
                        out,
                        r#"<path id="{p}-g{id}" transform="scale({})" d="{}"/>"#,
                        format_scale(scale),
                        g.path()
                    );
                }
            }
            out.push_str("</defs>");
        }
        for item in &self.items {
            match item {
                Item::Glyph {
                    glyph, x, y, role, note, ..
                } => {
                    let _ = write!(
                        out,
                        r##"<use href="#{p}-g{glyph}" x="{}" y="{}" class="{p}-{}""##,
                        n(*x),
                        n(*y),
                        role.class()
                    );
                    if let Some(id) = note {
                        let _ = write!(out, r#" data-note="{id}""#);
                    }
                    out.push_str("/>");
                }
                Item::Rect { x, y, w, h, role, note } => {
                    let _ = write!(
                        out,
                        r#"<rect x="{}" y="{}" width="{}" height="{}" class="{p}-{}""#,
                        n(*x),
                        n(*y),
                        n(*w),
                        n(*h),
                        role.class()
                    );
                    if let Some(id) = note {
                        let _ = write!(out, r#" data-note="{id}""#);
                    }
                    out.push_str("/>");
                }
                Item::Text {
                    x,
                    baseline,
                    size,
                    runs,
                    role,
                    syllable,
                } => {
                    let class = match role {
                        TextRole::Lyric => "lyric",
                        TextRole::Hyphen => "hyphen",
                        TextRole::Initial => "initial",
                        TextRole::Annotation => "annotation",
                        TextRole::Rubric => "rubric",
                    };
                    let _ = write!(
                        out,
                        r#"<text x="{}" y="{}" font-size="{}" class="{p}-{class}""#,
                        n(*x),
                        n(*baseline),
                        n(*size)
                    );
                    if let Some(s) = syllable {
                        let _ = write!(out, r#" data-syllable="{s}""#);
                    }
                    out.push('>');
                    for r in runs {
                        let st = r.style;
                        let mut attrs = String::new();
                        if st.italic {
                            attrs.push_str(r#" font-style="italic""#);
                        }
                        if st.bold {
                            attrs.push_str(r#" font-weight="bold""#);
                        }
                        if st.small_caps {
                            attrs.push_str(r#" font-variant="small-caps""#);
                        }
                        if st.underline {
                            attrs.push_str(r#" text-decoration="underline""#);
                        }
                        if st.rubric && *role != TextRole::Rubric {
                            let _ = write!(attrs, r#" class="{p}-rubric""#);
                        }
                        if attrs.is_empty() {
                            escape(&r.text, &mut out);
                        } else {
                            let _ = write!(out, "<tspan{attrs}>");
                            escape(&r.text, &mut out);
                            out.push_str("</tspan>");
                        }
                    }
                    out.push_str("</text>");
                }
            }
        }
        out.push_str("</svg>");
        out
    }
}

/// The glyph scale needs more precision than coordinates: four significant decimals.
fn format_scale(s: f32) -> String {
    let t = format!("{:.5}", s);
    let t = t.trim_end_matches('0');
    t.trim_end_matches('.').to_string()
}
