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
    let mut s = String::with_capacity(12);
    push_n(&mut s, v);
    s
}

/// Appends `v` as [`n`] writes it.
fn push_n(out: &mut String, v: f32) {
    let start = out.len();
    crate::decimal::push_fixed(out, v, 2);
    if &out[start..] == "-0.00" {
        out.replace_range(start.., "0.00");
    }
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
        let mut out = String::with_capacity(1024 + self.items.len() * 96);
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
                "<style>.{p}{{fill:currentColor}}.{p} text{{font-family:{f};font-variant-ligatures:none;font-kerning:normal}}.{p} .{p}-rubric{{fill:var(--{p}-rubric,#a3211c)}}.{p} .{p}-sign{{stroke:currentColor;stroke-width:.04em}}.{p} .{p}-rubric.{p}-sign,.{p} .{p}-rubric .{p}-sign{{stroke:var(--{p}-rubric,#a3211c)}}</style>",
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
                    glyph,
                    x,
                    y,
                    role,
                    note,
                    through,
                    ..
                } => {
                    out.push_str("<use href=\"#");
                    out.push_str(p);
                    out.push_str("-g");
                    crate::decimal::push_u64(&mut out, *glyph as u64);
                    out.push_str("\" x=\"");
                    push_n(&mut out, *x);
                    out.push_str("\" y=\"");
                    push_n(&mut out, *y);
                    push_class(&mut out, p, role.class());
                    data_note(&mut out, *note, *through);
                    out.push_str("/>");
                }
                Item::Rect {
                    x,
                    y,
                    w,
                    h,
                    role,
                    note,
                    through,
                } => {
                    out.push_str("<rect x=\"");
                    push_n(&mut out, *x);
                    out.push_str("\" y=\"");
                    push_n(&mut out, *y);
                    out.push_str("\" width=\"");
                    push_n(&mut out, *w);
                    out.push_str("\" height=\"");
                    push_n(&mut out, *h);
                    push_class(&mut out, p, role.class());
                    data_note(&mut out, *note, *through);
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
                    out.push_str("<text x=\"");
                    push_n(&mut out, *x);
                    out.push_str("\" y=\"");
                    push_n(&mut out, *baseline);
                    out.push_str("\" font-size=\"");
                    push_n(&mut out, *size);
                    push_class(&mut out, p, class);
                    if let Some(s) = syllable {
                        out.push_str(" data-syllable=\"");
                        crate::decimal::push_u64(&mut out, *s as u64);
                        out.push('"');
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
                        // ℣ and ℟ are thin in text faces; GregorioTeX's are heavier, so the style
                        // block strokes them.
                        let sign = r.text.contains(['℣', '℟']);
                        match (st.rubric && *role != TextRole::Rubric, sign) {
                            (true, true) => {
                                let _ = write!(attrs, r#" class="{p}-rubric {p}-sign""#);
                            }
                            (true, false) => {
                                let _ = write!(attrs, r#" class="{p}-rubric""#);
                            }
                            (false, true) => {
                                let _ = write!(attrs, r#" class="{p}-sign""#);
                            }
                            (false, false) => {}
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

/// Closes the previous attribute and writes the role class.
fn push_class(out: &mut String, prefix: &str, class: &str) {
    out.push_str("\" class=\"");
    out.push_str(prefix);
    out.push('-');
    out.push_str(class);
    out.push('"');
}

/// `data-note` lists every note the ink draws, so `[data-note~="3"]` finds note 3 whether it
/// has a glyph of its own or shares a porrectus swash.
fn data_note(out: &mut String, note: Option<u32>, through: Option<u32>) {
    let Some(first) = note else { return };
    out.push_str(" data-note=\"");
    crate::decimal::push_u64(out, first as u64);
    for id in first + 1..=through.unwrap_or(first) {
        out.push(' ');
        crate::decimal::push_u64(out, id as u64);
    }
    out.push('"');
}
