//! The browser package's engine side: a thin wrapper that gives [`neuma::Chant`]'s answers as
//! the JSON and strings `js/neuma.mjs` reads. On `wasm32` the `ffi` module exports it over a
//! plain C ABI, so no generated glue is needed and the module can be inlined into a single
//! file. Options, defaults and sanitizing are the core's; this crate only converts.
//!
//! Each page in the glue owns a [`Page`] here, its layout (and its SVG in parts), so it
//! answers for the score it shows for as long as it is kept, as a Rust `Layout` does. The
//! glue frees it with the page.

use std::fmt::Write as _;

#[cfg(feature = "tones")]
use neuma::Utf16Index;
use neuma::{Element, Layout, LayoutOptions, SvgOptions, SvgParts, Weights, json};
#[cfg(feature = "tones")]
use neuma_tones::{AnyChant, PsalmChant, PsalmNote};

#[cfg(any(feature = "font-google", feature = "font-garamond12"))]
pub use neuma::LyricFont;
pub use neuma::{ChantOptions, Initial};

/// What a [`Chant`] engraves: GABC, or with psalm tones built in, a psalm set to a tone.
#[cfg(feature = "tones")]
type Source = AnyChant;
#[cfg(not(feature = "tones"))]
type Source = neuma::Chant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgOutput {
    /// One SVG document.
    Whole,
    /// The SVG in parts, a string per line (see [`neuma::Layout::svg_parts_with`]).
    Lines,
    /// As `Lines`, but a line whose SVG is the same as a line's of the previous page given
    /// is given as `\u{1}` and that line's index, for a page that kept them.
    ChangedLines,
    /// No SVG: only the layout, for its hit tests and timeline.
    None,
}

/// One score, with its answers kept as the JSON the glue reads.
#[derive(Debug)]
pub struct Chant {
    source: Source,
    /// Parse (or psalm-setting) and engrave diagnostics, as JSON.
    diagnostics: String,
    /// The library entry, as JSON, made when first asked for.
    summary: Option<String>,
    /// For a psalm, the setting's `{ gabc, notes, diagnostics }` (see [`setting_json`]).
    psalm: Option<String>,
}

/// A page's layout, and its SVG in parts if it was asked for them, for the next page of the
/// same view to reuse.
#[derive(Debug)]
pub struct Page {
    layout: Layout,
    parts: Option<SvgParts>,
}

impl Chant {
    pub fn new(gabc: &str, options: ChantOptions) -> Chant {
        let chant = neuma::Chant::with_options(gabc, options);
        #[cfg(feature = "tones")]
        let chant = chant.into();
        Chant::wrap(chant)
    }

    /// Psalm text set to `tone` and engraved, its spans in the text.
    #[cfg(feature = "tones")]
    pub fn from_psalm(text: &str, tone: &neuma_tones::Tone, psalm: &neuma_tones::PsalmOptions, options: ChantOptions) -> Chant {
        Chant::wrap(PsalmChant::new(text, tone, psalm, options).into())
    }

    fn wrap(source: Source) -> Chant {
        let mut out = Chant {
            source,
            diagnostics: String::new(),
            summary: None,
            psalm: None,
        };
        out.refresh();
        out
    }

    fn chant(&self) -> &neuma::Chant {
        &self.source
    }

    /// Names the chant's current state, as [`neuma::Chant::version`].
    pub fn version(&self) -> u64 {
        self.source.version()
    }

    /// Replaces the score with `src` (GABC, or psalm text for a chant set from a psalm),
    /// engraved with the same options, as an editor does on each change. Returns whether it
    /// changed anything (not when `src` is the current source).
    pub fn update(&mut self, src: &str) -> bool {
        let changed = self.source.update(src);
        if changed {
            self.refresh();
        }
        changed
    }

    /// Engraves again with `options`. Returns whether that changed anything: options that
    /// engrave as the current ones don't.
    pub fn set_options(&mut self, options: ChantOptions) -> bool {
        let changed = self.source.set_options(options);
        if changed {
            self.refresh();
        }
        changed
    }

    fn refresh(&mut self) {
        let chant = self.chant();
        let mut diagnostics = String::new();
        json::diagnostics(&mut diagnostics, chant.diagnostics(), Some(chant.utf16()));
        self.diagnostics = diagnostics;
        self.summary = None;
        #[cfg(feature = "tones")]
        let psalm = self.source.psalm().map(|p| {
            let mut out = String::new();
            setting_json(&mut out, p.gabc(), p.notes(), p.setting_diagnostics(), p.utf16());
            out
        });
        #[cfg(not(feature = "tones"))]
        let psalm = None;
        self.psalm = psalm;
    }

    pub fn diagnostics_json(&self) -> &str {
        &self.diagnostics
    }

    /// For a chant set from a psalm, the setting's `{ gabc, notes, diagnostics }`.
    pub fn psalm_json(&self) -> Option<&str> {
        self.psalm.as_deref()
    }

    pub fn summary_json(&mut self) -> &str {
        if self.summary.is_none() {
            let mut out = String::new();
            json::summary(&mut out, &self.chant().summary());
            self.summary = Some(out);
        }
        self.summary.as_deref().unwrap_or_default()
    }

    /// Lays the score out at `width` output units: the page, and `{ width, height }`, a NUL,
    /// then the SVG (for `Lines` and `ChangedLines`, as `svg_lines` gives it, reusing the lines
    /// of `previous`, the view's last page).
    pub fn layout(&self, width: f32, opts: &LayoutOptions, svg: &SvgOptions, mode: SvgOutput, previous: Option<&Page>) -> (Page, String) {
        let layout = self.chant().layout_with(width, opts);
        let (w, h) = layout.size();
        let mut out = String::from("{\"width\":");
        json::number(&mut out, w);
        out.push_str(",\"height\":");
        json::number(&mut out, h);
        out.push_str("}\0");
        let parts = match mode {
            SvgOutput::None => None,
            SvgOutput::Whole => {
                out.push_str(&layout.svg_with(svg));
                None
            }
            SvgOutput::Lines | SvgOutput::ChangedLines => {
                let parts = match previous.and_then(|p| p.parts.as_ref()) {
                    Some(shown) => layout.svg_parts_reusing(shown, svg),
                    None => layout.svg_parts_with(svg),
                };
                svg_lines(&mut out, &parts, mode == SvgOutput::ChangedLines);
                Some(parts)
            }
        };
        (Page { layout, parts }, out)
    }

    /// The source this Chant was made or last updated from.
    pub fn source(&self) -> &str {
        self.chant().source()
    }
}

/// Head, definitions and the initial, then each line's top and SVG, all separated by NULs,
/// which SVG text never contains.
fn svg_lines(out: &mut String, parts: &SvgParts, changed: bool) {
    out.reserve(parts.lines.iter().map(|l| l.svg.len() + 12).sum::<usize>() + 4096);
    for s in [&parts.head, &parts.defs, &parts.rest] {
        out.push_str(s);
        out.push('\0');
    }
    for line in &parts.lines {
        json::number(out, line.top);
        out.push('\0');
        match line.reused_from.filter(|_| changed) {
            Some(k) => {
                out.push('\u{1}');
                let _ = write!(out, "{k}");
            }
            None => out.push_str(&line.svg),
        }
        out.push('\0');
    }
    out.pop();
}

impl Page {
    pub fn layout(&self) -> &Layout {
        &self.layout
    }
}

/// The timeline of `layout`, timed with `weights`, as JSON.
pub fn timeline_json(layout: &Layout, weights: &Weights) -> String {
    let timeline = layout.timeline_with(weights);
    let mut out = String::with_capacity(timeline.notes.len() * 460 + 1024);
    json::timeline(&mut out, &timeline, layout.utf16());
    out
}

/// The note, bar or syllable under (`x`, `y`) of `layout`, as JSON (`null` for none).
pub fn source_at_json(layout: &Layout, x: f32, y: f32) -> String {
    let mut out = String::new();
    match layout.source_at(x, y) {
        Some(e) => element_json(&mut out, layout, e),
        None => out.push_str("null"),
    }
    out
}

/// What to highlight for a caret at `offset` (UTF-16 units if `utf16`, else bytes) of
/// `layout`'s source, as a JSON array, most specific first. Past the end is at the end.
pub fn elements_at_json(layout: &Layout, offset: usize, utf16: bool) -> String {
    let found = if utf16 {
        layout.elements_at_utf16(offset)
    } else {
        layout.elements_at(offset)
    };
    let mut out = String::from("[");
    for (i, e) in found.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        element_json(&mut out, layout, e);
    }
    out.push(']');
    out
}

fn element_json(out: &mut String, layout: &Layout, e: &Element) {
    let index = layout.utf16().expect("a chant's layouts know their source");
    json::element(out, e, index);
}

#[cfg(feature = "tones")]
fn part_name(k: neuma_tones::VersePart) -> &'static str {
    match k {
        neuma_tones::VersePart::Flex => "flex",
        neuma_tones::VersePart::Mediant => "mediant",
        neuma_tones::VersePart::Termination => "termination",
    }
}

#[cfg(feature = "tones")]
fn role_name(r: neuma_tones::ToneRole) -> &'static str {
    match r {
        neuma_tones::ToneRole::Intonation => "intonation",
        neuma_tones::ToneRole::Tenor => "tenor",
        neuma_tones::ToneRole::Preparatory => "preparatory",
        neuma_tones::ToneRole::Accent => "accent",
        neuma_tones::ToneRole::Ending => "ending",
        _ => "other",
    }
}

/// A psalm setting: `{ gabc, notes: [{ verse, number, part, role, sourceStart, sourceEnd,
/// sourceUtf16Start, sourceUtf16End }], diagnostics }`. `notes[i]` describes note `i` of the
/// engraved score, and its source is the sung syllable's in `text`, named as the timeline
/// names a note's.
#[cfg(feature = "tones")]
pub fn setting_json(out: &mut String, gabc: &str, notes: &[PsalmNote], diagnostics: &[neuma::Diagnostic], text: &Utf16Index) {
    out.push_str("{\"gabc\":");
    json::string(out, gabc);
    notes_json(out, notes, text);
    out.push_str(",\"diagnostics\":");
    json::diagnostics(out, diagnostics, Some(text));
    out.push('}');
}

#[cfg(feature = "tones")]
fn notes_json(out: &mut String, notes: &[PsalmNote], text: &Utf16Index) {
    out.push_str(",\"notes\":[");
    for (i, n) in notes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, r#"{{"verse":{},"number":"#, n.verse);
        match n.number {
            Some(v) => {
                let _ = write!(out, "{v}");
            }
            None => out.push_str("null"),
        }
        let _ = write!(out, r#","part":"{}","role":"{}","#, part_name(n.part), role_name(n.role));
        json::source_span(out, &n.span, Some(text));
        out.push('}');
    }
    out.push(']');
}

/// A pointing: `{ text, halves: [{ verse, part, confidence, kept, sourceStart, sourceEnd,
/// sourceUtf16Start, sourceUtf16End }], diagnostics }`, each half's source its sung
/// syllables in the text given.
#[cfg(feature = "pointing")]
pub fn pointing_json(out: &mut String, p: &neuma_tones::Pointing, text: &Utf16Index) {
    out.push_str("{\"text\":");
    json::string(out, &p.text);
    out.push_str(",\"halves\":[");
    for (i, h) in p.halves.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, r#"{{"verse":{},"part":"{}","confidence":"#, h.verse, part_name(h.part));
        json::number(out, h.confidence);
        let _ = write!(out, r#","kept":{},"#, h.kept);
        json::source_span(out, &h.span, Some(text));
        out.push('}');
    }
    out.push_str("],\"diagnostics\":");
    json::diagnostics(out, &p.diagnostics, Some(text));
    out.push('}');
}

/// A psalm display: `{ verses: [{ number, sourceStart, sourceEnd, sourceUtf16Start,
/// sourceUtf16End, runs: [{ text, kind }] }], diagnostics }`. A run's `kind` is `text`,
/// `syllable`, `point`, `held`, `mediant`, `flex` or `rubric`; a syllable's run also has
/// `part`, `role`, `accent`, `flexDrop`, `wordStart` and its source in the text.
#[cfg(feature = "tones")]
pub fn display_json(out: &mut String, d: &neuma_tones::PsalmDisplay, text: &Utf16Index) {
    use neuma_tones::PsalmRunKind as K;
    out.push_str("{\"toneLabel\":");
    json::string(out, &d.tone().label());
    out.push_str(",\"verses\":[");
    for (i, v) in d.verses().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"number\":");
        match v.number {
            Some(n) => {
                let _ = write!(out, "{n}");
            }
            None => out.push_str("null"),
        }
        out.push(',');
        json::source_span(out, &v.span, Some(text));
        out.push_str(",\"runs\":[");
        for (j, r) in v.runs.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str("{\"text\":");
            json::string(out, &r.text);
            let kind = match &r.kind {
                K::Syllable(s) => {
                    let _ = write!(
                        out,
                        r#","kind":"syllable","part":"{}","role":"{}","accent":{},"flexDrop":{},"wordStart":{},"#,
                        part_name(s.part),
                        role_name(s.role),
                        s.accent,
                        s.flex_drop,
                        s.word_start
                    );
                    json::source_span(out, &s.span, Some(text));
                    out.push('}');
                    continue;
                }
                K::Point => "point",
                K::Held => "held",
                K::Mediant => "mediant",
                K::Flex => "flex",
                K::Rubric => "rubric",
                _ => "text",
            };
            let _ = write!(out, r#","kind":"{kind}"}}"#);
        }
        out.push_str("]}");
    }
    out.push_str("],\"diagnostics\":");
    json::diagnostics(out, d.diagnostics(), Some(text));
    out.push('}');
}

#[cfg(target_arch = "wasm32")]
mod ffi;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod slab;

#[cfg(test)]
mod tests {
    use super::*;

    fn split(page: &str) -> (&str, &str) {
        page.split_once('\0').unwrap()
    }

    fn page(c: &Chant, width: f32, opts: &LayoutOptions) -> Page {
        c.layout(width, opts, &SvgOptions::default(), SvgOutput::Whole, None).0
    }

    #[test]
    fn lays_out_and_hit_tests() {
        let c = Chant::new(
            "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)",
            ChantOptions::default(),
        );
        assert_eq!(c.diagnostics_json(), "[]");
        let opts = LayoutOptions::default();
        let (p, out) = c.layout(400.0, &opts, &SvgOptions::default(), SvgOutput::Whole, None);
        let (size, svg) = split(&out);
        assert!(size.starts_with(r#"{"width":400,"height":"#), "{size}");
        assert!(svg.starts_with("<svg"));
        let j = timeline_json(p.layout(), &Weights::default());
        assert!(j.contains("\"kind\":\"mediant\"") && j.contains("\"kind\":\"double\""), "{j}");
        assert!(
            j.contains(r#""sourceStart":20,"sourceEnd":21,"sourceUtf16Start":20,"sourceUtf16End":21"#),
            "{j}"
        );
        let n = &p.layout().timeline().notes[0];
        assert_eq!(p.layout().note_at(n.cx, n.cy), Some(0));
    }

    #[test]
    fn pages_answer_for_the_score_they_show() {
        let mut c = Chant::new("(c4) a(g)", ChantOptions::default());
        let opts = LayoutOptions::default();
        let shown = page(&c, 400.0, &opts);
        let before = elements_at_json(shown.layout(), 7, true);
        assert!(!c.update("(c4) a(g)"));
        assert!(!c.set_options(ChantOptions::default()));
        // Any number of changes and other layouts later, the page answers as before.
        for (i, src) in ["(c4) b(h) a(g)", "(c4) c(i) b(h) a(g)", "(c4) a(g) (::)"].iter().enumerate() {
            assert!(c.update(src));
            for w in [100.0, 200.0, 300.0, 500.0, 600.0] {
                let _ = page(&c, w, &opts);
            }
            assert!(c.set_options(ChantOptions::default().with_lyric_size(3.0 + i as f32)));
        }
        assert_eq!(elements_at_json(shown.layout(), 7, true), before);
    }

    #[test]
    fn each_view_reuses_its_own_lines() {
        let mut c = Chant::new("(c4) a(g) b(h) (;) c(i) d(h) (:) e(g) f(h) (::)", ChantOptions::default());
        let opts = LayoutOptions::default();
        let thumb = opts.with_max_lines(1);
        let svg = SvgOptions::default().with_ids(false);
        let lines = |c: &Chant, o: &LayoutOptions, prev: Option<&Page>| c.layout(90.0, o, &svg, SvgOutput::ChangedLines, prev);
        let (main, _) = lines(&c, &opts, None);
        let (small, _) = lines(&c, &thumb, None);
        c.update("(c4) a(g) b(h) (;) c(i) d(h) (:) e(g) f(hg) (::)");
        let (_, out) = lines(&c, &opts, Some(&main));
        assert!(out.contains("\u{1}0"), "the main view keeps its first line");
        let (_, out) = lines(&c, &thumb, Some(&small));
        assert!(out.contains("\u{1}0"), "so does the thumbnail");
    }

    #[test]
    fn updates_and_answers_for_editors() {
        let mut c = Chant::new("(c4) a-(g)", ChantOptions::default());
        assert!(
            c.diagnostics_json()
                .contains(r#""fix":{"start":6,"end":7,"utf16Start":6,"utf16End":7,"replacement":"","title":"#),
            "{}",
            c.diagnostics_json()
        );
        // `é` is two bytes and one UTF-16 unit.
        c.update("(c4) é(g) b(h)");
        assert_eq!(c.diagnostics_json(), "[]");
        let opts = LayoutOptions::default();
        let (p, out) = c.layout(400.0, &opts, &SvgOptions::default(), SvgOutput::Lines, None);
        assert_eq!(split(&out).1.split('\0').count(), 3 + 2);
        let layout = p.layout();
        let b = elements_at_json(layout, 12, true);
        assert!(
            b.starts_with(r#"[{"kind":"note","index":1,"start":13,"end":14,"utf16Start":12,"utf16End":13,"#),
            "{b}"
        );
        assert_eq!(elements_at_json(layout, 13, false), b);
        assert_eq!(elements_at_json(layout, usize::MAX, false), elements_at_json(layout, 16, true));
        assert_eq!(source_at_json(layout, -5.0, -5.0), "null");
        assert!(c.summary_json().contains("\"notes\":2"));
    }

    #[test]
    #[cfg(feature = "tones")]
    fn a_psalm_display_counts_the_text_in_both_units() {
        let text = "1 Bléssed is he * that · cómeth. [Stand.]";
        let tone = neuma_tones::Tone::named("8.G").unwrap();
        let d = neuma_tones::PsalmDisplay::new(text, tone, &neuma_tones::PsalmOptions::default());
        let mut out = String::new();
        display_json(&mut out, &d, &Utf16Index::new(text));
        assert!(out.starts_with(r#"{"toneLabel":"Tone 8 G","verses":[{"number":1,"sourceStart":0,"sourceEnd":44,"sourceUtf16Start":0,"sourceUtf16End":41,"runs":[{"text":"Bléssed","kind":"syllable","part":"mediant","role":"accent","accent":true,"flexDrop":false,"wordStart":true,"sourceStart":2,"sourceEnd":10,"sourceUtf16Start":2,"sourceUtf16End":9}"#), "{out}");
        assert!(
            out.contains("{\"text\":\"\u{a0}\",\"kind\":\"text\"},{\"text\":\"*\",\"kind\":\"mediant\"}"),
            "{out}"
        );
        assert!(
            out.contains(r#"{"text":"·","kind":"point"}"#) && out.contains(r#"{"text":"Stand.","kind":"rubric"}"#),
            "{out}"
        );
        assert!(out.contains(r#"}]}],"diagnostics":[{"severity":"info""#), "{out}");
        serde_check(&out);
    }

    /// The JSON is well formed: brackets balance outside strings.
    #[cfg(feature = "tones")]
    fn serde_check(json: &str) {
        let (mut depth, mut in_str, mut esc) = (0i32, false, false);
        for c in json.chars() {
            match (in_str, esc, c) {
                (true, true, _) => esc = false,
                (true, false, '\\') => esc = true,
                (true, false, '"') | (false, _, '"') => in_str = !in_str,
                (false, _, '{' | '[') => depth += 1,
                (false, _, '}' | ']') => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0);
        }
        assert_eq!((depth, in_str), (0, false));
    }

    #[test]
    #[cfg(feature = "tones")]
    fn psalm_notes_count_the_text_in_both_units() {
        let text = "Bléssed is he * that cómeth.";
        let tone = neuma_tones::Tone::named("8.G").unwrap();
        let s = neuma_tones::psalm(text, tone, &neuma_tones::PsalmOptions::default());
        let mut out = String::new();
        setting_json(&mut out, &s.gabc, &s.notes, &s.diagnostics, &Utf16Index::new(text));
        assert!(
            out.contains(r#""sourceStart":0,"sourceEnd":8,"sourceUtf16Start":0,"sourceUtf16End":7}"#),
            "{out}"
        );
        let mut c = Chant::from_psalm(text, tone, &neuma_tones::PsalmOptions::default(), ChantOptions::default());
        assert_eq!(c.psalm_json().unwrap(), out);
        assert_eq!(c.source(), text);
        assert!(c.update("Bléssed is he * that cómeth, and is."));
        assert!(c.psalm_json().unwrap().contains("is.(g)"), "{}", c.psalm_json().unwrap());
    }
}
