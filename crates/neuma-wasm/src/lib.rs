//! The browser package's engine side: a thin wrapper that gives [`neuma::Chant`]'s answers as
//! the JSON and strings `js/neuma.mjs` reads. On `wasm32` the `ffi` module exports it over a
//! plain C ABI, so no generated glue is needed and the module can be inlined into a single
//! file. Options, defaults and sanitizing are the core's; this crate only converts.

use std::fmt::Write as _;

use neuma::{LayoutOptions, OffsetUnit, SvgOptions, Utf16Index, Weights, json};

pub use neuma::{ChantOptions, Initial, LyricFont};

/// What a layout returns besides its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outputs {
    /// The playback timeline in the layout JSON. An editor that only shows the score can
    /// skip it: on long scores it is most of the JSON.
    pub timeline: bool,
    pub svg: SvgOutput,
}

impl Default for Outputs {
    fn default() -> Outputs {
        Outputs {
            timeline: true,
            svg: SvgOutput::Whole,
        }
    }
}

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

/// One score, with its answers kept as the JSON the glue reads.
#[derive(Debug)]
pub struct Chant {
    chant: neuma::Chant,
    /// Parse and engrave diagnostics, as JSON.
    diagnostics: String,
    /// The library entry, as JSON, made when first asked for.
    summary: Option<String>,
    svg: String,
    /// The layout as JSON: size and timeline (without the SVG).
    layout_json: String,
}

impl Chant {
    pub fn new(gabc: &str, options: ChantOptions) -> Chant {
        let chant = neuma::Chant::with_options(gabc, options);
        let mut out = Chant {
            chant,
            diagnostics: String::new(),
            summary: None,
            svg: String::new(),
            layout_json: String::new(),
        };
        out.refresh();
        out
    }

    /// Replaces the score with `gabc`, engraved with the same options, as an editor does on
    /// each change. The last layout is dropped.
    pub fn update(&mut self, gabc: &str) {
        self.chant.update(gabc);
        self.refresh();
    }

    fn refresh(&mut self) {
        self.diagnostics.clear();
        json::diagnostics(&mut self.diagnostics, self.chant.diagnostics(), Some(self.chant.utf16()));
        self.summary = None;
        self.svg.clear();
        self.layout_json.clear();
    }

    pub fn diagnostics_json(&self) -> &str {
        &self.diagnostics
    }

    pub fn summary_json(&mut self) -> &str {
        let chant = &self.chant;
        self.summary.get_or_insert_with(|| {
            let mut out = String::new();
            json::summary(&mut out, &chant.summary());
            out
        })
    }

    /// Lays the score out at `width` output units, keeping the SVG and the layout JSON.
    pub fn layout(&mut self, width: f32, opts: &LayoutOptions, weights: &Weights, svg: &SvgOptions, outputs: Outputs) {
        let layout = self.chant.layout_with(width, opts);
        let size = layout.size();
        let timeline = outputs.timeline.then(|| layout.timeline_with(weights));
        self.svg = match outputs.svg {
            SvgOutput::Whole => layout.svg_with(svg),
            SvgOutput::Lines | SvgOutput::ChangedLines => self.svg_lines(svg, outputs.svg == SvgOutput::ChangedLines),
        };
        self.layout_json.clear();
        json::layout(&mut self.layout_json, size, timeline.as_ref(), Some(self.chant.utf16()));
    }

    /// Head, definitions and the initial, then each line's top and SVG, all separated by
    /// NULs, which SVG text never contains.
    fn svg_lines(&mut self, svg: &SvgOptions, changed: bool) -> String {
        let parts = self.chant.svg_parts_with(svg);
        let reused = if changed { self.chant.reused_svg_lines() } else { &[] };
        let mut out = String::with_capacity(parts.lines.iter().map(|l| l.svg.len() + 12).sum::<usize>() + 4096);
        for s in [&parts.head, &parts.defs, &parts.rest] {
            out.push_str(s);
            out.push('\0');
        }
        for (i, line) in parts.lines.iter().enumerate() {
            json::number(&mut out, line.top);
            out.push('\0');
            match reused.get(i).copied().flatten() {
                Some(k) => {
                    out.push('\u{1}');
                    let _ = write!(out, "{k}");
                }
                None => out.push_str(&line.svg),
            }
            out.push('\0');
        }
        out.pop();
        out
    }

    pub fn svg(&self) -> &str {
        &self.svg
    }

    pub fn layout_json(&self) -> &str {
        &self.layout_json
    }

    pub fn note_at(&self, x: f32, y: f32) -> Option<u32> {
        self.chant.note_at(x, y)
    }

    /// The note, bar or syllable under (`x`, `y`) in the last layout, as JSON (`null` for
    /// none).
    pub fn source_at_json(&self, x: f32, y: f32) -> String {
        let mut out = String::new();
        match self.chant.source_at(x, y) {
            Some(e) => json::element(&mut out, e, self.chant.utf16()),
            None => out.push_str("null"),
        }
        out
    }

    /// What to highlight for a caret at `offset` in the last layout, as a JSON array, most
    /// specific first.
    pub fn elements_at_json(&self, offset: usize, unit: OffsetUnit) -> String {
        let mut out = String::from("[");
        for (i, e) in self.chant.elements_at(offset, unit).into_iter().enumerate() {
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

/// A psalm setting: `{ gabc, notes: [{ verse, number, part, role, start, end, utf16Start,
/// utf16End }], diagnostics }`. `notes[i]` describes note `i` of the engraved score, and its
/// offsets are the sung syllable's in `text`.
pub fn setting_json(out: &mut String, s: &neuma_tones::PsalmSetting, text: &Utf16Index) {
    out.push_str("{\"gabc\":");
    json::string(out, &s.gabc);
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
        json::span(out, &n.source, Some(text));
        out.push('}');
    }
    out.push_str("],\"diagnostics\":");
    json::diagnostics(out, &s.diagnostics, Some(text));
    out.push('}');
}

/// A pointing: `{ text, halves: [{ verse, part, confidence, kept }], diagnostics }`.
pub fn pointing_json(out: &mut String, p: &neuma_tones::Pointing, text: &Utf16Index) {
    out.push_str("{\"text\":");
    json::string(out, &p.text());
    out.push_str(",\"halves\":[");
    for (i, h) in p.halves.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, r#"{{"verse":{},"part":"{}","confidence":"#, h.verse, part_name(h.part));
        json::number(out, h.confidence);
        let _ = write!(out, r#","kept":{}}}"#, h.kept);
    }
    out.push_str("],\"diagnostics\":");
    json::diagnostics(out, &p.pointed.diagnostics, Some(text));
    out.push('}');
}

#[cfg(target_arch = "wasm32")]
mod ffi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_and_hit_tests() {
        let mut c = Chant::new(
            "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)",
            ChantOptions::default(),
        );
        assert_eq!(c.diagnostics_json(), "[]");
        let opts = LayoutOptions::default();
        c.layout(400.0, &opts, &Weights::default(), &SvgOptions::default(), Outputs::default());
        assert!(c.svg().starts_with("<svg"));
        let j = c.layout_json();
        assert!(j.contains("\"kind\":\"mediant\"") && j.contains("\"kind\":\"double\""), "{j}");
        assert!(
            j.contains(r#""sourceStart":20,"sourceEnd":21,"sourceUtf16Start":20,"sourceUtf16End":21"#),
            "{j}"
        );
        let timeline = c.chant.last_layout().unwrap().timeline();
        let n = &timeline.notes[0];
        assert_eq!(c.note_at(n.cx, n.cy), Some(0));
        // Without the timeline, hit tests answer all the same.
        let lines = Outputs {
            timeline: false,
            svg: SvgOutput::Lines,
        };
        c.layout(400.0, &opts, &Weights::default(), &SvgOptions::default(), lines);
        assert!(!c.layout_json().contains("timeline"));
        assert_eq!(c.note_at(n.cx, n.cy), Some(0));
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
        assert!(c.layout_json().is_empty());
        let outputs = Outputs {
            timeline: false,
            svg: SvgOutput::Lines,
        };
        c.layout(
            400.0,
            &LayoutOptions::default(),
            &Weights::default(),
            &SvgOptions::default(),
            outputs,
        );
        assert_eq!(c.svg().split('\0').count(), 3 + 2);
        let b = c.elements_at_json(12, OffsetUnit::Utf16);
        assert!(
            b.starts_with(r#"[{"kind":"note","index":1,"start":13,"end":14,"utf16Start":12,"utf16End":13,"#),
            "{b}"
        );
        assert_eq!(c.elements_at_json(13, OffsetUnit::Utf8), b);
        assert_eq!(c.source_at_json(-5.0, -5.0), "null");
        assert!(c.summary_json().contains("\"notes\":2"));
    }

    #[test]
    fn psalm_notes_count_the_text_in_both_units() {
        let text = "Bléssed is he * that cómeth.";
        let tone = neuma_tones::Tone::named("8.G").unwrap();
        let s = neuma_tones::psalm(text, tone, &neuma_tones::PsalmOptions::default());
        let mut out = String::new();
        setting_json(&mut out, &s, &Utf16Index::new(text));
        assert!(out.contains(r#""start":0,"end":8,"utf16Start":0,"utf16End":7}"#), "{out}");
    }
}
