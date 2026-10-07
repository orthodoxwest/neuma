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
    /// Mark each note's ink with `data-note` and each lyric with `data-syllable`.
    pub ids: bool,
}

impl Default for SvgOptions {
    fn default() -> SvgOptions {
        SvgOptions {
            font_family: "'EB Garamond', serif".into(),
            prefix: "neuma".into(),
            style: true,
            ids: true,
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

    /// The SVG in parts, for an editor that patches its page rather than replacing it: each
    /// line's SVG is positioned relative to the line's top, so a line that only moves up or
    /// down keeps the same string. With `opts.ids` off, a line also keeps its string when
    /// notes are added or removed before it.
    pub fn svg_parts(&self, opts: &SvgOptions) -> SvgParts {
        self.svg_parts_cached(opts, &mut SvgCache::default())
    }

    /// As [`Layout::svg_parts`], taking each line's SVG from `cache` when the line draws the
    /// same as one the cache saw last time (and keeping this layout's lines there for the
    /// next). After a small edit most lines draw as before, so only the lines it touched
    /// are written again; the result is the same as `svg_parts`.
    pub fn svg_parts_cached(&self, opts: &SvgOptions, cache: &mut SvgCache) -> SvgParts {
        let p = prefix(opts);
        if cache.prefix != p || cache.ids != opts.ids {
            *cache = SvgCache {
                prefix: p.clone(),
                ids: opts.ids,
                ..SvgCache::default()
            };
        }
        let mut old: Vec<Option<CachedLine>> = std::mem::take(&mut cache.lines).into_iter().map(Some).collect();
        let mut by_hash: std::collections::HashMap<u64, Vec<usize>> = std::collections::HashMap::with_capacity(old.len());
        for (k, l) in old.iter().enumerate() {
            if let Some(l) = l {
                by_hash.entry(l.hash).or_default().push(k);
            }
        }
        let s = self.scale;
        let one = |line: crate::layout::PlacedLine, initial| Layout {
            eng: self.eng,
            lines: vec![line],
            initial,
            width: self.width,
            height: self.height,
            scale: self.scale,
        };
        let mut used = BTreeSet::new();
        let mut glyph_scale = None;
        let mut lines = Vec::with_capacity(self.lines.len());
        cache.reused.clear();
        for line in &self.lines {
            let top = line.top;
            let mut items = Vec::new();
            self.push_line(&mut items, line, top);
            note_glyphs(&items, &mut used, &mut glyph_scale);
            if !opts.ids {
                // Without ids, a line draws the same whatever its notes' and syllables'
                // numbers, which an edit before it changes.
                strip_ids(&mut items);
            }
            let hash = hash_items(&items);
            let seen = by_hash
                .get(&hash)
                .and_then(|ks| ks.iter().copied().find(|&k| old[k].as_ref().is_some_and(|l| l.items == items)));
            let svg = match seen.and_then(|k| old[k].take()) {
                Some(l) => {
                    cache.reused.push(seen);
                    l.svg
                }
                None => {
                    cache.reused.push(None);
                    let mut svg = String::with_capacity(items.len() * 96);
                    write_items(&mut svg, &items, &p, opts.ids);
                    svg
                }
            };
            lines.push(SvgLine {
                top: top * s,
                svg: svg.clone(),
            });
            cache.lines.push(CachedLine { hash, items, svg });
        }
        // The initial and annotations, which hang beside the first lines.
        let mut rest = String::new();
        if let Some(first) = self.lines.first()
            && self.initial.is_some()
        {
            let on_line = one(first.clone(), None).display().items.len();
            let items = one(first.clone(), self.initial).display().items;
            let items = &items[on_line.min(items.len())..];
            note_glyphs(items, &mut used, &mut glyph_scale);
            write_items(&mut rest, items, &p, opts.ids);
        }
        let (width, height) = self.size();
        let mut head = String::new();
        write_head(&mut head, width, height, &self.eng.alt_text, &p, opts);
        let mut defs = String::new();
        write_defs(&mut defs, &used, glyph_scale.unwrap_or(1.0), &p);
        SvgParts {
            width,
            height,
            head,
            defs,
            lines,
            rest,
        }
    }
}

/// What [`Layout::svg_parts_cached`] keeps between layouts: each line's drawing and SVG.
#[derive(Clone, Debug, Default)]
pub struct SvgCache {
    prefix: String,
    ids: bool,
    lines: Vec<CachedLine>,
    reused: Vec<Option<usize>>,
}

impl SvgCache {
    /// For each line of the last [`Layout::svg_parts_cached`], the line of the call before
    /// it whose SVG it took, if it took one: a page that kept those lines' SVG need not read
    /// it again.
    pub fn reused(&self) -> &[Option<usize>] {
        &self.reused
    }
}

#[derive(Clone, Debug)]
struct CachedLine {
    hash: u64,
    items: Vec<Item>,
    svg: String,
}

/// Clears the note and syllable numbers, which only ids write.
fn strip_ids(items: &mut [Item]) {
    for i in items {
        match i {
            Item::Glyph { note, through, .. } | Item::Rect { note, through, .. } => {
                *note = None;
                *through = None;
            }
            Item::Text { syllable, .. } => *syllable = None,
        }
    }
}

/// A hash of `items` that agrees with their equality (numbers equal as floats hash alike),
/// quick rather than strong: a match is confirmed by comparing the items.
fn hash_items(items: &[Item]) -> u64 {
    let mut h = items.len() as u64;
    let mut mix = |v: u64| h = (h.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    let f = |v: f32| if v == 0.0 { 0 } else { v.to_bits() as u64 };
    for i in items {
        match i {
            Item::Glyph { glyph, x, y, .. } => {
                mix(*glyph as u64);
                mix(f(*x) << 32 | f(*y));
            }
            Item::Rect { x, y, w, h, .. } => {
                mix(f(*x) << 32 | f(*y));
                mix(f(*w) << 32 | f(*h));
            }
            Item::Text { x, baseline, runs, .. } => {
                mix(f(*x) << 32 | f(*baseline));
                for r in runs {
                    for b in r.text.bytes() {
                        mix(b as u64);
                    }
                }
            }
        }
    }
    h
}

/// A layout's SVG in parts (see [`Layout::svg_parts`]).
#[derive(Clone, Debug, PartialEq)]
pub struct SvgParts {
    pub width: f32,
    pub height: f32,
    /// The `<svg>` start tag, with its size, class and label, and the `<style>` element if
    /// asked for.
    pub head: String,
    /// The `<path>` elements of the glyphs the score uses, to go inside a `<defs>` element.
    pub defs: String,
    pub lines: Vec<SvgLine>,
    /// Everything that isn't on a line (the initial and its annotations), in page
    /// coordinates.
    pub rest: String,
}

/// One line of a score's SVG.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgLine {
    /// Where the line's top falls on the page, in output units.
    pub top: f32,
    /// The line's elements, positioned relative to its top: draw them translated down by
    /// `top`.
    pub svg: String,
}

impl SvgParts {
    /// The parts put together as one SVG document, each line in a translated `<g>`.
    pub fn to_svg(&self) -> String {
        let mut out = String::with_capacity(self.head.len() + self.defs.len() + self.lines.iter().map(|l| l.svg.len() + 40).sum::<usize>());
        out.push_str(&self.head);
        if !self.defs.is_empty() {
            out.push_str("<defs>");
            out.push_str(&self.defs);
            out.push_str("</defs>");
        }
        for line in &self.lines {
            out.push_str("<g transform=\"translate(0 ");
            push_n(&mut out, line.top);
            out.push_str(")\">");
            out.push_str(&line.svg);
            out.push_str("</g>");
        }
        out.push_str(&self.rest);
        out.push_str("</svg>");
        out
    }
}

/// The class and id prefix, sanitized.
fn prefix(opts: &SvgOptions) -> String {
    let p: String = opts
        .prefix
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    if p.is_empty() { "neuma".into() } else { p }
}

/// Adds the glyphs `items` use to `used`, and notes their scale.
fn note_glyphs(items: &[Item], used: &mut BTreeSet<u16>, scale: &mut Option<f32>) {
    for i in items {
        if let Item::Glyph { glyph, scale: s, .. } = i {
            used.insert(*glyph);
            scale.get_or_insert(*s);
        }
    }
}

fn write_head(out: &mut String, width: f32, height: f32, alt_text: &str, p: &str, opts: &SvgOptions) {
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" class="{p}" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-label=""#,
        w = n(width),
        h = n(height)
    );
    escape(alt_text, out);
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
}

fn write_defs(out: &mut String, used: &BTreeSet<u16>, scale: f32, p: &str) {
    for id in used {
        if let Some(g) = GlyphId::from_id(*id) {
            let _ = write!(
                out,
                r#"<path id="{p}-s{}-g{id}" transform="scale({})" d="{}"/>"#,
                scale_tag(scale),
                format_scale(scale),
                g.path()
            );
        }
    }
}

impl DisplayList {
    pub fn svg(&self, opts: &SvgOptions) -> String {
        let p = prefix(opts);
        let mut out = String::with_capacity(1024 + self.items.len() * 96);
        write_head(&mut out, self.width, self.height, &self.alt_text, &p, opts);
        let mut used = BTreeSet::new();
        let mut scale = None;
        note_glyphs(&self.items, &mut used, &mut scale);
        if !used.is_empty() {
            out.push_str("<defs>");
            write_defs(&mut out, &used, scale.unwrap_or(1.0), &p);
            out.push_str("</defs>");
        }
        write_items(&mut out, &self.items, &p, opts.ids);
        out.push_str("</svg>");
        out
    }
}

/// Writes the elements for `items`; `ids` adds the `data-note` and `data-syllable`
/// attributes.
fn write_items(out: &mut String, items: &[Item], p: &str, ids: bool) {
    // The scale's part of the glyph ids, for the last scale seen (a layout has one).
    let mut tag: Option<(f32, String)> = None;
    for item in items {
        match item {
            Item::Glyph {
                glyph,
                x,
                y,
                scale,
                role,
                note,
                through,
            } => {
                let t = match &tag {
                    Some((s, t)) if s == scale => t,
                    _ => &tag.insert((*scale, scale_tag(*scale))).1,
                };
                out.push_str("<use href=\"#");
                out.push_str(p);
                out.push_str("-s");
                out.push_str(t);
                out.push_str("-g");
                crate::decimal::push_u64(out, *glyph as u64);
                out.push_str("\" x=\"");
                push_n(out, *x);
                out.push_str("\" y=\"");
                push_n(out, *y);
                push_class(out, p, role.class());
                if ids {
                    data_note(out, *note, *through);
                }
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
                push_n(out, *x);
                out.push_str("\" y=\"");
                push_n(out, *y);
                out.push_str("\" width=\"");
                push_n(out, *w);
                out.push_str("\" height=\"");
                push_n(out, *h);
                push_class(out, p, role.class());
                if ids {
                    data_note(out, *note, *through);
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
                out.push_str("<text x=\"");
                push_n(out, *x);
                out.push_str("\" y=\"");
                push_n(out, *baseline);
                out.push_str("\" font-size=\"");
                push_n(out, *size);
                push_class(out, p, class);
                if let Some(s) = syllable
                    && ids
                {
                    out.push_str(" data-syllable=\"");
                    crate::decimal::push_u64(out, *s as u64);
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
                        escape(&r.text, out);
                    } else {
                        let _ = write!(out, "<tspan{attrs}>");
                        escape(&r.text, out);
                        out.push_str("</tspan>");
                    }
                }
                out.push_str("</text>");
            }
        }
    }
}

/// The scale as glyph ids carry it: the definition's scale with `_` for the point, so two
/// scores on one page share a glyph's id only when they draw it alike.
fn scale_tag(s: f32) -> String {
    format_scale(s).replace('.', "_")
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
