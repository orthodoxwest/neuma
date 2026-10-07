//! [`Chant`]: one score and everything made from it, owned, for apps, servers and editors.

use std::fmt;
use std::sync::{Arc, OnceLock};

use crate::NoteRef;
use crate::diag::Diagnostic;
use crate::engrave::{EngraveCache, Engraving, Initial, StyleOptions};
#[cfg(feature = "fonts")]
use crate::fonts::LyricFont;
use crate::layout::{Layout, LayoutCache, LayoutOptions, Lines, PlacedInitial, PlacedLine};
use crate::score::Score;
use crate::source::{Element, OffsetUnit, SourceMap, Utf16Index};
use crate::summary::Summary;
#[cfg(feature = "svg")]
use crate::svg::{SvgCache, SvgOptions, SvgParts};
use crate::text::TextMeasure;

/// How a [`Chant`] engraves its score: the lyric font to measure with and the style. Build it
/// with the `with_*` setters:
/// `ChantOptions::default().with_font(LyricFont::Garamond12).with_initial(Initial::Lines(2))`.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct ChantOptions {
    /// The EB Garamond the lyrics will be drawn with; default [`LyricFont::Google`].
    #[cfg(feature = "fonts")]
    pub font: LyricFont,
    /// Everything else about the engraving. A lyric size that isn't positive and finite
    /// falls back to the default.
    pub style: StyleOptions,
    /// Measures lyrics instead of `font`'s built-in metrics.
    measure: Option<Arc<dyn TextMeasure + Send + Sync>>,
}

impl ChantOptions {
    /// Sets [`font`](Self::font).
    #[cfg(feature = "fonts")]
    #[must_use]
    pub fn with_font(mut self, font: LyricFont) -> ChantOptions {
        self.font = font;
        self
    }

    /// Sets [`style`](Self::style).
    #[must_use]
    pub fn with_style(mut self, style: StyleOptions) -> ChantOptions {
        self.style = style;
        self
    }

    /// Sets the style's [`initial`](StyleOptions::initial).
    #[must_use]
    pub fn with_initial(mut self, initial: Initial) -> ChantOptions {
        self.style.initial = initial;
        self
    }

    /// Sets the style's [`annotation`](StyleOptions::annotation).
    #[must_use]
    pub fn with_annotation(mut self, annotation: bool) -> ChantOptions {
        self.style.annotation = annotation;
        self
    }

    /// Sets the style's [`lyric_size`](StyleOptions::lyric_size).
    #[must_use]
    pub fn with_lyric_size(mut self, lyric_size: f32) -> ChantOptions {
        self.style.lyric_size = lyric_size;
        self
    }

    /// Measures lyrics with `measure` (a [`MetricsTable`](crate::MetricsTable) for another
    /// font, say) rather than a built-in [`LyricFont`]'s metrics.
    #[must_use]
    pub fn with_measure(mut self, measure: Arc<dyn TextMeasure + Send + Sync>) -> ChantOptions {
        self.measure = Some(measure);
        self
    }

    fn measure(&self) -> &dyn TextMeasure {
        match &self.measure {
            Some(m) => m.as_ref(),
            #[cfg(feature = "fonts")]
            None => self.font.metrics(),
            #[cfg(not(feature = "fonts"))]
            None => &crate::text::ApproxMeasure,
        }
    }

    /// The style with its lyric size made usable.
    fn sanitized_style(&self) -> StyleOptions {
        let mut style = self.style.clone();
        if !(style.lyric_size.is_finite() && style.lyric_size > 0.0) {
            style.lyric_size = StyleOptions::default().lyric_size;
        }
        style
    }
}

impl PartialEq for ChantOptions {
    fn eq(&self, other: &ChantOptions) -> bool {
        #[cfg(feature = "fonts")]
        if self.font != other.font {
            return false;
        }
        let same_measure = match (&self.measure, &other.measure) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        self.style == other.style && same_measure
    }
}

impl fmt::Debug for ChantOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("ChantOptions");
        #[cfg(feature = "fonts")]
        d.field("font", &self.font);
        d.field("style", &self.style)
            .field("measure", &self.measure.as_ref().map(|_| "custom"))
            .finish()
    }
}

/// One score and everything made from it: the source, the score, its engraving, the caches
/// that make the next edit and the next layout cheap, and the last layout, which answers
/// hit tests. It is the front door to neuma; the pipeline it wraps ([`crate::parse`],
/// [`Score::engrave`], [`crate::Engraving::layout`] and the caches) stays public for advanced
/// use.
///
/// A `Chant` is `Send` and `Sync`. Coordinates are in output units from the layout's top left,
/// y down (see the crate docs).
///
/// ```
/// use neuma::{Chant, LayoutOptions, OffsetUnit, SvgOptions};
///
/// let mut chant = Chant::new("(c4) Ky(f)ri(gh)e(g) (::)");
/// let svg = chant.layout(600.0).svg();
/// // A click on the first note finds it and its source.
/// let (x, y) = { let e = &chant.source_map().unwrap().notes[0]; (e.x + 1.0, e.y + 1.0) };
/// assert_eq!(chant.note_at(x, y), Some(0));
/// assert_eq!(&chant.source()[chant.source_at(x, y).unwrap().span.clone()], "f");
/// // An edit engraves and lays out again only around what changed.
/// chant.update("(c4) Ky(f)ri(gh)e(gf) (::)");
/// let parts = { chant.layout(600.0); chant.svg_parts() };
/// assert_eq!(parts.lines.len(), 1);
/// assert_eq!(chant.elements_at(18, OffsetUnit::Utf16).len(), 2); // the note and its syllable
/// ```
#[derive(Debug)]
pub struct Chant {
    options: ChantOptions,
    source: String,
    utf16: Utf16Index,
    /// The score and its engraving, kept to engrave the next edit from.
    engraved: EngraveCache,
    /// Parse and engrave diagnostics, in that order.
    diagnostics: Vec<Diagnostic>,
    last: Option<Placed>,
    /// The line breaker's work, reused by the next layout after an edit.
    layout_cache: LayoutCache,
    /// Each line's SVG from the last `svg_parts`, to reuse for lines an edit left alone.
    #[cfg(feature = "svg")]
    svg_cache: SvgCache,
    /// The last layout's source map, made when first asked for.
    sources: OnceLock<SourceMap>,
}

/// A layout's lines and size, kept without borrowing the engraving.
#[derive(Debug)]
struct Placed {
    /// The width (as bits) and options it was made for.
    asked: (u32, LayoutOptions),
    lines: Vec<PlacedLine>,
    initial: Option<PlacedInitial>,
    width: f32,
    height: f32,
    scale: f32,
}

impl Placed {
    fn view<'a>(&'a self, eng: &'a Engraving) -> Layout<'a> {
        Layout {
            eng,
            lines: Lines::Borrowed(&self.lines),
            initial: self.initial,
            width: self.width,
            height: self.height,
            scale: self.scale,
        }
    }
}

impl Chant {
    /// Parses and engraves `gabc` with the default options. Parsing never fails: problems
    /// come back as [`diagnostics`](Self::diagnostics).
    #[must_use]
    pub fn new(gabc: &str) -> Chant {
        Chant::with_options(gabc, ChantOptions::default())
    }

    /// Parses and engraves `gabc` with `options`.
    #[must_use]
    pub fn with_options(gabc: &str, options: ChantOptions) -> Chant {
        let mut chant = Chant::empty(options);
        chant.update(gabc);
        chant
    }

    /// Engraves a score built some other way, such as a psalm set to a tone
    /// (`neuma_tones::psalm`), whose spans count bytes of `source`: hit tests and
    /// [`utf16`](Self::utf16) then answer in `source`.
    #[must_use]
    pub fn from_score(score: Score, source: &str, options: ChantOptions) -> Chant {
        let mut chant = Chant::empty(options);
        chant.update_score(score, source);
        chant
    }

    fn empty(options: ChantOptions) -> Chant {
        Chant {
            options,
            source: String::new(),
            utf16: Utf16Index::default(),
            engraved: EngraveCache::default(),
            diagnostics: Vec::new(),
            last: None,
            layout_cache: LayoutCache::default(),
            #[cfg(feature = "svg")]
            svg_cache: SvgCache::default(),
            sources: OnceLock::new(),
        }
    }

    /// Replaces the score with `gabc`, keeping the options, as an editor does on each change.
    /// Only the syllables around the edit are engraved again, and the next layout breaks
    /// again only the lines they are on. The last layout is dropped: lay out again to see the
    /// change.
    pub fn update(&mut self, gabc: &str) {
        let parsed = crate::parse(gabc);
        self.set(parsed.score, gabc, parsed.diagnostics);
    }

    /// [`update`](Self::update) with a score built some other way (see
    /// [`from_score`](Self::from_score)).
    pub fn update_score(&mut self, score: Score, source: &str) {
        self.set(score, source, Vec::new());
    }

    fn set(&mut self, score: Score, source: &str, mut diagnostics: Vec<Diagnostic>) {
        let style = self.options.sanitized_style();
        let engraving = self.engraved.engrave(score, self.options.measure(), &style);
        diagnostics.extend(engraving.diagnostics.iter().cloned());
        self.diagnostics = diagnostics;
        self.source.clear();
        self.source.push_str(source);
        self.utf16 = Utf16Index::new(source);
        self.last = None;
        self.sources = OnceLock::new();
    }

    /// The source the chant was made or last updated from.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The options the chant engraves with.
    #[must_use]
    pub fn options(&self) -> &ChantOptions {
        &self.options
    }

    /// Converts the source's offsets between UTF-8 bytes (as spans count) and UTF-16 units.
    #[must_use]
    pub fn utf16(&self) -> &Utf16Index {
        &self.utf16
    }

    /// The score.
    #[must_use]
    pub fn score(&self) -> &Score {
        self.engraved.score().expect("a chant is engraved when made")
    }

    /// The score's engraving, for the lower-level pipeline.
    #[must_use]
    pub fn engraving(&self) -> &Engraving {
        self.engraved.engraving().expect("a chant is engraved when made")
    }

    /// Every problem found reading and engraving the score, those from reading first.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The score's library entry, as [`crate::summarize`] gives it.
    #[must_use]
    pub fn summary(&self) -> Summary {
        self.engraving().summary(&self.score().header)
    }

    /// Lays the score out at `width` output units with the default [`LayoutOptions`]; see
    /// [`layout_with`](Self::layout_with).
    pub fn layout(&mut self, width: f32) -> Layout<'_> {
        self.layout_with(width, &LayoutOptions::default())
    }

    /// Lays the score out at `width` output units, and keeps the layout for hit tests and
    /// [`svg_parts`](Self::svg_parts). After an edit only the lines it touched are broken
    /// again; at the same width and options as the last layout, with no edit between, the
    /// last layout is returned as it is.
    ///
    /// A `Chant` shared between threads can't lay out through `&self`; lay out its
    /// [`engraving`](Self::engraving) instead, which any number of threads can do at once.
    pub fn layout_with(&mut self, width: f32, options: &LayoutOptions) -> Layout<'_> {
        let eng = self.engraved.engraving().expect("a chant is engraved when made");
        let asked = (width.to_bits(), *options);
        if self.last.as_ref().is_none_or(|p| p.asked != asked) {
            let layout = eng.layout_cached(width, options, &mut self.layout_cache);
            let lines = match layout.lines {
                Lines::Owned(lines) => lines,
                Lines::Borrowed(lines) => lines.to_vec(),
            };
            self.sources = OnceLock::new();
            self.last = Some(Placed {
                asked,
                lines,
                initial: layout.initial,
                width: layout.width,
                height: layout.height,
                scale: layout.scale,
            });
        }
        self.last.as_ref().expect("laid out above").view(eng)
    }

    /// The last layout, until the next [`update`](Self::update); `None` before the first.
    #[must_use]
    pub fn last_layout(&self) -> Option<Layout<'_>> {
        Some(self.last.as_ref()?.view(self.engraving()))
    }

    /// The last layout's SVG in parts (see [`Layout::svg_parts_with`]), each line's string taken
    /// from the parts made before it when the line is drawn the same. Before the first layout
    /// it has no lines.
    #[cfg(feature = "svg")]
    #[must_use]
    pub fn svg_parts(&mut self) -> SvgParts {
        self.svg_parts_with(&SvgOptions::default())
    }

    /// [`svg_parts`](Self::svg_parts) with `options`.
    #[cfg(feature = "svg")]
    #[must_use]
    pub fn svg_parts_with(&mut self, options: &SvgOptions) -> SvgParts {
        let eng = self.engraved.engraving().expect("a chant is engraved when made");
        let empty = Placed {
            asked: (0, LayoutOptions::default()),
            lines: Vec::new(),
            initial: None,
            width: 0.0,
            height: 0.0,
            scale: LayoutOptions::default().scale,
        };
        self.last
            .as_ref()
            .unwrap_or(&empty)
            .view(eng)
            .svg_parts_cached(options, &mut self.svg_cache)
    }

    /// For each line of the last [`svg_parts`](Self::svg_parts), the line of the parts before
    /// it whose string it took, if any: a page that kept those lines need not read them again.
    #[cfg(feature = "svg")]
    #[must_use]
    pub fn reused_svg_lines(&self) -> &[Option<usize>] {
        self.svg_cache.reused()
    }

    /// The last layout's source map, made when first asked for; `None` before the first
    /// layout.
    #[must_use]
    pub fn source_map(&self) -> Option<&SourceMap> {
        let layout = self.last_layout()?;
        Some(self.sources.get_or_init(|| layout.source_map()))
    }

    /// The note under (`x`, `y`) in the last layout: one whose box holds the point, else the
    /// nearest on the line the point falls in. `None` outside every line.
    #[must_use]
    pub fn note_at(&self, x: f32, y: f32) -> Option<NoteRef> {
        self.source_map()?.note_at(x, y)
    }

    /// The note, bar or syllable under (`x`, `y`) in the last layout, with its source span: a
    /// notehead, else a bar within half a staff space, else a syllable's box, else the
    /// nearest syllable on that line. `None` outside every line.
    #[must_use]
    pub fn source_at(&self, x: f32, y: f32) -> Option<&Element> {
        self.source_map()?.source_at(x, y)
    }

    /// What to highlight for a caret at `offset` in the source, counted in `unit`, in the
    /// last layout: the notes and bar whose source holds it, then a box per line for its
    /// syllable, most specific first. A caret just after a note, as after typing it, counts
    /// as on it; an offset past the end is the end.
    #[must_use]
    pub fn elements_at(&self, offset: usize, unit: OffsetUnit) -> Vec<&Element> {
        let byte = match unit {
            OffsetUnit::Utf8 => offset.min(self.source.len()),
            OffsetUnit::Utf16 => self.utf16.to_utf8(offset),
        };
        self.source_map().map(|m| m.elements_at(byte)).unwrap_or_default()
    }
}

#[cfg(all(test, feature = "svg", feature = "fonts"))]
mod tests {
    use super::*;
    use crate::{ApproxMeasure, ElementKind};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn is_send_and_sync() {
        assert_send_sync::<Chant>();
        assert_send_sync::<ChantOptions>();
    }

    #[test]
    fn lays_out_as_the_pipeline_does() {
        let src = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)";
        let mut chant = Chant::new(src);
        assert!(chant.diagnostics().is_empty());
        let opts = LayoutOptions::default().with_scale(7.0);
        let eng = crate::parse(src)
            .score
            .engrave(LyricFont::Google.metrics(), &StyleOptions::default());
        let want = eng.layout_with(400.0, &opts);
        let got = chant.layout_with(400.0, &opts);
        assert_eq!(got.svg(), want.svg());
        assert_eq!(got.timeline(), want.timeline());
        assert_eq!(chant.last_layout().unwrap().display(), want.display());
        assert_eq!(chant.summary(), crate::summarize(src));
    }

    #[test]
    fn hit_tests_need_a_layout() {
        let mut chant = Chant::new("(c4) a(g)");
        assert!(chant.note_at(0.0, 0.0).is_none() && chant.source_map().is_none());
        assert!(chant.svg_parts().lines.is_empty());
        let _ = chant.layout(300.0);
        let timeline = chant.last_layout().unwrap().timeline();
        let n = &timeline.notes[0];
        assert_eq!(chant.note_at(n.cx, n.cy), Some(0));
        assert_eq!(chant.source_at(n.cx, n.cy).map(|e| e.kind), Some(ElementKind::Note));
        // An edit drops the layout.
        chant.update("(c4) é(g) b(h)");
        assert!(chant.note_at(n.cx, n.cy).is_none());
        let _ = chant.layout(300.0);
        // `é` is two bytes and one UTF-16 unit: `b`'s note `h` is at byte 13, unit 12.
        let at16 = chant.elements_at(12, OffsetUnit::Utf16);
        assert_eq!(at16, chant.elements_at(13, OffsetUnit::Utf8));
        assert_eq!((at16[0].kind, at16[0].index, at16[0].span.clone()), (ElementKind::Note, 1, 13..14));
    }

    #[test]
    fn sanitizes_and_measures_as_told() {
        let src = "(c4) Al(f)le(gf)lú(gh)ia.(g.) (::)";
        let bad = Chant::with_options(src, ChantOptions::default().with_lyric_size(f32::NAN));
        let good = Chant::new(src);
        assert_eq!(bad.engraving(), good.engraving());
        let approx = Chant::with_options(src, ChantOptions::default().with_measure(Arc::new(ApproxMeasure)));
        let want = crate::parse(src).score.engrave(&ApproxMeasure, &StyleOptions::default());
        assert_eq!(approx.engraving(), &want);
    }
}
