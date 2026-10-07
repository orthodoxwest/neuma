//! SVG output. Every fill is `currentColor` and every element has a role class, so pages can
//! theme staff, notes and rubrics (and dark mode) with CSS alone. Coordinates are written with
//! exactly two decimals.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::sync::Arc;

use crate::display::{DisplayList, Item, TextRole};
use crate::glyphs::GlyphId;
use crate::layout::Layout;

/// How to write SVG. Build it with the `with_*` setters:
/// `SvgOptions::default().with_prefix("intro").with_ids(false)`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
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

impl SvgOptions {
    /// Sets [`font_family`](Self::font_family).
    #[must_use]
    pub fn with_font_family(mut self, font_family: impl Into<String>) -> SvgOptions {
        self.font_family = font_family.into();
        self
    }

    /// Sets [`prefix`](Self::prefix).
    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> SvgOptions {
        self.prefix = prefix.into();
        self
    }

    /// Sets [`style`](Self::style).
    #[must_use]
    pub fn with_style(mut self, style: bool) -> SvgOptions {
        self.style = style;
        self
    }

    /// Sets [`ids`](Self::ids).
    #[must_use]
    pub fn with_ids(mut self, ids: bool) -> SvgOptions {
        self.ids = ids;
        self
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

impl Layout {
    /// This layout as one SVG document, with the default [`SvgOptions`].
    #[must_use]
    pub fn svg(&self) -> String {
        self.svg_with(&SvgOptions::default())
    }

    /// This layout as one SVG document.
    #[must_use]
    pub fn svg_with(&self, opts: &SvgOptions) -> String {
        self.display().svg_with(opts)
    }

    /// The SVG in parts, for an editor that patches its page rather than replacing it: each
    /// line's SVG is positioned relative to the line's top, so a line that only moves up or
    /// down keeps the same string. With `opts.ids` off, a line also keeps its string when
    /// notes are added or removed before it.
    #[must_use]
    pub fn svg_parts(&self) -> SvgParts {
        self.svg_parts_with(&SvgOptions::default())
    }

    /// [`svg_parts`](Self::svg_parts) with `opts`.
    #[must_use]
    pub fn svg_parts_with(&self, opts: &SvgOptions) -> SvgParts {
        self.parts(opts, None)
    }

    /// [`svg_parts_with`](Self::svg_parts_with), taking each line's SVG from `previous` (the
    /// parts of an earlier layout, of this score or an earlier version of it) when the line
    /// draws the same as one of `previous`'s. After a small edit most lines draw as before, so
    /// only the lines it touched are written again, and each line's
    /// [`reused_from`](SvgLine::reused_from) says which line of `previous` it repeats: a page
    /// showing `previous` replaces only the lines where that is `None`, and moves the rest.
    /// The result is otherwise the same as `svg_parts_with`'s. Parts written with another
    /// prefix or `ids` setting reuse nothing.
    ///
    /// Only what `previous` keeps privately is read, so its public `lines` may have been
    /// drained into a page or changed: what is reused is always the engine's own SVG.
    ///
    /// ```
    /// use neuma::{Chant, SvgOptions};
    ///
    /// let mut chant = Chant::new("(c4) Ky(f)ri(gh)e(g) e(f)lé(g)i(h)son.(g) (::)");
    /// let opts = SvgOptions::default().with_ids(false);
    /// let shown = chant.layout(60.0).svg_parts_with(&opts);
    /// chant.update("(c4) Ky(f)ri(gh)e(g) e(f)lé(g)i(h)son.(gf) (::)");
    /// let next = chant.layout(60.0).svg_parts_reusing(&shown, &opts);
    /// assert_eq!(next.lines[0].reused_from, Some(0)); // the edit is on the last line
    /// assert_eq!(next.lines.last().unwrap().reused_from, None);
    /// ```
    #[must_use]
    pub fn svg_parts_reusing(&self, previous: &SvgParts, opts: &SvgOptions) -> SvgParts {
        self.parts(opts, Some(previous))
    }

    fn parts(&self, opts: &SvgOptions, previous: Option<&SvgParts>) -> SvgParts {
        let p = prefix(opts);
        // Only what the parts keep privately is read: the caller may have taken or changed
        // the public lines.
        let previous = previous.map(|q| &q.drawn).filter(|d| d.prefix == p && d.ids == opts.ids);
        let mut taken = vec![false; previous.map_or(0, |d| d.lines.len())];
        let mut by_hash: std::collections::HashMap<u64, Vec<usize>> = std::collections::HashMap::new();
        if let Some(d) = previous {
            for (k, line) in d.lines.iter().enumerate() {
                by_hash.entry(line.hash).or_default().push(k);
            }
        }
        let s = self.scale;
        let one = |line: crate::layout::PlacedLine, initial| {
            Layout::new(self.eng.clone(), vec![line], initial, self.width, self.height, self.scale)
        };
        let mut used = BTreeSet::new();
        let mut glyph_scale = None;
        let mut lines = Vec::with_capacity(self.lines.len());
        let mut drawn = Vec::with_capacity(self.lines.len());
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
            let seen = previous.and_then(|d| {
                by_hash
                    .get(&hash)?
                    .iter()
                    .copied()
                    .find(|&k| !taken[k] && d.lines[k].items == items)
            });
            let svg: Arc<str> = match (previous, seen) {
                (Some(d), Some(k)) => {
                    taken[k] = true;
                    Arc::clone(&d.lines[k].svg)
                }
                _ => {
                    let mut svg = String::with_capacity(items.len() * 96);
                    write_items(&mut svg, &items, &p, opts.ids);
                    svg.into()
                }
            };
            lines.push(SvgLine {
                top: top * s,
                svg: Arc::clone(&svg),
                reused_from: seen,
            });
            drawn.push(DrawnLine { hash, items, svg });
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
            drawn: Drawn {
                prefix: p,
                ids: opts.ids,
                lines: drawn,
            },
        }
    }
}

/// What each line of [`SvgParts`] draws and its SVG, kept apart from the public lines (which
/// the caller may take or change) so that the next parts can tell which lines draw the same
/// and share their strings.
#[derive(Clone, Default)]
struct Drawn {
    prefix: String,
    ids: bool,
    lines: Vec<DrawnLine>,
}

#[derive(Clone)]
struct DrawnLine {
    hash: u64,
    items: Vec<Item>,
    svg: Arc<str>,
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
///
/// Besides the strings, parts keep what each line draws, so that
/// [`Layout::svg_parts_reusing`] can tell which lines of the next layout draw the same: about
/// two thirds again the memory of the strings. Keep only the parts a page shows.
#[derive(Clone)]
#[non_exhaustive]
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
    drawn: Drawn,
}

impl PartialEq for SvgParts {
    /// Whether the parts are written alike (what they keep to compare lines with aside).
    fn eq(&self, other: &SvgParts) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.head == other.head
            && self.defs == other.defs
            && self.lines == other.lines
            && self.rest == other.rest
    }
}

impl std::fmt::Debug for SvgParts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgParts")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("head", &self.head)
            .field("defs", &self.defs)
            .field("lines", &self.lines)
            .field("rest", &self.rest)
            .finish_non_exhaustive()
    }
}

/// One line of a score's SVG.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SvgLine {
    /// Where the line's top falls on the page, in output units.
    pub top: f32,
    /// The line's elements, positioned relative to its top: draw them translated down by
    /// `top`. Shared, not copied, with the parts it is reused from and by.
    pub svg: Arc<str>,
    /// From [`Layout::svg_parts_reusing`]: the index of the line of the previous parts that
    /// this line draws the same as, its SVG unchanged, so a page showing those parts can keep
    /// that line's element and only move it. Always `None` from [`Layout::svg_parts`].
    ///
    /// The index is in the engine's numbering of the previous parts: their lines as the
    /// engine wrote them, which is `previous.lines` as returned. A caller that reorders or
    /// drains those lines keeps its own map from that numbering to what it shows.
    pub reused_from: Option<usize>,
}

impl SvgParts {
    /// The parts put together as one SVG document, each line in a translated `<g>`.
    #[must_use]
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
    /// This display list as one SVG document, with the default [`SvgOptions`].
    #[must_use]
    pub fn svg(&self) -> String {
        self.svg_with(&SvgOptions::default())
    }

    /// This display list as one SVG document.
    #[must_use]
    pub fn svg_with(&self, opts: &SvgOptions) -> String {
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
                    // ℣, ℟ and the slashed A are thin in text faces; GregorioTeX's are
                    // heavier, so the style block strokes them.
                    let sign = r.text.contains(['℣', '℟', '\u{338}']);
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
