//! [`Chant`]: one score and everything made from it, owned, for apps, servers and editors.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use crate::diag::Diagnostic;
use crate::engrave::{AlterationScope, CustosPolicy, EngraveCache, Engraving, Initial, StyleOptions};
#[cfg(feature = "fonts")]
use crate::fonts::LyricFont;
use crate::layout::{Layout, LayoutCache, LayoutOptions, usable_width};
use crate::score::Score;
use crate::source::Utf16Index;
use crate::summary::Summary;
use crate::text::TextMeasure;
use crate::vowel::VowelRules;

/// How a [`Chant`] engraves its score: the lyric font to measure with and the style. Build it
/// with the `with_*` setters, which include one for each [`StyleOptions`] field:
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

    /// Sets the style's [`vowels`](StyleOptions::vowels).
    #[must_use]
    pub fn with_vowels(mut self, vowels: Option<VowelRules>) -> ChantOptions {
        self.style.vowels = vowels;
        self
    }

    /// Sets the style's [`alterations`](StyleOptions::alterations).
    #[must_use]
    pub fn with_alterations(mut self, alterations: AlterationScope) -> ChantOptions {
        self.style.alterations = alterations;
        self
    }

    /// Sets the style's [`custos`](StyleOptions::custos).
    #[must_use]
    pub fn with_custos(mut self, custos: CustosPolicy) -> ChantOptions {
        self.style.custos = custos;
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

    /// Whether `self` and `other` engrave alike: the same measure and the same style once
    /// sanitized.
    fn engraves_like(&self, other: &ChantOptions) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.style = a.sanitized_style();
        b.style = b.sanitized_style();
        a == b
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

/// One score and everything made from it: the source, the score, its engraving, and the
/// caches that make the next edit and the next layout cheap. It is the front door to neuma;
/// the pipeline it wraps ([`crate::parse`], [`Score::engrave`], [`Engraving::layout`]) stays
/// public for advanced use.
///
/// A `Chant` is `Send` and `Sync`, and lays out through `&self`: share one in an `Arc` and lay
/// it out from several threads at once. Each [`Layout`] it returns is owned, so layouts at
/// several widths can be kept side by side, each answering its own hit tests, while the
/// chant is updated. Coordinates are in output units from the layout's top left, y down (see
/// the crate docs).
///
/// ```
/// use neuma::{Chant, SvgOptions};
///
/// let mut chant = Chant::new("(c4) Ky(f)ri(gh)e(g) (::)");
/// let layout = chant.layout(600.0);
/// let svg = layout.svg();
/// // A click on the first note finds it and its source.
/// let (x, y) = { let e = &layout.source_map().notes[0]; (e.x + 1.0, e.y + 1.0) };
/// assert_eq!(layout.note_at(x, y), Some(0));
/// assert_eq!(&chant.source()[layout.source_at(x, y).unwrap().span.clone()], "f");
/// // An edit engraves and lays out again only around what changed; the old layout lives on.
/// chant.update("(c4) Ky(f)ri(gh)e(gf) (::)");
/// let after = chant.layout(600.0);
/// let parts = after.svg_parts_reusing(&layout.svg_parts(), &SvgOptions::default());
/// assert_eq!(parts.lines.len(), 1);
/// assert_eq!(after.elements_at_utf16(18).len(), 2); // a caret in a text field: the note and its syllable
/// assert_eq!(layout.note_at(x, y), Some(0));
/// ```
pub struct Chant {
    options: ChantOptions,
    source: String,
    utf16: Arc<Utf16Index>,
    /// The score and its engraving, kept to engrave the next edit from.
    engraved: EngraveCache,
    /// What reading the score found (parse or psalm-setting diagnostics).
    read: Vec<Diagnostic>,
    /// `read`, then the engraving's.
    diagnostics: Vec<Diagnostic>,
    caches: Caches,
}

/// What lays the chant out again cheaply. Each is behind its own lock, taken only for a
/// moment (`recent`) or tried and done without when busy (`layouts`), so concurrent layouts
/// never wait on one another for long.
#[derive(Default)]
struct Caches {
    /// The last few layouts, most recent last, by the width (as bits) and the sanitized
    /// options they were made for; cleared on every change.
    recent: Mutex<Vec<(Asked, Layout)>>,
    /// The line breaker's work, reused by the next layout after an edit.
    layouts: Mutex<LayoutCache>,
}

type Asked = (u32, LayoutOptions);

/// Layouts the memo keeps: enough for a main view and a thumbnail or two.
const RECENT: usize = 4;

/// Runs `f` with the cache behind `m`, or with none if another thread holds it. A cache left
/// poisoned by a panic is started over.
fn with_cache<C: Default, R>(m: &Mutex<C>, f: impl FnOnce(Option<&mut C>) -> R) -> R {
    match m.try_lock() {
        Ok(mut cache) => f(Some(&mut cache)),
        Err(TryLockError::Poisoned(p)) => {
            let mut cache = p.into_inner();
            *cache = C::default();
            m.clear_poison();
            f(Some(&mut cache))
        }
        Err(TryLockError::WouldBlock) => f(None),
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
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
    /// (`neuma_tones::PsalmSetting::into_chant`), whose spans count bytes of `source`: hit
    /// tests and [`utf16`](Self::utf16) then answer in `source`. `diagnostics` are what
    /// building the score found; the chant's [`diagnostics`](Self::diagnostics) start with
    /// them. [`update`](Self::update) reads GABC: give a chant of another source its new
    /// scores with [`update_score`](Self::update_score) (as `neuma_tones::PsalmChant` does).
    #[must_use]
    pub fn from_score(score: Score, source: &str, diagnostics: Vec<Diagnostic>, options: ChantOptions) -> Chant {
        let mut chant = Chant::empty(options);
        chant.update_score(score, source, diagnostics);
        chant
    }

    fn empty(options: ChantOptions) -> Chant {
        Chant {
            options,
            source: String::new(),
            utf16: Arc::default(),
            engraved: EngraveCache::default(),
            read: Vec::new(),
            diagnostics: Vec::new(),
            caches: Caches::default(),
        }
    }

    /// Replaces the source with `gabc`, keeping the options, as an editor does on each
    /// change. Only the syllables around the edit are engraved again, and the next layout
    /// breaks again only the lines they are on. Layouts made before keep showing the old
    /// score.
    ///
    /// A layout still held shares the old engraving, which the update must then copy rather
    /// than take back: on a long score that adds about a quarter to the update's time, on a
    /// typical one next to nothing. Drop layouts a view has replaced.
    ///
    /// Returns whether anything changed: `false` when `gabc` is the current source, and the
    /// chant, its layouts and its memo stay as they were.
    pub fn update(&mut self, gabc: &str) -> bool {
        if self.engraved.score().is_some() && gabc == self.source {
            return false;
        }
        let parsed = crate::parse(gabc);
        self.update_score(parsed.score, gabc, parsed.diagnostics)
    }

    /// [`update`](Self::update) with a score built some other way (see
    /// [`from_score`](Self::from_score)). Returns whether anything changed: `false` when the
    /// source, the score and the diagnostics are the current ones.
    pub fn update_score(&mut self, score: Score, source: &str, diagnostics: Vec<Diagnostic>) -> bool {
        if self.engraved.score().is_some_and(|s| *s == score) && source == self.source && diagnostics == self.read {
            return false;
        }
        self.read = diagnostics;
        self.source.clear();
        self.source.push_str(source);
        self.utf16 = Arc::new(Utf16Index::new(source));
        self.engrave(score);
        true
    }

    /// Engraves the score with new options, as when the reader changes the lyric font or
    /// size (Dynamic Type, say) or the initial. Options that engrave the same as the current
    /// ones change nothing; others engrave the score again, and the next layout reuses
    /// whatever lines still come out the same.
    ///
    /// Returns whether it engraved again, which is when layouts made before are out of date.
    pub fn set_options(&mut self, options: ChantOptions) -> bool {
        let same = self.options.engraves_like(&options);
        self.options = options;
        if same {
            return false;
        }
        if let Some(score) = self.engraved.take_score() {
            self.engrave(score);
        }
        true
    }

    fn engrave(&mut self, score: Score) {
        // Layouts in the memo share the old engraving; dropping them lets the cache take it
        // back without a copy.
        lock(&self.caches.recent).clear();
        let style = self.options.sanitized_style();
        let engraving = self.engraved.engrave(score, self.options.measure(), &style);
        self.diagnostics.clear();
        self.diagnostics.extend(self.read.iter().cloned());
        self.diagnostics.extend(engraving.diagnostics.iter().cloned());
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
        self.shared()
    }

    fn shared(&self) -> &Arc<Engraving> {
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
    #[must_use]
    pub fn layout(&self, width: f32) -> Layout {
        self.layout_with(width, &LayoutOptions::default())
    }

    /// Lays the score out at `width` output units. The chant remembers its last few layouts:
    /// asked again for one at the same width and options (as sanitized), with no change
    /// between, it returns that layout, sharing its source map. After an edit only the lines
    /// it touched are broken again. The layout's [`utf16`](Layout::utf16) index is the
    /// chant's.
    #[must_use]
    pub fn layout_with(&self, width: f32, options: &LayoutOptions) -> Layout {
        let asked = (usable_width(width).to_bits(), options.sanitized());
        {
            let mut recent = lock(&self.caches.recent);
            if let Some(i) = recent.iter().position(|(a, _)| *a == asked) {
                let hit = recent.remove(i);
                let layout = hit.1.clone();
                recent.push(hit);
                return layout;
            }
        }
        let eng = self.shared();
        let mut layout = with_cache(&self.caches.layouts, |cache| match cache {
            Some(cache) => eng.layout_cached(width, options, cache),
            None => eng.layout_with(width, options),
        });
        layout.utf16 = Some(Arc::clone(&self.utf16));
        let mut recent = lock(&self.caches.recent);
        if recent.len() >= RECENT {
            recent.remove(0);
        }
        recent.push((asked, layout.clone()));
        layout
    }
}

impl fmt::Debug for Chant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chant")
            .field("source_len", &self.source.len())
            .field("syllables", &self.score().syllables.len())
            .field("diagnostics", &self.diagnostics.len())
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

#[cfg(all(test, feature = "svg", feature = "fonts"))]
mod tests {
    use super::*;
    use crate::svg::{SvgOptions, SvgParts};
    use crate::{ApproxMeasure, ElementKind};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn is_send_and_sync() {
        assert_send_sync::<Chant>();
        assert_send_sync::<ChantOptions>();
        assert_send_sync::<Layout>();
    }

    fn pipeline(src: &str, style: &StyleOptions) -> Arc<Engraving> {
        crate::parse(src).score.engrave(LyricFont::Google.metrics(), style)
    }

    #[test]
    fn lays_out_as_the_pipeline_does() {
        let src = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)";
        let chant = Chant::new(src);
        assert!(chant.diagnostics().is_empty());
        let opts = LayoutOptions::default().with_scale(7.0);
        let want = pipeline(src, &StyleOptions::default()).layout_with(400.0, &opts);
        let got = chant.layout_with(400.0, &opts);
        assert_eq!(got.svg(), want.svg());
        assert_eq!(got.timeline(), want.timeline());
        assert_eq!(got.display(), want.display());
        assert_eq!(chant.summary(), crate::summarize(src));
    }

    #[test]
    fn layouts_are_independent() {
        let chant = Arc::new(Chant::new("(c4) a(g) b(h) c(i) d(h) e(g) f(h) g(i) (::)"));
        let wide = chant.layout(600.0);
        let thumb = chant.layout_with(600.0, &LayoutOptions::default().with_max_lines(1));
        let narrow = std::thread::spawn({
            let chant = Arc::clone(&chant);
            move || chant.layout(80.0)
        })
        .join()
        .unwrap();
        assert!(narrow.line_count() > wide.line_count());
        // Each answers for itself, whichever was made last.
        let n = &wide.timeline().notes[6];
        assert_eq!(wide.note_at(n.cx, n.cy), Some(6));
        assert_eq!(thumb.note_at(n.cx, n.cy), Some(6));
        let m = &narrow.timeline().notes[6];
        assert_eq!(narrow.note_at(m.cx, m.cy), Some(6));
        // The same request gives the same layout, sharing its source map.
        let again = chant.layout(600.0);
        assert!(std::ptr::eq(again.source_map(), wide.source_map()));
        let nan = chant.layout_with(600.0, &LayoutOptions::default().with_scale(f32::NAN));
        assert!(std::ptr::eq(nan.source_map(), wide.source_map()));
    }

    #[test]
    fn an_update_leaves_old_layouts_alone() {
        let mut chant = Chant::new("(c4) a(g)");
        let before = chant.layout(300.0);
        let n = before.timeline().notes[0].clone();
        assert_eq!(before.note_at(n.cx, n.cy), Some(0));
        assert_eq!(before.source_at(n.cx, n.cy).map(|e| e.kind), Some(ElementKind::Note));
        chant.update("(c4) é(g) b(h)");
        assert_eq!(before.note_at(n.cx, n.cy), Some(0));
        assert_eq!(before.timeline().notes.len(), 1);
        let after = chant.layout(300.0);
        assert_eq!(after.timeline().notes.len(), 2);
        // `é` is two bytes and one UTF-16 unit: `b`'s note `h` is at byte 13, unit 12.
        let at = after.elements_at(after.utf16().unwrap().to_utf8(12));
        assert_eq!((at[0].kind, at[0].index, at[0].span.clone()), (ElementKind::Note, 1, 13..14));
    }

    #[test]
    fn svg_parts_say_what_they_reused() {
        let mut chant = Chant::new("(c4) a(g) b(h) (;) c(i) d(h) (:) e(g) f(h) (::)");
        let opts = SvgOptions::default();
        let first = chant.layout(90.0).svg_parts_with(&opts);
        assert!(first.lines.len() > 2, "{}", first.lines.len());
        assert!(first.lines.iter().all(|l| l.reused_from.is_none()));
        chant.update("(c4) a(g) b(h) (;) c(i) d(h) (:) e(g) f(hg) (::)");
        let second = chant.layout(90.0).svg_parts_reusing(&first, &opts);
        assert_eq!(second.lines[0].reused_from, Some(0));
        assert_eq!(second.lines.last().unwrap().reused_from, None);
        let fresh = chant.layout(90.0).svg_parts_with(&opts);
        let strip = |p: &SvgParts| p.lines.iter().map(|l| (l.top, l.svg.clone())).collect::<Vec<_>>();
        assert_eq!(strip(&second), strip(&fresh));
        // Parts with another prefix share no lines.
        let other = chant.layout(90.0).svg_parts_reusing(&first, &opts.clone().with_prefix("x"));
        assert!(other.lines.iter().all(|l| l.reused_from.is_none()));
    }

    #[test]
    fn svg_parts_reuse_only_what_the_engine_wrote() {
        let mut chant = Chant::new("(c4) a(g) b(h) (;) c(i) d(h) (:) e(g) f(h) (::)");
        let opts = SvgOptions::default();
        let fresh = |chant: &Chant| chant.layout(90.0).svg_parts_with(&opts);
        // A page that moved the lines' strings out of the parts, and one that changed them.
        let mut drained = fresh(&chant);
        let taken: Vec<_> = drained.lines.drain(..).collect();
        assert!(taken.len() > 2);
        let mut edited = fresh(&chant);
        for l in &mut edited.lines {
            l.svg = format!("<g>{}</g>", l.svg).into();
        }
        edited.lines.truncate(1);
        chant.update("(c4) a(g) b(h) (;) c(i) d(h) (:) e(g) f(hg) (::)");
        let want = fresh(&chant);
        for previous in [&drained, &edited] {
            let next = chant.layout(90.0).svg_parts_reusing(previous, &opts);
            assert_eq!(next.lines[0].reused_from, Some(0));
            let svg = |p: &SvgParts| p.lines.iter().map(|l| (l.top, l.svg.clone())).collect::<Vec<_>>();
            assert_eq!(svg(&next), svg(&want));
        }
    }

    #[test]
    fn says_whether_it_changed() {
        let mut chant = Chant::new("(c4) a(g)");
        let before = chant.layout(300.0);
        assert!(!chant.update("(c4) a(g)"));
        assert!(std::ptr::eq(chant.layout(300.0).source_map(), before.source_map()));
        assert!(!chant.set_options(ChantOptions::default()));
        assert!(std::ptr::eq(chant.layout(300.0).source_map(), before.source_map()));
        assert!(chant.update("(c4) a(h)"));
        assert!(chant.set_options(ChantOptions::default().with_lyric_size(3.0)));
        let score = chant.score().clone();
        let source = chant.source().to_owned();
        assert!(!chant.update_score(score, &source, Vec::new()));
    }

    #[test]
    fn set_options_engraves_again() {
        let src = "(c4) Al(f)le(gf)lú(gh)ia.(g.) (::)";
        let mut chant = Chant::new(src);
        let small = chant.layout(600.0);
        assert!(!chant.set_options(ChantOptions::default().with_lyric_size(f32::NAN)));
        assert!(std::ptr::eq(chant.layout(600.0).source_map(), small.source_map()));
        chant.set_options(ChantOptions::default().with_lyric_size(4.0));
        let style = StyleOptions::default().with_lyric_size(4.0);
        assert_eq!(chant.engraving(), &*pipeline(src, &style));
        assert_ne!(chant.layout(600.0).size(), small.size());
    }

    #[test]
    fn sanitizes_and_measures_as_told() {
        let src = "(c4) Al(f)le(gf)lú(gh)ia.(g.) (::)";
        let bad = Chant::with_options(src, ChantOptions::default().with_lyric_size(f32::NAN));
        let good = Chant::new(src);
        assert_eq!(bad.engraving(), good.engraving());
        let approx = Chant::with_options(src, ChantOptions::default().with_measure(Arc::new(ApproxMeasure)));
        let want = crate::parse(src).score.engrave(&ApproxMeasure, &StyleOptions::default());
        assert_eq!(approx.engraving(), &*want);
        assert!(!format!("{good:?}").contains("segments"));
    }
}
