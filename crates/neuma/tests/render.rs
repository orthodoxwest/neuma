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
fn a_long_score_breaks_its_end_as_a_short_one_does() {
    // After a written break the rest of a score breaks as it would alone, however many lines
    // come before: their demerits mustn't swamp the small differences that choose its breaks.
    let tail = "Ad(f) te(g) le(h)vá(g)vi(f) á(gh)ni(g)mam(f) me(e)am(f) De(g)us(h) me(g)us(f) \
                in(g) te(h) con(g)fí(f)do(g) non(h) e(g)ru(f)bé(g)scam(h) ne(g)que(f) ir(g)rí(h)de(g)ant(f) \
                me(g) in(h)i(g)mí(f)ci(g) me(h)i(g) (::)";
    let head = "a(g) (z) ".repeat(3000);
    for width in (150..400).step_by(25) {
        let alone = line_texts(&format!("(c4) {tail}"), width as f32);
        let long = line_texts(&format!("(c4) {head}{tail}"), width as f32);
        assert_eq!(long[long.len() - alone.len()..], alone[..], "{width}");
    }
}

#[test]
fn unclosed_nlba_still_fills_lines() {
    // From GregoBase: a `<nlba>` never closed forbids every later break. The lines still fill
    // the width instead of taking one syllable each.
    let src = format!("(c4) <nlba>{} (::)", vec!["la(g)"; 60].join(" "));
    let eng = parse(&src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let layout = eng.layout(300.0, &LayoutOptions::default());
    let lines = layout.display().lines.len();
    assert!((2..10).contains(&lines), "{lines} lines");
    assert!(layout.size().0 <= 300.01);
    // A syllable wider than the column inside the run gets an overfull line of its own, and
    // the syllables after it still share lines.
    let src = "(c4) <nlba>Ab(g) c(h) d(g) e(h) f(g) g(h) h(g) Supercalifragilistic(ghghghghghghghghghghghghgh) i(g) j(h) k(g) l(h) m(g) n(h) o(g) p(h) q(g)</nlba>(::)";
    let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let lines = eng.layout(200.0, &LayoutOptions::default()).display().lines.len();
    assert!((3..8).contains(&lines), "{lines} lines");
}

#[test]
fn long_melismas_break_between_note_groups() {
    // From GregoBase: one syllable wider than a phone column, cut by `//` and bars.
    let src = "(c4) To(ixdh//gih//ivGF;ggf//gg//f/gh//jjg;hhg//hvGF;4hiHG//ixhi)ta(h) (::)";
    let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let layout = eng.layout(300.0, &LayoutOptions::default());
    assert!(layout.size().0 <= 300.01, "{:?}", layout.size());
    assert!(layout.display().lines.len() >= 2);
    // As in Gregorio, a syllable of fewer than ten notes isn't split, and a longer one keeps
    // four notes at either end.
    let lines = |src: &str| {
        let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
        eng.layout(40.0, &LayoutOptions::default()).display().lines.len()
    };
    assert_eq!(lines("(c4) A(ghg/hgh/ghg)"), 1);
    assert_eq!(lines("(c4) A(gh/hg/gh/hg/gh)"), 3);
    // Inside `<nlba>` the melisma stays whole, even past the width.
    let src = "(c4) <nlba>To(ixdh//gih//ivGF;ggf//gg//f/gh//jjg;hhg//hvGF;4hiHG//ixhi//ixdh//gih//ivGF)ta(h)</nlba> (::)";
    let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    assert!(eng.layout(300.0, &LayoutOptions::default()).size().0 > 300.0);
}

#[test]
fn one_syllable_nlba_keeps_its_melisma_whole() {
    // The region holds only this syllable, so no other syllable carries the no-break mark.
    let src = "(c4) <nlba>To(g/h/g/h/g/h/g/h/g/h/g/h/g/h/g/h/g/h)</nlba> (::)";
    // Its line: the clef, the whole melisma, and the final bar on a line of its own.
    let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    assert_eq!(eng.layout(100.0, &LayoutOptions::default()).line_count(), 2);
    // Without the region the same melisma breaks.
    let eng = parse("(c4) To(g/h/g/h/g/h/g/h/g/h/g/h/g/h/g/h/g/h) (::)")
        .score
        .engrave(&ApproxMeasure, &NO_INITIAL);
    assert_eq!(eng.layout(100.0, &LayoutOptions::default()).line_count(), 3);
    // And the region survives a round trip through GABC.
    let gabc = parse(src).score.to_gabc();
    assert!(gabc.contains("<nlba>To("), "{gabc}");
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
    // As in GregorioTeX, the capital stands on the first line's lyric baseline, left of the
    // staff, four times the lyrics' size.
    let line = &list.lines[0];
    assert!((initial.3 - line.baseline).abs() < 0.01);
    let size = |role: TextRole| {
        list.items.iter().find_map(|i| match i {
            Item::Text { role: r, size, .. } if *r == role => Some(*size),
            _ => None,
        })
    };
    assert!((size(TextRole::Initial).unwrap() - 4.0 * size(TextRole::Lyric).unwrap()).abs() < 0.01);
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
    // Over the capital, whose cap height is 0.65 em.
    let cap_top = initial.3 - 0.65 * size(TextRole::Initial).unwrap();
    assert!(ann[0].3 < ann[1].3 && ann[1].3 < cap_top);
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

/// The initial's x, baseline, font size and width (as `ApproxMeasure` measures it).
fn initial_item(list: &neuma::DisplayList) -> Option<(f32, f32, f32, f32)> {
    use neuma::TextMeasure;
    list.items.iter().find_map(|i| match i {
        Item::Text {
            role: TextRole::Initial,
            x,
            baseline,
            size,
            runs,
            ..
        } => Some((*x, *baseline, *size, ApproxMeasure.advance(&runs[0].text, runs[0].style) * size)),
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
        assert_initial_clear_of_staff(&list, src);
        let (_, baseline, size, _) = initial_item(&list).unwrap();
        let span = list.lines.len().min(2);
        let first = &list.lines[0];
        let last = &list.lines[span - 1];
        let sp = list.staff_space;
        assert!((baseline - (last.staff + 3.0 * sp)).abs() < 0.01, "{src}");
        // Cap height 0.65 em: the capital's top is the first staff's top line, since the
        // breaker widens the column rather than narrowing the capital.
        let top = baseline - 0.65 * size;
        assert!((top - (first.staff - 3.0 * sp)).abs() < 0.01, "{src}");
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
    let (_, baseline, size, _) = initial_item(&list).unwrap();
    assert!(baseline + 0.25 * size <= list.height + 0.01);
}

/// The capital ends left of where the indented staves start.
fn assert_initial_clear_of_staff(list: &neuma::DisplayList, src: &str) {
    let Some((x, _, _, width)) = initial_item(list) else { return };
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
    assert!(x >= -0.01 && x + width <= staff_left + 0.01, "{src}: {x} + {width} vs {staff_left}");
}

#[test]
fn tall_initials_never_cover_the_clef() {
    // High notes and breaks that land on the spanned lines only after the second pass.
    let src = "%%\n(c4) Wglo(ghgh) glori(ahvhv)menmi(,)(goz)Ky(ijz,)men(h.g_)e()(,) mi(,)";
    for lines in 2..=4 {
        let style = StyleOptions {
            initial: Initial::Lines(lines),
            ..StyleOptions::default()
        };
        let eng = parse(src).score.engrave(&ApproxMeasure, &style);
        for width in [200.0, 240.0, 320.0] {
            assert_initial_clear_of_staff(&eng.layout(width, &LayoutOptions::default()).display(), src);
        }
    }
}

#[test]
fn max_lines_keeps_the_first_lines_as_broken() {
    for (name, src) in corpus() {
        for spans in 1..=4 {
            let style = StyleOptions {
                initial: Initial::Lines(spans),
                ..StyleOptions::default()
            };
            let eng = parse(&src).score.engrave(&ApproxMeasure, &style);
            for width in [300.0, 500.0] {
                let full = eng.layout(width, &LayoutOptions::default());
                let full_list = full.display();
                let full_map = full.notes(&Weights::default());
                for n in 1..=3 {
                    let opts = LayoutOptions {
                        max_lines: n,
                        ..LayoutOptions::default()
                    };
                    let part = eng.layout(width, &opts);
                    let list = part.display();
                    let ctx = format!("{name} spans {spans} width {width} lines {n}");
                    let kept = n.min(full_list.lines.len());
                    assert_eq!(list.lines, full_list.lines[..kept], "{ctx}");
                    assert!(list.height <= full_list.height + 0.01, "{ctx}");
                    assert!(list.width <= full_list.width + 0.01, "{ctx}");
                    // Every item on a kept line, the initial included, is drawn exactly as in
                    // the full layout.
                    let bottom = list.lines.last().unwrap().bottom;
                    for item in &list.items {
                        assert!(full_list.items.contains(item), "{ctx}: {item:?}");
                        if let Item::Rect { y, .. } = item {
                            assert!(*y <= bottom, "{ctx}");
                        }
                    }
                    // The timeline is the full one cut after the kept notes and the pauses
                    // drawn with them.
                    let map = part.notes(&Weights::default());
                    assert_eq!(map.notes[..], full_map.notes[..map.notes.len()], "{ctx}");
                    assert_eq!(map.pauses[..], full_map.pauses[..map.pauses.len()], "{ctx}");
                    if kept < full_list.lines.len() {
                        let last = map.notes.last().unwrap();
                        let end = map.pauses.last().map_or(0.0, |p| p.start + p.weight);
                        let expected = (last.start + last.duration).max(end);
                        assert!((map.duration - expected).abs() < 1e-4, "{ctx}");
                        assert!(map.pauses.iter().all(|p| p.before_note <= last.id + 1), "{ctx}");
                    } else {
                        assert_eq!(map.duration, full_map.duration, "{ctx}");
                    }
                }
            }
        }
    }
}

#[test]
fn clefs_fit_their_lines() {
    // A do clef on the top line rises above the staff; its line makes room for it. A score of
    // only a clef draws its staff, as Gregorio does, rather than nothing.
    for src in ["(c4) a(f) b(g)", "(c4)", "name: a;\n%%\n(f3) ()"] {
        let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
        let list = eng.layout(300.0, &LayoutOptions::default()).display();
        assert_eq!(list.lines.len(), 1, "{src}");
        let line = list.lines[0];
        let mut clefs = 0;
        for item in &list.items {
            if let Item::Glyph {
                glyph,
                y,
                scale,
                role: neuma::Ink::Clef,
                ..
            } = item
            {
                let (_, top, _, bottom) = neuma::glyphs::GlyphId::from_id(*glyph).unwrap().ink();
                let unit = scale * neuma::glyphs::UNITS_PER_SPACE;
                assert!(y + top * unit >= line.top - 0.01, "{src}: {item:?} {line:?}");
                assert!(y + bottom * unit <= line.bottom + 0.01, "{src}: {item:?} {line:?}");
                clefs += 1;
            }
        }
        assert_eq!(clefs, 1, "{src}");
    }
}

#[test]
fn a_clef_change_right_after_the_opening_clef_shows_both() {
    let clefs = |src: &str| {
        let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
        let list = eng.layout(400.0, &LayoutOptions::default()).display();
        list.items
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    Item::Glyph {
                        role: neuma::Ink::Clef,
                        ..
                    }
                )
            })
            .count()
    };
    assert_eq!(clefs("(c4) (c3)"), 2);
    assert_eq!(clefs("(c4) (c3) a(g)"), 2);
    assert_eq!(clefs("(c4)"), 1);
    assert_eq!(clefs("(c4) a(g)"), 1);
    assert_eq!(clefs("a(c4g)"), 1);
}

/// The lyric and hyphen texts of a layout, with their left edges and sizes, in drawing order.
fn texts(src: &str, width: f32, opts: &LayoutOptions) -> Vec<(String, f32, f32, TextRole)> {
    let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    eng.layout(width, opts)
        .display()
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text { runs, x, size, role, .. } => Some((runs.iter().map(|r| r.text.as_str()).collect(), *x, *size, *role)),
            _ => None,
        })
        .collect()
}

#[test]
fn hyphens_follow_the_text_and_only_where_syllables_part() {
    use neuma::TextMeasure;
    let opts = LayoutOptions::default();
    // Syllables whose texts touch need no hyphen, as in Gregorio's "Dómi-nus".
    let t = texts("(c4) Dóm(g)mim(g)num(g)", 2000.0, &opts);
    assert!(t.iter().all(|t| t.3 != TextRole::Hyphen), "{t:?}");
    // A melisma holds the next syllable away: the hyphen sits right after the first text.
    let t = texts("(c4) a(ghghghgh)b(g)", 2000.0, &opts);
    let hyphens: Vec<_> = t.iter().filter(|t| t.3 == TextRole::Hyphen).collect();
    assert_eq!(hyphens.len(), 1, "{t:?}");
    let a = t.iter().find(|t| t.0 == "a").unwrap();
    let a_right = a.1 + ApproxMeasure.advance("a", Default::default()) * a.2;
    assert!((hyphens[0].1 - a_right).abs() < 0.01, "{t:?}");
    let b = t.iter().find(|t| t.0 == "b").unwrap();
    let hyphen_right = hyphens[0].1 + ApproxMeasure.advance("-", Default::default()) * a.2;
    assert!(b.1 >= hyphen_right - 0.01, "{t:?}");
}

#[test]
fn touching_syllables_stay_together_on_a_justified_line() {
    // Justifying a line widens the gaps between words, not between syllables whose texts touch.
    let src = format!("(c4) {} (::)", ["Dóm(g)mim(g)num(g)"; 12].join(" "));
    let t = texts(&src, 500.0, &LayoutOptions::default());
    // Only the lines that end inside a word have a hyphen, at their end.
    let lines = parse(&src)
        .score
        .engrave(&ApproxMeasure, &NO_INITIAL)
        .layout(500.0, &LayoutOptions::default())
        .line_count();
    assert!(lines > 1);
    let hyphens: Vec<_> = t.iter().filter(|t| t.3 == TextRole::Hyphen).collect();
    assert!(hyphens.len() < lines, "{t:?}");
    assert!(hyphens.iter().all(|h| h.1 > 400.0), "{t:?}");
}

#[test]
fn justifying_a_one_word_line_never_parts_its_syllables_without_a_hyphen() {
    use neuma::TextMeasure;
    // One long word whose syllables touch: a narrow width breaks it over lines, and stretching
    // those lines must not pull syllables apart unless a hyphen goes between them.
    let src = format!("(c4) {}(::)", ["Dóm(g)mim(g)num(g)"; 12].concat());
    for width in [150.0, 200.0, 300.0] {
        let t = texts(&src, width, &LayoutOptions::default());
        let hyphens: Vec<f32> = t.iter().filter(|t| t.3 == TextRole::Hyphen).map(|h| h.1).collect();
        let lyrics: Vec<_> = t.iter().filter(|t| t.3 == TextRole::Lyric).collect();
        for pair in lyrics.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let r = a.1 + ApproxMeasure.advance(&a.0, Default::default()) * a.2;
            // A later line starts back at the left.
            if b.1 < a.1 {
                continue;
            }
            let gap = b.1 - r;
            if gap > 0.1 {
                let dash = ApproxMeasure.advance("-", Default::default()) * a.2;
                assert!(hyphens.iter().any(|h| (h - r).abs() < 0.01), "{width}: {a:?} {b:?} {t:?}");
                assert!(gap >= dash - 0.01, "{width}: {a:?} {b:?} {t:?}");
            }
        }
    }
}

/// A text and its left and right ends.
type Span = (String, f32, f32);

/// Each line's lyric and hyphen texts, left to right.
fn line_spans(src: &str, width: f32, style: &StyleOptions) -> Vec<Vec<Span>> {
    use neuma::TextMeasure;
    let list = parse(src)
        .score
        .engrave(&ApproxMeasure, style)
        .layout(width, &LayoutOptions::default())
        .display();
    let mut lines: Vec<(f32, Vec<Span>)> = Vec::new();
    for item in &list.items {
        if let Item::Text {
            runs,
            x,
            size,
            role,
            baseline,
            ..
        } = item
            && matches!(role, TextRole::Lyric | TextRole::Hyphen)
        {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            let right = x + ApproxMeasure.advance(&text, Default::default()) * size;
            match lines.iter_mut().find(|l| l.0 == *baseline) {
                Some(l) => l.1.push((text, *x, right)),
                None => lines.push((*baseline, vec![(text, *x, right)])),
            }
        }
    }
    lines
        .into_iter()
        .map(|(_, mut l)| {
            l.sort_by(|a, b| a.1.total_cmp(&b.1));
            l
        })
        .collect()
}

#[test]
fn a_word_after_one_ending_in_an_empty_syllable_keeps_its_space() {
    // The empty syllable continues the first word, but the next text starts a new one: a word
    // space apart, however squeezed the line, and with no hyphen between.
    for (src, a) in [
        ("(c4) quam(eg/fssded)(/) *() la(g) (::)", "quam"),
        ("(c4) o(jr1)(ir) u(i) la(g) (::)", "o"),
        ("(c4) Glo(i)(j) ri(j) la(g) (::)", "Glo"),
    ] {
        let src = format!("(c4) {}", [&src[5..]; 6].join(" "));
        for lyric_size in [1.0, 2.45, 8.0] {
            let style = StyleOptions {
                lyric_size,
                ..NO_INITIAL.clone()
            };
            for width in (100..700).step_by(9) {
                for line in line_spans(&src, width as f32, &style) {
                    for p in line.windows(2) {
                        assert!(p[1].1 >= p[0].2 - 0.01, "{width} {lyric_size}: {line:?}");
                        assert!(!(p[0].0 == a && p[1].0 == "-"), "{width} {lyric_size}: {line:?}");
                    }
                    // Nor at a line's end, when the next word goes on the next line.
                    if line.last().is_some_and(|t| t.0 == "-") {
                        assert_ne!(line[line.len() - 2].0, a, "{width} {lyric_size}: {line:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn a_text_past_the_last_syllable_stays_in_the_box() {
    use neuma::TextMeasure;
    // The last syllable is empty, so the text before it is the line's right end: squeezing the
    // line must narrow that, not only the gap before the empty syllable.
    let src = "(c4) O(hg) be(gh) joy(h)ful(h) in(hg~) God,(gi) all(hi) ye(h) lands:(h) *(:) sing(hg) \
               prai(gh)ses(h) un(h)to(h) the(h) ho(h)nour(h) of(h) his(h) Name,(h.1) (,) make(h) his(h) \
               praise(h) to(gf) be(gh) glo(g)ri(e)ous.(e) (::) All(fff) the(dfe) earth.(e/gh.1) ()";
    for lyric_size in [2.0, 2.45, 8.0] {
        let style = StyleOptions {
            lyric_size,
            ..NO_INITIAL.clone()
        };
        let eng = parse(src).score.engrave(&ApproxMeasure, &style);
        for width in (150..900).step_by(3) {
            let list = eng.layout(width as f32, &LayoutOptions::default()).display();
            for item in &list.items {
                if let Item::Text { runs, x, size, .. } = item {
                    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
                    let right = x + ApproxMeasure.advance(&text, Default::default()) * size;
                    assert!(right <= list.width + 0.01, "{lyric_size} {width}: {text} {right} {}", list.width);
                }
            }
        }
    }
}

#[test]
fn a_line_starting_without_text_keeps_its_first_text_in_the_box() {
    // A line that opens with an empty syllable sets its first text at the line's
    // start at the least; squeezing the gaps before it mustn't push it past.
    let src = format!("(c4) {} (::)", ["() * Quidquid(ghGF) est(h) in(g) di(h)ce(g)re(h)"; 12].join(" "));
    for lyric_size in [2.45, 4.0, 8.0] {
        let style = StyleOptions {
            lyric_size,
            ..NO_INITIAL.clone()
        };
        let eng = parse(&src).score.engrave(&ApproxMeasure, &style);
        for width in (150..900).step_by(3) {
            let list = eng.layout(width as f32, &LayoutOptions::default()).display();
            for item in &list.items {
                if let Item::Text { runs, x, .. } = item {
                    assert!(*x >= -0.01, "{lyric_size} {width}: {runs:?} {x}");
                }
            }
        }
    }
}

#[test]
fn a_hyphen_ends_a_line_inside_a_word() {
    use neuma::TextMeasure;
    let src = format!("(c4) {} (::)", ["la(g)ta(h)"; 30].join("-").replace("-", ""));
    let t = texts(&src, 300.0, &LayoutOptions::default());
    // Every hyphen sits right after a syllable's text, the one ending each line included.
    let rights: Vec<f32> = t
        .iter()
        .filter(|t| t.3 == TextRole::Lyric)
        .map(|l| l.1 + ApproxMeasure.advance(&l.0, Default::default()) * l.2)
        .collect();
    let hyphens: Vec<f32> = t.iter().filter(|t| t.3 == TextRole::Hyphen).map(|h| h.1).collect();
    assert!(hyphens.len() >= 3, "{t:?}");
    for h in hyphens {
        assert!(rights.iter().any(|r| (h - r).abs() < 0.01), "{h} {t:?}");
    }
}

#[test]
fn squeezed_small_lyrics_never_meet() {
    use neuma::TextMeasure;
    // Words whose texts are wider than their notes are set a word space apart; squeezing a
    // line takes at most part of that space, however small the lyrics.
    let style = StyleOptions {
        lyric_size: 0.5,
        ..NO_INITIAL.clone()
    };
    let src = format!("(c4) {} (::)", ["Mmmmmmmmmmmmmmmm(g)"; 24].join(" "));
    let eng = parse(&src).score.engrave(&ApproxMeasure, &style);
    for width in (100..600).step_by(7) {
        let list = eng.layout(width as f32, &LayoutOptions::default()).display();
        let lyrics: Vec<(f32, f32)> = list
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text {
                    runs,
                    x,
                    size,
                    role: TextRole::Lyric,
                    ..
                } => {
                    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
                    Some((*x, *x + ApproxMeasure.advance(&text, Default::default()) * size))
                }
                _ => None,
            })
            .collect();
        for pair in lyrics.windows(2) {
            if pair[1].0 > pair[0].0 {
                assert!(pair[1].0 >= pair[0].1 + 0.01, "{width}: {pair:?}");
            }
        }
    }
}

#[test]
fn a_preview_draws_its_staves_as_wide_as_the_whole_score() {
    // The second line holds a word too wide for the column, which widens the layout; the
    // one-line preview's staff is as wide as in the whole score.
    let src = "(c4) a(g) b(g) c(g) Supercalifragilisticexpialidocious(g) d(g)";
    let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let full = eng.layout(120.0, &LayoutOptions::default()).display();
    let preview = eng
        .layout(
            120.0,
            &LayoutOptions {
                max_lines: 1,
                ..LayoutOptions::default()
            },
        )
        .display();
    assert!(full.width > 120.0);
    assert_eq!(preview.width, full.width);
    for item in &preview.items {
        assert!(full.items.contains(item), "{item:?}");
    }
}

#[test]
fn words_keep_gregorios_space_between_them() {
    use neuma::TextMeasure;
    assert_eq!(StyleOptions::default().lyric_size, 2.45);
    // Short notes don't pull two words' texts closer than GregorioTeX's 0.17 cm (0.48 em).
    let t = texts("(c4) hính(g) là(g) lúc(g)", 2000.0, &LayoutOptions::default());
    let lyrics: Vec<_> = t.iter().filter(|t| t.3 == TextRole::Lyric).collect();
    for w in lyrics.windows(2) {
        let right = w[0].1 + ApproxMeasure.advance(&w[0].0, Default::default()) * w[0].2;
        assert!(w[1].1 - right >= 0.48 * w[0].2 - 0.01, "{t:?}");
    }
}

#[test]
fn notes_keep_gregorios_space_between_syllables_and_words() {
    // GregorioTeX's intersyllablespacenotes and interwordspacenotes: 0.24 cm and 0.29 cm on a
    // staff whose interline is 0.288 cm, so 1.67 and 2 staff spaces between noteheads.
    let gaps = |src: &str| {
        let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
        let opts = LayoutOptions::default();
        let map = eng.layout(2000.0, &opts).notes(&Weights::default());
        let n = &map.notes;
        (n[1].x - n[0].x - n[0].w) / opts.scale
    };
    assert!((gaps("(c4) i(g)i(g)") - 1.67).abs() < 0.01);
    assert!((gaps("(c4) i(g) i(g)") - 2.0).abs() < 0.01);
}

#[test]
fn a_syllable_with_its_own_hyphen_gets_no_other() {
    // As in Gregorio, "Giê-su" written with the hyphen keeps that one only, inside a line and
    // at its end.
    let t = texts("(c4) Giê-(ghghgh)su(g) Vua(g)", 2000.0, &LayoutOptions::default());
    assert!(t.iter().all(|t| t.3 != TextRole::Hyphen), "{t:?}");
    let src = format!("(c4) {} (::)", ["Ma-(g)đa-(h)le-(g)na(h)"; 12].join(" "));
    let t = texts(&src, 300.0, &LayoutOptions::default());
    assert!(t.iter().all(|t| t.3 != TextRole::Hyphen), "{t:?}");
}

#[test]
fn lyrics_sit_where_gregorio_sets_them() {
    let lines = |src: &str| {
        let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
        let opts = LayoutOptions::default();
        let list = eng.layout(100.0, &opts).display();
        let s = opts.scale;
        list.lines
            .iter()
            .map(|l| ((l.baseline - l.staff) / s, l.baseline / s))
            .collect::<Vec<_>>()
    };
    let src = "(c4) la(g) la(h) la(g) la(h) la(g) la(h) la(g) la(h) la(g) la(h) la(g) la(h)";
    let plain = lines(src);
    assert!(plain.len() > 2);
    // 3.3 staff spaces below the bottom line, on every line, and 13.43 from baseline to
    // baseline (GregorioTeX's spacelinestext and baselineskip).
    for (drop, _) in &plain {
        assert!((drop - 6.3).abs() < 0.01, "{plain:?}");
    }
    for w in plain.windows(2) {
        assert!((w[1].1 - w[0].1 - 13.43).abs() < 0.01, "{plain:?}");
    }
    // A note below `c` anywhere in the score lowers the lyrics on every line by a staff space
    // a step.
    let low = lines(&src.replacen("la(h)", "la(b)", 1));
    for (drop, _) in &low {
        assert!((drop - 7.3).abs() < 0.01, "{low:?}");
    }
    let lower = lines(&format!("{src} la(a)"));
    for (drop, _) in &lower {
        assert!((drop - 8.3).abs() < 0.01, "{lower:?}");
    }
}

#[test]
fn a_double_mora_on_a_clivis_dots_each_note() {
    // Gregorio reads `hg..` as a mora on each note: one dot after the clivis at each note's
    // height, and both notes held.
    let eng = parse("(c4) a(hg..)").score.engrave(&ApproxMeasure, &NO_INITIAL);
    let layout = eng.layout(400.0, &LayoutOptions::default());
    let dots: Vec<(f32, f32, Option<u32>)> = layout
        .display()
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Glyph {
                x,
                y,
                role: neuma::Ink::Mora,
                note,
                ..
            } => Some((*x, *y, *note)),
            _ => None,
        })
        .collect();
    assert_eq!(dots.len(), 2, "{dots:?}");
    assert!((dots[0].0 - dots[1].0).abs() < 0.01, "{dots:?}");
    assert!(dots[0].1 < dots[1].1, "{dots:?}");
    assert_eq!((dots[0].2, dots[1].2), (Some(0), Some(1)));
    let weights = Weights::default();
    let map = layout.notes(&weights);
    assert!(map.notes.iter().all(|n| n.weight == weights.mora), "{:?}", map.notes);
    // A double mora on a single note stays two dots side by side, on that note.
    let eng = parse("(c4) a(h..)").score.engrave(&ApproxMeasure, &NO_INITIAL);
    let list = eng.layout(400.0, &LayoutOptions::default()).display();
    let ys: Vec<f32> = list
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Glyph {
                y, role: neuma::Ink::Mora, ..
            } => Some(*y),
            _ => None,
        })
        .collect();
    assert_eq!(ys.len(), 2);
    assert_eq!(ys[0], ys[1]);
}

#[test]
fn an_end_of_line_custos_keeps_gregorios_gap() {
    // GregorioTeX's spacebeforeeolcustos: 0.23 cm, 1.6 staff spaces, from the last note.
    let src = format!("(c4) {} (::)", ["la(g)"; 40].join(" "));
    let eng = parse(&src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let opts = LayoutOptions::default();
    let layout = eng.layout(300.0, &opts);
    let list = layout.display();
    let map = layout.notes(&Weights::default());
    let first_line_end = map
        .notes
        .iter()
        .filter(|n| n.line == 0)
        .map(|n| n.x + n.w / 2.0)
        .fold(0.0, f32::max);
    let custos = list
        .items
        .iter()
        .find_map(|i| match i {
            Item::Glyph {
                x,
                role: neuma::Ink::Custos,
                ..
            } => Some(*x),
            _ => None,
        })
        .unwrap();
    let gap = (custos - first_line_end) / opts.scale;
    assert!(gap > 1.5 && gap < 1.8, "{gap}");
}

#[test]
fn versicle_signs_are_drawn_heavier() {
    // GregorioTeX's ℣ and ℟ are heavier than a text face's; the SVG strokes them.
    let eng = parse("(c4) <sp>V/</sp> Ve(g)ni(h) <sp>R/</sp> Do(g)")
        .score
        .engrave(&ApproxMeasure, &NO_INITIAL);
    let svg = eng.layout(400.0, &LayoutOptions::default()).svg(&SvgOptions::default());
    assert!(svg.contains(r#"class="neuma-rubric neuma-sign">℣"#), "{svg}");
    assert!(svg.contains(r#"class="neuma-rubric neuma-sign">℟"#), "{svg}");
    assert!(svg.contains(".neuma .neuma-sign{stroke:currentColor;stroke-width:.04em}"));
}

#[test]
fn a_first_syllable_taken_by_the_initial_leaves_a_hyphen() {
    // GregorioTeX's "E -O-dem": the initial took the whole first syllable of the word.
    let roles = |src: &str| -> Vec<(TextRole, String, f32)> {
        let eng = parse(src).score.engrave(&ApproxMeasure, &StyleOptions::default());
        eng.layout(600.0, &LayoutOptions::default())
            .display()
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text { role, runs, x, .. } => Some((*role, runs.iter().map(|r| r.text.as_str()).collect(), *x)),
                _ => None,
            })
            .collect()
    };
    let t = roles("(c4) E(f)o(g)dem(h) ve(g)ro(h)");
    let lead = t.iter().find(|t| t.0 == TextRole::Hyphen).expect("a hyphen");
    let o = t.iter().find(|t| t.1 == "o").unwrap();
    assert!(lead.2 < o.2, "{t:?}");
    // Not when the initial's syllable is a word of its own, or keeps some text.
    assert!(roles("(c4) A(f) ve(g)").iter().all(|t| t.0 != TextRole::Hyphen));
    let t = roles("(c4) Ky(f)ri(g)e(h)");
    let y = t.iter().find(|t| t.1 == "y").unwrap();
    assert!(t.iter().all(|h| h.0 != TextRole::Hyphen || h.2 > y.2), "{t:?}");
}

#[test]
fn spaces_inside_notes_are_gregorios() {
    // `/`, `//` and a space between note groups: GregorioTeX's interelementspace, largerspace
    // and glyphspace, 0.48, 0.76 and 1.52 staff spaces.
    let gap = |notes: &str| {
        let eng = parse(&format!("(c4) a({notes})")).score.engrave(&ApproxMeasure, &NO_INITIAL);
        let opts = LayoutOptions::default();
        let map = eng.layout(2000.0, &opts).notes(&Weights::default());
        let n = &map.notes;
        (n[1].x - n[1].w / 2.0 - (n[0].x + n[0].w / 2.0)) / opts.scale
    };
    assert!((gap("f/f") - 0.48).abs() < 0.01, "{}", gap("f/f"));
    assert!((gap("f//f") - 0.76).abs() < 0.01, "{}", gap("f//f"));
    assert!((gap("f f") - 1.52).abs() < 0.01, "{}", gap("f f"));
}

#[test]
fn a_bar_keeps_gregorios_space_either_side() {
    let eng = parse("(c4) a(g) (;) b(g)").score.engrave(&ApproxMeasure, &NO_INITIAL);
    let opts = LayoutOptions::default();
    let layout = eng.layout(2000.0, &opts);
    let map = layout.notes(&Weights::default());
    let bar = layout
        .display()
        .items
        .iter()
        .find_map(|i| match i {
            Item::Rect {
                x,
                w,
                role: neuma::Ink::Bar,
                ..
            } => Some((*x, *w)),
            _ => None,
        })
        .unwrap();
    let (a, b) = (&map.notes[0], &map.notes[1]);
    let before = (bar.0 - (a.x + a.w / 2.0)) / opts.scale;
    let after = (b.x - b.w / 2.0 - (bar.0 + bar.1)) / opts.scale;
    assert!((before - 1.6).abs() < 0.01 && (after - 1.6).abs() < 0.01, "{before} {after}");
}

#[test]
fn a_line_a_little_too_wide_shrinks_its_word_gaps() {
    use neuma::TextMeasure;
    // GregorioTeX's word spaces may shrink by 0.05 cm (0.35 staff spaces) to fit a line.
    let src = format!("(c4) {}", ["mum(g)"; 10].join(" "));
    let eng = parse(&src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let opts = LayoutOptions::default();
    // The narrowest column that holds the score on one line.
    let mut w = 2000.0;
    while eng.layout(w - 1.0, &opts).line_count() == 1 {
        w -= 1.0;
    }
    let t = texts(&src, w, &opts);
    let gaps: Vec<f32> = t
        .windows(2)
        .map(|p| (p[1].1 - p[0].1 - ApproxMeasure.advance(&p[0].0, Default::default()) * p[0].2) / opts.scale)
        .collect();
    let space = 0.48 * StyleOptions::default().lyric_size;
    // Every word gap gave up the same part of its shrink, and none more than 0.35.
    assert!(gaps.iter().all(|g| *g < space - 0.05 && *g >= space - 0.35 - 0.01), "{gaps:?}");
    assert!(gaps.iter().all(|g| (g - gaps[0]).abs() < 0.01), "{gaps:?}");
}

#[test]
fn a_score_takes_as_few_lines_as_gregorio_would() {
    use neuma::TextMeasure;
    // GregorioTeX sets \looseness=-1: rather than give a word a line of its own, it uses all
    // the shrink its word gaps have. At the narrowest column that holds ten words on one line,
    // each gap has given up nearly all of its 0.35 staff spaces.
    let src = format!("(c4) {}", ["mum(g)"; 10].join(" "));
    let eng = parse(&src).score.engrave(&ApproxMeasure, &NO_INITIAL);
    let opts = LayoutOptions::default();
    let mut w = 2000.0;
    while eng.layout(w - 1.0, &opts).line_count() == 1 {
        w -= 1.0;
    }
    let t = texts(&src, w, &opts);
    let space = 0.48 * StyleOptions::default().lyric_size;
    for p in t.windows(2) {
        let gap = (p[1].1 - p[0].1 - ApproxMeasure.advance(&p[0].0, Default::default()) * p[0].2) / opts.scale;
        assert!(gap < space - 0.3, "{gap}");
    }
}

#[test]
fn source_map_links_every_note_both_ways() {
    for (name, src) in corpus() {
        let eng = parse(&src).score.engrave(&ApproxMeasure, &StyleOptions::default());
        let layout = eng.layout(500.0, &LayoutOptions::default());
        let map = layout.source_map();
        let notes = layout.notes(&Weights::SOLESMES);
        assert_eq!(map.notes.len(), notes.notes.len(), "{name}");
        for (e, n) in map.notes.iter().zip(&notes.notes) {
            assert_eq!((e.index, &e.span), (n.id, &n.span), "{name}");
            // Score to source: the notehead's center is a note.
            let hit = map.source_at(n.x, n.y).unwrap();
            assert_eq!(hit.kind, neuma::ElementKind::Note, "{name}: note {}", n.id);
            // Source to score: a caret on the note highlights it and its syllable.
            let at = map.at(e.span.start);
            assert!(
                at.iter().any(|a| a.kind == neuma::ElementKind::Note && a.index == n.id),
                "{name}: note {}",
                n.id
            );
            assert!(
                at.iter()
                    .any(|a| a.kind == neuma::ElementKind::Syllable && a.span.start <= e.span.start && e.span.end <= a.span.end),
                "{name}"
            );
        }
        for b in &map.bars {
            assert!(src.get(b.span.clone()).is_some_and(|t| !t.is_empty()), "{name}: bar {}", b.index);
        }
    }
}

#[test]
fn svg_parts_draw_what_the_svg_draws() {
    for (name, src) in corpus() {
        let eng = parse(&src).score.engrave(&ApproxMeasure, &StyleOptions::default());
        let layout = eng.layout(500.0, &LayoutOptions::default());
        let whole = layout.svg(&SvgOptions::default());
        let parts = layout.svg_parts(&SvgOptions::default());
        let joined = parts.to_svg();
        assert_eq!(parts.lines.len(), layout.line_count(), "{name}");
        for tag in ["<use ", "<rect ", "<text ", "<path ", "<style>", "data-note=\""] {
            assert_eq!(joined.matches(tag).count(), whole.matches(tag).count(), "{name}: {tag}");
        }
        assert!(joined.starts_with(&parts.head) && joined.ends_with("</svg>"));
    }
}

#[test]
fn svg_lines_keep_their_strings_when_lines_above_change() {
    let opts = SvgOptions {
        ids: false,
        ..SvgOptions::default()
    };
    let lines = |src: &str| {
        let eng = parse(src).score.engrave(&ApproxMeasure, &NO_INITIAL);
        let parts = eng.layout(600.0, &LayoutOptions::default()).svg_parts(&opts);
        assert!(!parts.to_svg().contains("data-note"));
        parts.lines
    };
    let a = lines("(c4) Ky(g)ri(h)e(g) (z) e(h)le(g)i(h) (z) son(g) (::)");
    // A higher note on the first line makes it taller and adds a note before the others.
    let b = lines("(c4) Ky(gm)ri(h)e(g) (z) e(h)le(g)i(h) (z) son(g) (::)");
    assert_eq!((a.len(), b.len()), (3, 3));
    assert_ne!(a[0].svg, b[0].svg);
    assert!(b[1].top > a[1].top);
    assert_eq!(a[1].svg, b[1].svg);
    assert_eq!(a[2].svg, b[2].svg);
}
