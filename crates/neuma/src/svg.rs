//! SVG output. Every fill is `currentColor` and every element has a role class, so pages can
//! theme staff, notes and rubrics (and dark mode) with CSS alone. Coordinates are written with
//! exactly two decimals.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use crate::display::{DisplayList, Item, TextRole};
use crate::glyphs::{GlyphId, UNITS_PER_SPACE};
use crate::layout::{Layout, LineShape, PlacedLine};

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
    /// Include the default `<style>` block. A page that styles the SVG itself should give its
    /// `text` the lyric face, `font-variant-ligatures: none` and `text-rendering:
    /// geometricPrecision`, so lyrics are drawn at the advances they were measured with.
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
        let old: &[DrawnLine] = previous.map_or(&[], |d| &d.lines);
        let mut taken = vec![false; old.len()];
        // The previous lines by what they draw from, and by their SVG.
        let mut by_key: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut by_svg: HashMap<u64, Vec<usize>> = HashMap::new();
        for (k, line) in old.iter().enumerate() {
            by_key.entry(line.key.hash).or_default().push(k);
            by_svg.entry(line.svg_hash).or_default().push(k);
        }
        let s = self.scale;
        let mut used = Glyphs::default();
        let mut lines = Vec::with_capacity(self.lines.len());
        let mut drawn = Vec::with_capacity(self.lines.len());
        let mut items = Vec::new();
        for line in self.lines.iter() {
            let top = line.top;
            let key = self.line_key(line, opts.ids);
            // A previous line drawn from the same segments, placed alike, draws the same.
            let same = by_key
                .get(&key.hash)
                .and_then(|ks| ks.iter().map(|&k| &old[k]).find(|o| o.key == key));
            let (svg, svg_hash, glyphs) = match same {
                Some(o) => (Arc::clone(&o.svg), o.svg_hash, o.glyphs),
                None => {
                    items.clear();
                    self.push_line(&mut items, line, top);
                    let mut glyphs = Glyphs::default();
                    glyphs.add(&items);
                    let mut svg = String::with_capacity(items.len() * 96);
                    write_items(&mut svg, &items, &p, opts.ids);
                    let svg_hash = hash_str(&svg);
                    (Arc::<str>::from(svg), svg_hash, glyphs)
                }
            };
            used.union(&glyphs);
            // Which previous line, not yet taken, wrote the same SVG.
            let seen = by_svg
                .get(&svg_hash)
                .and_then(|ks| ks.iter().copied().find(|&k| !taken[k] && *old[k].svg == *svg));
            let svg = match seen {
                Some(k) => {
                    taken[k] = true;
                    Arc::clone(&old[k].svg)
                }
                None => svg,
            };
            lines.push(SvgLine {
                top: top * s,
                svg: Arc::clone(&svg),
                reused_from: seen,
            });
            drawn.push(DrawnLine {
                key,
                svg,
                svg_hash,
                glyphs,
            });
        }
        // The initial and annotations, which hang beside the first lines.
        let mut rest = String::new();
        if self.initial.is_some() && !self.lines.is_empty() {
            items.clear();
            self.push_initial(&mut items);
            used.add(&items);
            write_items(&mut rest, &items, &p, opts.ids);
        }
        let (width, height) = self.size();
        let head_of = HeadOf {
            width: width.to_bits(),
            height: height.to_bits(),
            style: opts.style,
            font_family: opts.font_family.clone(),
            alt_text: self.eng.alt_text.clone(),
        };
        // The head writes the score's text, as its label, so is kept when that is unchanged.
        let head = match previous {
            Some(d) if d.head_of == head_of => d.head.clone(),
            _ => {
                let mut head = String::new();
                write_head(&mut head, width, height, &self.eng.alt_text, &p, opts);
                head
            }
        };
        let kept_head = head.clone();
        let mut defs = String::new();
        write_defs(&mut defs, &used, s / UNITS_PER_SPACE, &p);
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
                head: kept_head,
                head_of,
            },
        }
    }

    /// What a line's SVG is made from: how it is set across (shared between layouts that set
    /// it alike, from the same segments' ink), and where it is placed. Two lines with equal
    /// keys draw the same.
    fn line_key(&self, line: &PlacedLine, ids: bool) -> LineKey {
        let eng = &*self.eng;
        let f = |x: f32| x.to_bits();
        let mut words = Vec::with_capacity(if ids { 6 + 2 * (line.last + 1 - line.first) } else { 6 });
        words.extend([
            f(self.scale),
            f(self.width),
            f(eng.lyric_size),
            f(eng.hyphen),
            f(line.staff - line.top),
            f(line.baseline - line.top),
        ]);
        if ids {
            // The note and syllable numbers the ids write.
            for seg in &eng.segments[line.first..=line.last] {
                words.extend([seg.note_base, seg.syllable]);
            }
        }
        let mut h = Arc::as_ptr(&line.shape) as usize as u64;
        for &x in &words {
            h = (h.rotate_left(5) ^ u64::from(x)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
        LineKey {
            hash: h,
            words,
            shape: Arc::clone(&line.shape),
        }
    }
}

/// What each line of [`SvgParts`] was drawn from and its SVG, kept apart from the public
/// lines (which the caller may take or change) so that the next parts can tell which lines
/// draw the same and share their strings.
#[derive(Clone, Default)]
struct Drawn {
    prefix: String,
    ids: bool,
    lines: Vec<DrawnLine>,
    head: String,
    head_of: HeadOf,
}

/// What the head was written from.
#[derive(Clone, Default, PartialEq)]
struct HeadOf {
    width: u32,
    height: u32,
    style: bool,
    font_family: String,
    alt_text: String,
}

#[derive(Clone)]
struct DrawnLine {
    key: LineKey,
    svg: Arc<str>,
    svg_hash: u64,
    glyphs: Glyphs,
}

/// See [`Layout::line_key`]. A layout shares a line's shape only with lines set from the same
/// segments' ink, which it holds while it does; the key holds the shape, so no other shape
/// takes its address while the key is kept.
#[derive(Clone)]
struct LineKey {
    hash: u64,
    words: Vec<u32>,
    shape: Arc<LineShape>,
}

impl PartialEq for LineKey {
    fn eq(&self, other: &LineKey) -> bool {
        self.hash == other.hash && Arc::ptr_eq(&self.shape, &other.shape) && self.words == other.words
    }
}

/// A quick hash of `s`, to find a line's SVG among others: a match is confirmed by comparing
/// the strings.
fn hash_str(s: &str) -> u64 {
    let mut h = s.len() as u64;
    let mut mix = |x: u64| h = (h.rotate_left(5) ^ x).wrapping_mul(0x517c_c1b7_2722_0a95);
    let (chunks, rest) = s.as_bytes().as_chunks::<8>();
    for c in chunks {
        mix(u64::from_le_bytes(*c));
    }
    for &b in rest {
        mix(u64::from(b));
    }
    h
}

/// A set of glyphs, by id.
#[derive(Clone, Copy, Default)]
struct Glyphs([u64; GlyphId::ALL.len().div_ceil(64)]);

impl Glyphs {
    /// Adds the glyphs `items` use.
    fn add(&mut self, items: &[Item]) {
        for i in items {
            if let Item::Glyph { glyph, .. } = i {
                let g = usize::from(*glyph);
                if let Some(w) = self.0.get_mut(g / 64) {
                    *w |= 1 << (g % 64);
                }
            }
        }
    }

    fn union(&mut self, other: &Glyphs) {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a |= b;
        }
    }

    fn ids(&self) -> impl Iterator<Item = u16> + '_ {
        (0..self.0.len() * 64)
            .filter(|&g| self.0[g / 64] >> (g % 64) & 1 == 1)
            .map(|g| g as u16)
    }
}

/// A layout's SVG in parts (see [`Layout::svg_parts`]).
///
/// Besides the strings, parts keep what each line was drawn from (a few numbers, and a share
/// of the engraving's ink for its segments), so that [`Layout::svg_parts_reusing`] can tell
/// which lines of the next layout draw the same without drawing them. That share keeps the
/// ink of the chant's version they were written from: keep only the parts a page shows.
#[derive(Clone)]
#[non_exhaustive]
pub struct SvgParts {
    /// The SVG's width, in output units.
    pub width: f32,
    /// The SVG's height, in output units.
    pub height: f32,
    /// The `<svg>` start tag, with its size, class and label, and the `<style>` element if
    /// asked for.
    pub head: String,
    /// The `<path>` elements of the glyphs the score uses, to go inside a `<defs>` element.
    pub defs: String,
    /// Each line of the score, top to bottom.
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
            "<style>.{p}{{fill:currentColor}}.{p} text{{font-family:{f};font-variant-ligatures:none;font-kerning:normal;text-rendering:geometricPrecision}}.{p} .{p}-rubric{{fill:var(--{p}-rubric,#a3211c)}}.{p} .{p}-sign{{stroke:currentColor;stroke-width:.04em}}.{p} .{p}-rubric.{p}-sign,.{p} .{p}-rubric .{p}-sign{{stroke:var(--{p}-rubric,#a3211c)}}</style>",
            f = opts
                .font_family
                .chars()
                .filter(|&c| xml_char(c) && !c.is_control() && !matches!(c, '<' | '>' | '&' | '{' | '}' | ';'))
                .collect::<String>()
        );
    }
}

fn write_defs(out: &mut String, used: &Glyphs, scale: f32, p: &str) {
    for id in used.ids() {
        if let Some(g) = GlyphId::from_id(id) {
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
        let mut used = Glyphs::default();
        used.add(&self.items);
        // The scale of the first glyph (a layout's are all alike).
        let scale = self.items.iter().find_map(|i| match i {
            Item::Glyph { scale, .. } => Some(*scale),
            _ => None,
        });
        if let Some(scale) = scale {
            out.push_str("<defs>");
            write_defs(&mut out, &used, scale, &p);
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
