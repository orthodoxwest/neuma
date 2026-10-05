//! End-to-end: every corpus score engraves, lays out at several widths and renders.

use std::fs;
use std::path::Path;

use neuma::{ApproxMeasure, Item, LayoutOptions, StyleOptions, SvgOptions, Weights, parse};

fn corpus() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut out: Vec<(String, String)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "gabc"))
        .map(|p| (p.display().to_string(), fs::read_to_string(&p).unwrap()))
        .collect();
    out.sort();
    out
}

#[test]
fn lays_out_within_width() {
    for (name, src) in corpus() {
        let score = parse(&src).score;
        let eng = score.engrave(&ApproxMeasure, &StyleOptions::default());
        let total_notes = score
            .syllables
            .iter()
            .flat_map(|s| &s.notation)
            .filter(|f| matches!(f, neuma::score::Figure::Note(_)))
            .count();
        for width in [320.0, 700.0, 1200.0] {
            let layout = eng.layout(width, &LayoutOptions::default());
            let (w, h) = layout.size();
            assert!(w <= width + 0.01, "{name} at {width}: {w}");
            assert!(h > 0.0);
            let list = layout.display();
            for item in &list.items {
                if let Item::Glyph { x, .. } = item {
                    assert!(*x <= w + 0.01, "{name} at {width}: glyph at {x}");
                }
            }
            let map = layout.notes(&Weights::SOLESMES);
            assert_eq!(map.notes.len(), total_notes, "{name}");
            let mut last = (0, f32::MIN);
            for n in &map.notes {
                if n.line == last.0 {
                    assert!(n.x >= last.1 - 0.01, "{name} at {width}: note {} goes backwards", n.id);
                }
                last = (n.line, n.x);
            }
        }
    }
}

#[test]
fn narrower_means_more_lines() {
    let (_, src) = corpus().into_iter().find(|(n, _)| n.contains("psalm-134")).unwrap();
    let eng = parse(&src).score.engrave(&ApproxMeasure, &StyleOptions::default());
    let wide = eng.layout(1200.0, &LayoutOptions::default()).line_count();
    let narrow = eng.layout(320.0, &LayoutOptions::default()).line_count();
    assert!(narrow > wide, "{narrow} vs {wide}");
}

#[test]
fn output_is_deterministic() {
    for (_, src) in corpus() {
        let score = parse(&src).score;
        let a = score
            .engrave(&ApproxMeasure, &StyleOptions::default())
            .layout(500.0, &LayoutOptions::default())
            .svg(&SvgOptions::default());
        let b = score
            .engrave(&ApproxMeasure, &StyleOptions::default())
            .layout(500.0, &LayoutOptions::default())
            .svg(&SvgOptions::default());
        assert_eq!(a, b);
        assert!(a.starts_with("<svg") && a.ends_with("</svg>"));
    }
}
