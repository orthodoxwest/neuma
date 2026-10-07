//! The browser package's engine side: a [`Chant`] engraves a score once and lays it out at
//! any width, caching the SVG and the playback timeline. On `wasm32` the `ffi` module
//! exports it over a plain C ABI that `js/neuma.mjs` wraps, so no generated glue is needed
//! and the module can be inlined into a single file or turned into plain JavaScript.

pub mod json;

use neuma::score::Header;
use neuma::{
    EngraveCache, Engraving, Initial, LastLine, LayoutCache, LayoutOptions, NoteMap, NoteRef, SourceMap, StyleOptions, SvgOptions,
    Utf16Index, Weights, parse,
};

pub use neuma::Font;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChantOptions {
    /// Drop-cap height in staves; 0 for none.
    pub initial: u8,
    pub annotation: bool,
    pub lyric_size: f32,
    pub font: Font,
}

impl Default for ChantOptions {
    fn default() -> ChantOptions {
        let style = StyleOptions::default();
        ChantOptions {
            initial: 1,
            annotation: style.annotation,
            lyric_size: style.lyric_size,
            font: Font::default(),
        }
    }
}

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
    /// The SVG in parts, a string per line (see [`neuma::Layout::svg_parts`]).
    Lines,
}

/// One score: engraved once, laid out on demand.
#[derive(Debug)]
pub struct Chant {
    options: ChantOptions,
    source: String,
    utf16: Utf16Index,
    header: Header,
    /// The score and its engraving, kept to engrave the next edit from.
    engraved: EngraveCache,
    /// Parse and engrave diagnostics, as JSON.
    diagnostics: String,
    /// The catalogue entry, as JSON, made when first asked for.
    summary: Option<String>,
    svg: String,
    notes: Option<NoteMap>,
    sources: Option<SourceMap>,
    /// The layout as JSON: size, lines, notes and pauses (without the SVG).
    layout_json: String,
    /// The line breaker's work, reused by the next layout after an edit.
    layout_cache: LayoutCache,
}

impl Chant {
    pub fn new(gabc: &str, opts: ChantOptions) -> Chant {
        let mut chant = Chant {
            options: opts,
            source: String::new(),
            utf16: Utf16Index::default(),
            header: Header::default(),
            engraved: EngraveCache::default(),
            diagnostics: String::new(),
            summary: None,
            svg: String::new(),
            notes: None,
            sources: None,
            layout_json: String::new(),
            layout_cache: LayoutCache::default(),
        };
        chant.update(gabc);
        chant
    }

    /// Replaces the score with `gabc`, engraved with the same options, as an editor does on
    /// each change. The last layout is dropped.
    pub fn update(&mut self, gabc: &str) {
        let opts = self.options;
        let parsed = parse(gabc);
        let style = StyleOptions {
            initial: if opts.initial == 0 {
                Initial::None
            } else {
                Initial::Lines(opts.initial)
            },
            annotation: opts.annotation,
            lyric_size: if opts.lyric_size.is_finite() && opts.lyric_size > 0.0 {
                opts.lyric_size
            } else {
                StyleOptions::default().lyric_size
            },
            ..StyleOptions::default()
        };
        self.header = parsed.score.header.clone();
        let engraving = self.engraved.engrave(parsed.score, opts.font.table(), &style);
        self.source = gabc.to_string();
        self.utf16 = Utf16Index::new(gabc);
        let mut all = parsed.diagnostics;
        all.extend(engraving.diagnostics.iter().cloned());
        self.diagnostics.clear();
        json::diagnostics(&mut self.diagnostics, &all, Some(&self.utf16));
        self.summary = None;
        self.svg.clear();
        self.notes = None;
        self.sources = None;
        self.layout_json.clear();
    }

    pub fn diagnostics_json(&self) -> &str {
        &self.diagnostics
    }

    pub fn summary_json(&mut self) -> &str {
        self.summary.get_or_insert_with(|| {
            let mut out = String::new();
            json::summary(&mut out, &engraving(&self.engraved).summary(&self.header));
            out
        })
    }

    /// Lays the score out at `width` output units and caches the SVG and timeline.
    pub fn layout(&mut self, width: f32, opts: &LayoutOptions, weights: &Weights, svg: &SvgOptions) {
        self.layout_with(width, opts, weights, svg, Outputs::default());
    }

    /// As [`Chant::layout`], choosing what to produce.
    pub fn layout_with(&mut self, width: f32, opts: &LayoutOptions, weights: &Weights, svg: &SvgOptions, outputs: Outputs) {
        let layout = engraving(&self.engraved).layout_cached(width, opts, &mut self.layout_cache);
        self.svg = match outputs.svg {
            SvgOutput::Whole => layout.svg(svg),
            SvgOutput::Lines => {
                // Head, definitions and the initial, then each line's top and SVG, all
                // separated by NULs, which SVG text never contains.
                let parts = layout.svg_parts(svg);
                let mut out = String::with_capacity(parts.lines.iter().map(|l| l.svg.len() + 12).sum::<usize>() + 4096);
                for s in [&parts.head, &parts.defs, &parts.rest] {
                    out.push_str(s);
                    out.push('\0');
                }
                for line in &parts.lines {
                    json::number(&mut out, line.top);
                    out.push('\0');
                    out.push_str(&line.svg);
                    out.push('\0');
                }
                out.pop();
                out
            }
        };
        let map = layout.notes(weights);
        let (w, h) = layout.size();
        let mut out = String::with_capacity(if outputs.timeline { map.notes.len() * 420 + 1024 } else { 64 });
        out.push_str("{\"width\":");
        json::number(&mut out, w);
        out.push_str(",\"height\":");
        json::number(&mut out, h);
        if outputs.timeline {
            out.push_str(",\"timeline\":");
            json::note_map(&mut out, &map);
        }
        out.push('}');
        self.layout_json = out;
        self.notes = Some(map);
        self.sources = Some(layout.source_map());
    }

    pub fn svg(&self) -> &str {
        &self.svg
    }

    pub fn layout_json(&self) -> &str {
        &self.layout_json
    }

    pub fn note_at(&self, x: f32, y: f32) -> Option<NoteRef> {
        self.notes.as_ref()?.note_at(x, y)
    }

    /// The note, bar or syllable under (`x`, `y`) in the last layout, as JSON (`null` for
    /// none).
    pub fn source_at_json(&self, x: f32, y: f32) -> String {
        let mut out = String::new();
        match self.sources.as_ref().and_then(|m| m.source_at(x, y)) {
            Some(e) => json::element(&mut out, e, &self.utf16),
            None => out.push_str("null"),
        }
        out
    }

    /// What to highlight for a caret at `offset` (UTF-16 units if `utf16`, else UTF-8 bytes)
    /// in the last layout, as a JSON array, most specific first.
    pub fn elements_at_json(&self, offset: usize, utf16: bool) -> String {
        // An offset past the end is the end.
        let byte = if utf16 {
            self.utf16.to_utf8(offset)
        } else {
            offset.min(self.source.len())
        };
        let mut out = String::from("[");
        if let Some(map) = &self.sources {
            for (i, e) in map.at(byte).into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json::element(&mut out, e, &self.utf16);
            }
        }
        out.push(']');
        out
    }

    /// The source this Chant was made or last updated from.
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Weights from a flat list, in the order the JS glue sends them; NaN keeps the default.
/// The engraving a chant's cache holds: there is one from the chant's first update on.
fn engraving(cache: &EngraveCache) -> &Engraving {
    cache.engraving().expect("a chant is engraved when made")
}

pub fn weights_from(values: &[f32]) -> Weights {
    let mut w = Weights::SOLESMES;
    let slots: [&mut f32; 10] = [
        &mut w.note,
        &mut w.mora,
        &mut w.episema,
        &mut w.virgula,
        &mut w.minima,
        &mut w.minor,
        &mut w.maior,
        &mut w.finalis,
        &mut w.mediant,
        &mut w.flex,
    ];
    for (slot, v) in slots.into_iter().zip(values) {
        if v.is_finite() && *v >= 0.0 {
            *slot = *v;
        }
    }
    w
}

pub fn last_line(code: u32) -> LastLine {
    if code == 1 { LastLine::Justified } else { LastLine::Ragged }
}

#[cfg(target_arch = "wasm32")]
mod ffi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_and_caches() {
        let mut c = Chant::new(
            "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)",
            ChantOptions::default(),
        );
        assert_eq!(c.diagnostics_json(), "[]");
        c.layout(400.0, &LayoutOptions::default(), &weights_from(&[]), &SvgOptions::default());
        assert!(c.svg().starts_with("<svg"));
        let j = c.layout_json();
        assert!(j.contains("\"kind\":\"mediant\"") && j.contains("\"kind\":\"double\""), "{j}");
        let n = &c.notes.as_ref().unwrap().notes[0];
        assert_eq!(c.note_at(n.x, n.y), Some(0));
        let w = weights_from(&[f32::NAN, 3.0]);
        assert_eq!((w.note, w.mora), (1.0, 3.0));
    }

    #[test]
    fn updates_and_answers_for_editors() {
        let mut c = Chant::new("(c4) a-(g)", ChantOptions::default());
        assert!(
            c.diagnostics_json()
                .contains(r#""fix":{"start":6,"end":7,"from":6,"to":7,"insert":"","title":"#)
        );
        // `é` is two bytes and one UTF-16 unit.
        c.update("(c4) é(g) b(h)");
        assert_eq!(c.diagnostics_json(), "[]");
        assert!(c.layout_json().is_empty());
        let outputs = Outputs {
            timeline: false,
            svg: SvgOutput::Lines,
        };
        c.layout_with(
            400.0,
            &LayoutOptions::default(),
            &Weights::default(),
            &SvgOptions::default(),
            outputs,
        );
        assert!(!c.layout_json().contains("timeline"));
        assert_eq!(c.svg().split('\0').count(), 3 + 2);
        let b = c.elements_at_json(12, true);
        assert!(
            b.starts_with(r#"[{"kind":"note","index":1,"start":13,"end":14,"from":12,"to":13,"#),
            "{b}"
        );
        assert_eq!(c.elements_at_json(13, false), b);
        assert_eq!(c.source_at_json(-5.0, -5.0), "null");
        assert!(c.summary_json().contains("\"notes\":2"));
    }
}
