//! The browser package's engine side: a [`Chant`] engraves a score once and lays it out at
//! any width, caching the SVG and the playback timeline. On `wasm32` the `ffi` module
//! exports it over a plain C ABI that `js/neuma.mjs` wraps, so no generated glue is needed
//! and the module can be inlined into a single file or turned into plain JavaScript.

pub mod json;

use neuma::{Engraving, Initial, LastLine, LayoutOptions, MetricsTable, NoteMap, NoteRef, StyleOptions, SvgOptions, Weights, parse};

/// The metrics for the lyric face the SVG names (EB Garamond).
const EB_GARAMOND: &[u8] = include_bytes!("../../neuma-metrics/tables/eb-garamond-12.bin");

thread_local! {
    static METRICS: MetricsTable = MetricsTable::from_bytes(EB_GARAMOND).unwrap_or_default();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChantOptions {
    /// Drop-cap height in staves; 0 for none.
    pub initial: u8,
    pub annotation: bool,
    pub lyric_size: f32,
}

impl Default for ChantOptions {
    fn default() -> ChantOptions {
        let style = StyleOptions::default();
        ChantOptions {
            initial: 1,
            annotation: style.annotation,
            lyric_size: style.lyric_size,
        }
    }
}

/// One score: engraved once, laid out on demand.
#[derive(Debug)]
pub struct Chant {
    engraving: Engraving,
    /// Parse and engrave diagnostics, as JSON.
    diagnostics: String,
    svg: String,
    notes: Option<NoteMap>,
    /// The layout as JSON: size, lines, notes and pauses (without the SVG).
    layout_json: String,
}

impl Chant {
    pub fn new(gabc: &str, opts: ChantOptions) -> Chant {
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
        let engraving = METRICS.with(|m| parsed.score.engrave(m, &style));
        let mut all = parsed.diagnostics;
        all.extend(engraving.diagnostics.iter().cloned());
        let mut diagnostics = String::new();
        json::diagnostics(&mut diagnostics, &all);
        Chant {
            engraving,
            diagnostics,
            svg: String::new(),
            notes: None,
            layout_json: String::new(),
        }
    }

    pub fn diagnostics_json(&self) -> &str {
        &self.diagnostics
    }

    /// Lays the score out at `width` output units and caches the SVG and timeline.
    pub fn layout(&mut self, width: f32, opts: &LayoutOptions, weights: &Weights, svg: &SvgOptions) {
        let layout = self.engraving.layout(width, opts);
        self.svg = layout.svg(svg);
        let map = layout.notes(weights);
        let (w, h) = layout.size();
        let mut out = String::from("{\"width\":");
        json::number(&mut out, w);
        out.push_str(",\"height\":");
        json::number(&mut out, h);
        out.push_str(",\"timeline\":");
        json::note_map(&mut out, &map);
        out.push('}');
        self.layout_json = out;
        self.notes = Some(map);
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
}

/// Weights from a flat list, in the order the JS glue sends them; NaN keeps the default.
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
}
