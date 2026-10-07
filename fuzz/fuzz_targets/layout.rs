//! The whole pipeline: parse, engrave with the built-in metrics, lay out at two widths chosen
//! from the input, then draw the display list, the timeline and the SVG. Layout must finish
//! (libFuzzer's timeout catches a hang) with a finite size.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neuma::{Initial, LastLine, LayoutOptions, LyricFont, MetricsTable, StyleOptions};
use std::sync::LazyLock;

static METRICS: LazyLock<MetricsTable> =
    LazyLock::new(|| MetricsTable::from_bytes(LyricFont::Google.metrics_bytes()).expect("built-in metrics"));

/// A width from two bytes: mostly ordinary page widths, sometimes an extreme.
fn width(a: u8, b: u8) -> f32 {
    match a % 16 {
        0 => 0.0,
        1 => 1.0,
        2 => -100.0,
        3 => f32::INFINITY,
        4 => f32::NAN,
        5 => 1e9,
        _ => 40.0 + (u16::from(a) << 8 | u16::from(b)) as f32 % 2000.0,
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else { return };
    // The options come from the input's last bytes, which stay part of the GABC too, so plain
    // .gabc files work as seeds.
    let tail = |i: usize| data.len().checked_sub(i + 1).map_or(0, |k| data[k]);
    let style = StyleOptions::default()
        .with_initial(match tail(0) % 5 {
            0 => Initial::None,
            n => Initial::Lines(n),
        })
        .with_annotation(tail(1) % 2 == 0);
    let parsed = neuma::parse(src);
    let engraving = parsed.score.engrave(&*METRICS, &style);
    for (w, k) in [(width(tail(2), tail(3)), 0), (width(tail(4), tail(5)), 1)] {
        let opts = LayoutOptions::default()
            .with_scale(if k == 0 { 6.0 } else { 1.0 + f32::from(tail(6) % 16) })
            .with_last_line(if tail(7) % 2 == 0 { LastLine::Ragged } else { LastLine::Justified })
            .with_max_lines(usize::from(tail(8) % 4));
        let layout = engraving.layout_with(w, &opts);
        let (lw, lh) = layout.size();
        assert!(lw.is_finite() && lh.is_finite(), "size {lw} x {lh} at {w}");
        let _ = layout.display();
        let timeline = layout.timeline();
        if let Some(n) = timeline.notes.first() {
            let _ = layout.note_at(n.cx, n.cy);
            let _ = layout.source_at(n.cx, n.cy);
            let _ = layout.elements_at(n.span.start);
        }
        let _ = layout.svg();
    }
});
