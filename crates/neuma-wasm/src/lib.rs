//! The browser package's engine side: a thin wrapper that gives [`neuma::Chant`]'s answers as
//! the JSON and strings `js/neuma.mjs` reads. On `wasm32` the `ffi` module exports it over a
//! plain C ABI, so no generated glue is needed and the module can be inlined into a single
//! file. Options, defaults and sanitizing are the core's; this crate only converts.
//!
//! The glue's pages remember the width and options they were laid out with, and ask again
//! with them for their timeline and hit tests: the chant's memo of recent layouts answers
//! without laying out again.

use std::fmt::Write as _;

use neuma::{Layout, LayoutOptions, SvgOptions, Utf16Index, Weights, json};

pub use neuma::{ChantOptions, Initial, LyricFont};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgOutput {
    /// One SVG document.
    Whole,
    /// The SVG in parts, a string per line (see [`neuma::Layout::svg_parts_with`]).
    Lines,
    /// As `Lines`, but a line whose SVG is the same as a line's of the last layout in parts
    /// is given as `\u{1}` and that line's index, for a page that kept them.
    ChangedLines,
}

/// A psalm the chant was set from, to set again on each update.
#[derive(Debug)]
struct Psalm {
    tone: neuma_tones::Tone,
    options: neuma_tones::PsalmOptions,
    /// `{ gabc, notes }` (see [`setting_json`]).
    json: String,
}

/// One score, with its answers kept as the JSON the glue reads.
#[derive(Debug)]
pub struct Chant {
    chant: neuma::Chant,
    /// Parse (or psalm-setting) and engrave diagnostics, as JSON.
    diagnostics: String,
    /// The library entry, as JSON, made when first asked for.
    summary: Option<String>,
    psalm: Option<Psalm>,
}

impl Chant {
    pub fn new(gabc: &str, options: ChantOptions) -> Chant {
        Chant::wrap(neuma::Chant::with_options(gabc, options), None)
    }

    /// Psalm text set to `tone` and engraved, its spans in the text.
    pub fn from_psalm(text: &str, tone: neuma_tones::Tone, psalm: neuma_tones::PsalmOptions, options: ChantOptions) -> Chant {
        let (chant, json) = set_psalm(text, &tone, &psalm, options);
        let psalm = Psalm {
            tone,
            options: psalm,
            json,
        };
        Chant::wrap(chant, Some(psalm))
    }

    fn wrap(chant: neuma::Chant, psalm: Option<Psalm>) -> Chant {
        let mut out = Chant {
            chant,
            diagnostics: String::new(),
            summary: None,
            psalm,
        };
        out.refresh();
        out
    }

    /// Replaces the score with `src` (GABC, or psalm text for a chant set from a psalm),
    /// engraved with the same options, as an editor does on each change.
    pub fn update(&mut self, src: &str) {
        match &mut self.psalm {
            None => self.chant.update(src),
            Some(p) => {
                let s = neuma_tones::psalm(src, &p.tone, &p.options);
                p.json = psalm_json(&s, &Utf16Index::new(src));
                self.chant.update_score(s.score, src, s.diagnostics);
            }
        }
        self.refresh();
    }

    /// Engraves again with `options`.
    pub fn set_options(&mut self, options: ChantOptions) {
        self.chant.set_options(options);
        self.refresh();
    }

    fn refresh(&mut self) {
        self.diagnostics.clear();
        json::diagnostics(&mut self.diagnostics, self.chant.diagnostics(), Some(self.chant.utf16()));
        self.summary = None;
    }

    pub fn diagnostics_json(&self) -> &str {
        &self.diagnostics
    }

    /// For a chant set from a psalm, `{ gabc, notes }`.
    pub fn psalm_json(&self) -> Option<&str> {
        self.psalm.as_ref().map(|p| p.json.as_str())
    }

    pub fn summary_json(&mut self) -> &str {
        let chant = &self.chant;
        self.summary.get_or_insert_with(|| {
            let mut out = String::new();
            json::summary(&mut out, &chant.summary());
            out
        })
    }

    /// Lays the score out at `width` output units: `{ width, height }`, a NUL, then the SVG
    /// (for `Lines` and `ChangedLines`, as `svg_lines` gives it).
    pub fn layout(&self, width: f32, opts: &LayoutOptions, svg: &SvgOptions, mode: SvgOutput) -> String {
        let layout = self.chant.layout_with(width, opts);
        let (w, h) = layout.size();
        let mut out = String::from("{\"width\":");
        json::number(&mut out, w);
        out.push_str(",\"height\":");
        json::number(&mut out, h);
        out.push_str("}\0");
        match mode {
            SvgOutput::Whole => out.push_str(&layout.svg_with(svg)),
            SvgOutput::Lines | SvgOutput::ChangedLines => self.svg_lines(&mut out, &layout, svg, mode == SvgOutput::ChangedLines),
        }
        out
    }

    /// Head, definitions and the initial, then each line's top and SVG, all separated by
    /// NULs, which SVG text never contains.
    fn svg_lines(&self, out: &mut String, layout: &Layout, svg: &SvgOptions, changed: bool) {
        let parts = self.chant.svg_parts(layout, svg);
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

    /// The timeline of the layout at `width` and `opts`, timed with `weights`, as JSON.
    pub fn timeline_json(&self, width: f32, opts: &LayoutOptions, weights: &Weights) -> String {
        let timeline = self.chant.layout_with(width, opts).timeline_with(weights);
        let mut out = String::with_capacity(timeline.notes.len() * 460 + 1024);
        json::timeline(&mut out, &timeline, Some(self.chant.utf16()));
        out
    }

    pub fn note_at(&self, width: f32, opts: &LayoutOptions, x: f32, y: f32) -> Option<u32> {
        self.chant.layout_with(width, opts).note_at(x, y)
    }

    /// The note, bar or syllable under (`x`, `y`), as JSON (`null` for none).
    pub fn source_at_json(&self, width: f32, opts: &LayoutOptions, x: f32, y: f32) -> String {
        let layout = self.chant.layout_with(width, opts);
        let mut out = String::new();
        match layout.source_at(x, y) {
            Some(e) => json::element(&mut out, e, self.chant.utf16()),
            None => out.push_str("null"),
        }
        out
    }

    /// What to highlight for a caret at `offset` (UTF-16 units if `utf16`, else bytes), as a
    /// JSON array, most specific first. Past the end is at the end.
    pub fn elements_at_json(&self, width: f32, opts: &LayoutOptions, offset: usize, utf16: bool) -> String {
        let layout = self.chant.layout_with(width, opts);
        let byte = if utf16 { self.chant.utf16().to_utf8(offset) } else { offset };
        let mut out = String::from("[");
        for (i, e) in layout.elements_at(byte).into_iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            json::element(&mut out, e, self.chant.utf16());
        }
        out.push(']');
        out
    }

    /// The source this Chant was made or last updated from.
    pub fn source(&self) -> &str {
        self.chant.source()
    }
}

/// Sets `text` to `tone` and engraves it: the chant, and the setting's `{ gabc, notes }`.
fn set_psalm(text: &str, tone: &neuma_tones::Tone, psalm: &neuma_tones::PsalmOptions, options: ChantOptions) -> (neuma::Chant, String) {
    let s = neuma_tones::psalm(text, tone, psalm);
    let json = psalm_json(&s, &Utf16Index::new(text));
    (s.into_chant(options), json)
}

fn part_name(k: neuma_tones::VersePart) -> &'static str {
    match k {
        neuma_tones::VersePart::Flex => "flex",
        neuma_tones::VersePart::Mediant => "mediant",
        neuma_tones::VersePart::Termination => "termination",
    }
}

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
pub fn setting_json(out: &mut String, s: &neuma_tones::PsalmSetting, text: &Utf16Index) {
    out.push_str("{\"gabc\":");
    json::string(out, &s.gabc);
    notes_json(out, s, text);
    out.push_str(",\"diagnostics\":");
    json::diagnostics(out, &s.diagnostics, Some(text));
    out.push('}');
}

/// `{ gabc, notes }`: a setting without its diagnostics, which a chant set from it has.
fn psalm_json(s: &neuma_tones::PsalmSetting, text: &Utf16Index) -> String {
    let mut out = String::from("{\"gabc\":");
    json::string(&mut out, &s.gabc);
    notes_json(&mut out, s, text);
    out.push('}');
    out
}

fn notes_json(out: &mut String, s: &neuma_tones::PsalmSetting, text: &Utf16Index) {
    out.push_str(",\"notes\":[");
    for (i, n) in s.notes.iter().enumerate() {
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

#[cfg(target_arch = "wasm32")]
mod ffi;

#[cfg(test)]
mod tests {
    use super::*;

    fn split(page: &str) -> (&str, &str) {
        page.split_once('\0').unwrap()
    }

    #[test]
    fn lays_out_and_hit_tests() {
        let c = Chant::new(
            "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)",
            ChantOptions::default(),
        );
        assert_eq!(c.diagnostics_json(), "[]");
        let opts = LayoutOptions::default();
        let page = c.layout(400.0, &opts, &SvgOptions::default(), SvgOutput::Whole);
        let (size, svg) = split(&page);
        assert!(size.starts_with(r#"{"width":400,"height":"#) && svg.starts_with("<svg"), "{size}");
        let j = c.timeline_json(400.0, &opts, &Weights::default());
        assert!(j.contains("\"kind\":\"mediant\"") && j.contains("\"kind\":\"double\""), "{j}");
        assert!(
            j.contains(r#""sourceStart":20,"sourceEnd":21,"sourceUtf16Start":20,"sourceUtf16End":21"#),
            "{j}"
        );
        let timeline = c.chant.layout(400.0).timeline();
        let n = &timeline.notes[0];
        assert_eq!(c.note_at(400.0, &opts, n.cx, n.cy), Some(0));
        // A one-line preview laid out since doesn't change what the page finds.
        let _ = c.layout(400.0, &opts.with_max_lines(1), &SvgOptions::default(), SvgOutput::Lines);
        assert_eq!(c.note_at(400.0, &opts, n.cx, n.cy), Some(0));
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
        let page = c.layout(400.0, &opts, &SvgOptions::default(), SvgOutput::Lines);
        assert_eq!(split(&page).1.split('\0').count(), 3 + 2);
        let b = c.elements_at_json(400.0, &opts, 12, true);
        assert!(
            b.starts_with(r#"[{"kind":"note","index":1,"start":13,"end":14,"utf16Start":12,"utf16End":13,"#),
            "{b}"
        );
        assert_eq!(c.elements_at_json(400.0, &opts, 13, false), b);
        assert_eq!(
            c.elements_at_json(400.0, &opts, usize::MAX, false),
            c.elements_at_json(400.0, &opts, 16, true)
        );
        assert_eq!(c.source_at_json(400.0, &opts, -5.0, -5.0), "null");
        assert!(c.summary_json().contains("\"notes\":2"));
    }

    #[test]
    fn psalm_notes_count_the_text_in_both_units() {
        let text = "Bléssed is he * that cómeth.";
        let tone = neuma_tones::Tone::named("8.G").unwrap();
        let s = neuma_tones::psalm(text, tone, &neuma_tones::PsalmOptions::default());
        let mut out = String::new();
        setting_json(&mut out, &s, &Utf16Index::new(text));
        assert!(
            out.contains(r#""sourceStart":0,"sourceEnd":8,"sourceUtf16Start":0,"sourceUtf16End":7}"#),
            "{out}"
        );
        let mut c = Chant::from_psalm(text, tone.clone(), neuma_tones::PsalmOptions::default(), ChantOptions::default());
        assert!(
            c.psalm_json().unwrap().starts_with(r#"{"gabc":"%%\n(c4) Bléssed(k)"#),
            "{}",
            c.psalm_json().unwrap()
        );
        assert_eq!(c.source(), text);
        c.update("Bléssed is he * that cómeth, and is.");
        assert!(c.psalm_json().unwrap().contains("is.(g)"), "{}", c.psalm_json().unwrap());
    }
}
