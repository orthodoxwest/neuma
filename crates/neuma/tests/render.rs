//! End-to-end: every corpus score engraves, lays out at several widths and renders.

use std::fs;
use std::path::Path;

use neuma::{ApproxMeasure, Initial, Item, LayoutOptions, StyleOptions, SvgOptions, TextRole, Weights, parse};

/// Style without a drop cap, for tests about where syllables fall.
static NO_INITIAL: std::sync::LazyLock<StyleOptions> = std::sync::LazyLock::new(|| StyleOptions {
    initial: Initial::None,
    ..StyleOptions::default()
});

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

fn render(src: &str, width: f32) -> String {
    let eng = parse(src).score.engrave(&ApproxMeasure, &StyleOptions::default());
    eng.layout(width, &LayoutOptions::default()).svg(&SvgOptions::default())
}

#[test]
fn lyric_on_break_only_syllable_is_kept() {
    // The break follows the syllable, text and all, as in Gregorio.
    for (src, first_line) in [("(c4) A(g) men(z) (h)", ["A", "men"]), ("(c4) Ky(g)ri(z)e(h)", ["Ky", "ri"])] {
        let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
        let layout = eng.layout(400.0, &LayoutOptions::default());
        assert_eq!(layout.line_count(), 2, "{src}");
        let list = layout.display();
        let top: Vec<String> = list
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text { runs, baseline, role, .. } if *baseline < list.lines[0].bottom && *role != TextRole::Initial => {
                    Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .filter(|t| t != "-")
            .collect();
        assert_eq!(top, first_line, "{src}");
    }
}

#[test]
fn svg_drops_characters_xml_forbids() {
    let svg = render("(c4) A\u{1}B\u{7f}(g)", 400.0);
    assert!(!svg.contains('\u{1}'));
    let opts = SvgOptions {
        prefix: "x\"><script>".into(),
        font_family: "a}</style><script>".into(),
        ..SvgOptions::default()
    };
    let eng = parse("(c4) A(g)").score.engrave(&ApproxMeasure, &StyleOptions::default());
    let svg = eng.layout(400.0, &LayoutOptions::default()).svg(&opts);
    assert!(!svg.contains("<script"), "{svg}");
}

#[test]
fn non_finite_sizes_stay_finite() {
    let eng = parse("(c4) A(g)men(h) (::)")
        .score
        .engrave(&ApproxMeasure, &StyleOptions::default());
    for width in [f32::INFINITY, f32::NAN, -5.0, 1e9, 3e38] {
        for scale in [f32::INFINITY, f32::NAN, 0.0, 1e-38, 6.0] {
            let layout = eng.layout(
                width,
                &LayoutOptions {
                    scale,
                    ..LayoutOptions::default()
                },
            );
            let (w, h) = layout.size();
            assert!(w.is_finite() && h.is_finite(), "{width} {scale}: {w} {h}");
            assert!(!layout.svg(&SvgOptions::default()).contains("inf"));
        }
    }
}

#[test]
fn wide_layout_is_fast() {
    let src = format!("(c4) {}(::)", "la(g) ".repeat(2000));
    let eng = parse(&src).score.engrave(&ApproxMeasure, &StyleOptions::default());
    let t = std::time::Instant::now();
    let layout = eng.layout(1e9, &LayoutOptions::default());
    assert!(layout.line_count() > 0);
    // The quadratic breaker takes milliseconds here; the old cubic one took tens of seconds.
    // The bound is loose so a slow runner can't trip it.
    assert!(t.elapsed().as_secs() < 30, "{:?}", t.elapsed());
}

fn line_texts(src: &str, width: f32) -> Vec<Vec<String>> {
    let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let list = eng.layout(width, &LayoutOptions::default()).display();
    let mut out = vec![Vec::new(); list.lines.len()];
    for item in &list.items {
        if let Item::Text { runs, baseline, role, .. } = item
            && *role != TextRole::Initial
        {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            if let Some(li) = list.lines.iter().position(|l| *baseline >= l.top && *baseline <= l.bottom)
                && text != "-"
            {
                out[li].push(text);
            }
        }
    }
    out
}

#[test]
fn forced_breaks_keep_lines_balanced() {
    // Short syllables before a written break share a line rather than taking one each.
    assert_eq!(line_texts("(c4) A(g) B(h) C(gz) D(h)", 400.0), [vec!["A", "B", "C"], vec!["D"]]);
    // The lines before a mid-score break share the stretch; none is left nearly empty.
    let lines = line_texts(
        "(c4) Glo(g)ri(h)a(g) Pa(h)tri(g) et(h) Fi(g)li(h)o(g) et(h) Spi(g)ri(h)tu(g)i(h) San(g)cto(hz) Si(g)cut(h) e(g)rat(h) in(g)",
        300.0,
    );
    let before: Vec<usize> = lines.iter().take_while(|l| !l.contains(&"cto".to_string())).map(Vec::len).collect();
    let z_line = lines.iter().find(|l| l.contains(&"cto".to_string())).unwrap();
    assert!(z_line.len() > 2, "{lines:?}");
    assert!(before.iter().all(|&n| n > 2), "{lines:?}");
}

#[test]
fn initial_and_annotations() {
    let src = "annotation: Ant.;\nannotation: VIII G;\n%%\n(c4) Ky(g)ri(h)e(g) e(h)le(g)i(h)son(g) (::)";
    let eng = parse(src).score.engrave(&ApproxMeasure, &StyleOptions::default());
    let list = eng.layout(600.0, &LayoutOptions::default()).display();
    let texts: Vec<(TextRole, String, f32, f32)> = list
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text {
                runs, role, x, baseline, ..
            } => Some((*role, runs.iter().map(|r| r.text.as_str()).collect(), *x, *baseline)),
            _ => None,
        })
        .collect();
    let initial = texts.iter().find(|t| t.0 == TextRole::Initial).unwrap();
    assert_eq!(initial.1, "K");
    // The capital sits on the bottom staff line, left of the staff.
    let line = &list.lines[0];
    assert!((initial.3 - (line.staff + 3.0 * list.staff_space)).abs() < 0.01);
    let staff_left = list
        .items
        .iter()
        .find_map(|i| match i {
            Item::Rect {
                x,
                role: neuma::Ink::Staff,
                ..
            } => Some(*x),
            _ => None,
        })
        .unwrap();
    assert!(staff_left > initial.2);
    // The first lyric loses its capital; annotations stack above the staff, top line first.
    assert!(texts.iter().any(|t| t.0 == TextRole::Lyric && t.1 == "y"));
    let ann: Vec<&(TextRole, String, f32, f32)> = texts.iter().filter(|t| t.0 == TextRole::Annotation).collect();
    assert_eq!(ann.iter().map(|t| t.1.as_str()).collect::<Vec<_>>(), ["Ant.", "VIII G"]);
    assert!(ann[0].3 < ann[1].3 && ann[1].3 < line.staff - 3.0 * list.staff_space);
    assert!(ann[0].3 > 0.0, "annotations stay inside the layout");

    // Two-line initials indent the first two staves.
    let style = StyleOptions {
        initial: Initial::Lines(2),
        ..StyleOptions::default()
    };
    let eng = parse(&src.replace("(::)", &"la(g) ".repeat(120)))
        .score
        .engrave(&ApproxMeasure, &style);
    let list = eng.layout(500.0, &LayoutOptions::default()).display();
    assert!(list.lines.len() > 3);
    let lefts: Vec<f32> = list
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Rect {
                x,
                role: neuma::Ink::Staff,
                ..
            } => Some(*x),
            _ => None,
        })
        .step_by(4)
        .collect();
    assert!(lefts[0] > 0.0 && lefts[1] == lefts[0] && lefts[2] == 0.0, "{lefts:?}");
}

fn initial_item(list: &neuma::DisplayList) -> Option<(f32, f32, f32)> {
    list.items.iter().find_map(|i| match i {
        Item::Text {
            role: TextRole::Initial,
            x,
            baseline,
            size,
            ..
        } => Some((*x, *baseline, *size)),
        _ => None,
    })
}

#[test]
fn tall_initials_fit_the_staves_they_span() {
    let style = StyleOptions {
        initial: Initial::Lines(2),
        ..StyleOptions::default()
    };
    let cases = [
        // Fits on one line at this width, so the capital spans one staff.
        (
            "annotation: Ant.;\nmode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) e(h)le(g)i(h)son(g) (::)",
            1200.0,
        ),
        // High notes on the second line make the staves farther apart than nominal.
        (
            "annotation: Ant.;\nannotation: VIII G;\n%%\n(c4) Ky(g)ri(h)e(g) (z) e(mnm)le(nm)i(mn)son(m) la(g)la(h) (::)",
            500.0,
        ),
        // An accented capital.
        ("annotation: Ant.;\n%%\n(c4) É(g)ra(h)t(g) (::)", 500.0),
    ];
    for (src, width) in cases {
        let eng = parse(src).score.engrave(&ApproxMeasure, &style);
        let list = eng.layout(width, &LayoutOptions::default()).display();
        let (_, baseline, size) = initial_item(&list).unwrap();
        let span = list.lines.len().min(2);
        let first = &list.lines[0];
        let last = &list.lines[span - 1];
        let sp = list.staff_space;
        assert!((baseline - (last.staff + 3.0 * sp)).abs() < 0.01, "{src}");
        // Cap height 0.65 em: the capital's top is the first staff's top line.
        assert!((baseline - 0.65 * size - (first.staff - 3.0 * sp)).abs() < 0.01, "{src}");
        assert!(baseline <= list.height, "{src}");
        for a in list.items.iter().filter_map(|i| match i {
            Item::Text {
                role: TextRole::Annotation,
                baseline,
                ..
            } => Some(*baseline),
            _ => None,
        }) {
            assert!(a < baseline - 0.65 * size && a > 0.0, "{src}");
        }
    }
}

#[test]
fn initial_only_from_the_opening_syllable() {
    // Notes before the first text: no drop cap, since it would sit lines away.
    let src = format!("(c4) {}Ky(g)ri(h)e(g) (::)", "(g) ".repeat(70));
    let eng = parse(&src).score.engrave(&ApproxMeasure, &StyleOptions::default());
    assert!(initial_item(&eng.layout(500.0, &LayoutOptions::default()).display()).is_none());
    // A capital with a tail still fits inside the layout.
    let eng = parse("(c4) Q(g)").score.engrave(&ApproxMeasure, &StyleOptions::default());
    let list = eng.layout(500.0, &LayoutOptions::default()).display();
    let (_, baseline, size) = initial_item(&list).unwrap();
    assert!(baseline + 0.25 * size <= list.height + 0.01);
}
