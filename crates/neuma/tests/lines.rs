//! How lines are made up: which get a staff, where the initial goes, and what stays inside
//! the page.

use neuma::{Ink, Item, LyricFont, StyleOptions, TextRole, parse};

fn display(src: &str, width: f32) -> neuma::DisplayList {
    parse(src)
        .score
        .engrave(LyricFont::Google.metrics(), &StyleOptions::default())
        .layout(width)
        .display()
}

#[test]
fn text_after_the_last_note_draws_no_staff() {
    // A rubric after the final bar that wraps to lines of its own: they are text alone.
    let src =
        "(c4) Al(g)le(h)lú(g)ia.(f) (::) Repetitur() Alleluia() a() cantore() usque() ad() asteriscum() et() chorus() prosequitur() (z)";
    let list = display(src, 300.0);
    assert!(list.lines.len() >= 2, "{:?}", list.lines);
    let staff_lines = |top: f32, bottom: f32| {
        list.items
            .iter()
            .filter(|i| matches!(i, Item::Rect { role: Ink::Staff, y, .. } if *y >= top && *y <= bottom))
            .count()
    };
    let first = &list.lines[0];
    assert_eq!(staff_lines(first.top, first.bottom), 4);
    for line in &list.lines[1..] {
        assert_eq!(staff_lines(line.top, line.bottom), 0, "{line:?}");
        // Its text still sits below the line before.
        assert!(line.baseline > first.bottom, "{line:?}");
    }
}

#[test]
fn a_custos_high_over_the_staff_stays_on_the_page() {
    for src in ["(c4) a(g) b(h)(z) c(m) d(l)", "(c1) a(e) b(f)(z) c(m)"] {
        let list = display(src, 400.0);
        assert!(list.lines.len() == 2);
        let custos = list.items.iter().find(|i| matches!(i, Item::Glyph { role: Ink::Custos, .. }));
        let Some(Item::Glyph { y, .. }) = custos else {
            panic!("{src}: no custos")
        };
        let top = list.items.iter().filter_map(|i| match i {
            Item::Glyph { y, .. } | Item::Rect { y, .. } => Some(*y),
            _ => None,
        });
        // The custos's head stands on its pitch; its stem points away from the staff's top.
        assert!(*y > 0.0 && top.fold(f32::INFINITY, f32::min) >= 0.0, "{src}");
    }
}

#[test]
fn a_one_line_initial_stands_on_the_first_line() {
    // Lines that hold nothing but the initial's syllable would leave the capital by itself.
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/edge-cases.gabc")).unwrap();
    for width in (240..=900).step_by(20) {
        let list = display(&src, width as f32);
        let Some(Item::Text { baseline, .. }) = list.items.iter().find(|i| {
            matches!(
                i,
                Item::Text {
                    role: TextRole::Initial,
                    ..
                }
            )
        }) else {
            panic!("no initial")
        };
        let first = &list.lines[0];
        assert!(
            (baseline - first.baseline).abs() < 0.01,
            "{width}: initial at {baseline}, first line {first:?}"
        );
    }
}

/// The reference scores: the golden ones, the corpus and the examples.
fn reference_scores() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out: Vec<(String, String)> = ["tests/golden", "tests/corpus", "../../examples/compline"]
        .iter()
        .flat_map(|d| std::fs::read_dir(root.join(d)).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "gabc"))
        .map(|p| (p.display().to_string(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    out.sort();
    out
}

/// The lines after the first that start with a bar: one drawn before every note on its line,
/// with no syllable of the line before it in the source (a cue of text alone may lead it).
fn lines_led_by_a_bar(layout: &neuma::Layout) -> Vec<u32> {
    let map = layout.source_map();
    (1..map.lines.len() as u32)
        .filter(|&line| {
            let first_note = map
                .notes
                .iter()
                .filter(|e| e.line == line)
                .map(|e| e.x)
                .fold(f32::INFINITY, f32::min);
            map.bars
                .iter()
                .filter(|b| b.line == line)
                .min_by(|a, b| a.x.total_cmp(&b.x))
                .is_some_and(|bar| bar.x < first_note && !map.syllables.iter().any(|s| s.line == line && s.span.end <= bar.span.start))
        })
        .collect()
}

#[test]
fn lines_start_with_a_bar_only_when_nothing_else_will_do() {
    // A bar ends the line of the syllable before it, as in GregorioTeX and printed books,
    // unless every other way leaves a line far too loose: here, only at the narrowest widths,
    // where one syllable fills a line, or a line beside an initial. None of these scores
    // writes a break before a bar.
    let mut led = Vec::new();
    for (name, src) in reference_scores() {
        let eng = parse(&src).score.engrave(LyricFont::Google.metrics(), &StyleOptions::default());
        for width in (160..1400).step_by(20) {
            let layout = eng.layout(width as f32);
            for line in lines_led_by_a_bar(&layout) {
                let name = std::path::Path::new(&name).file_name().unwrap().to_string_lossy().into_owned();
                led.push((name, width, line));
            }
        }
    }
    let expected = [
        ("psalm-134.gabc", 180, 1),
        ("long-melisma.gabc", 220, 1),
        ("long-melisma.gabc", 240, 1),
    ];
    assert_eq!(led, expected.map(|(n, w, l)| (n.to_string(), w, l)));
}

#[test]
fn the_timeline_says_where_each_bar_is_drawn() {
    // Each bar's pause carries the bar's box from the source map; a mediant or flex carries
    // the bar it sits at, or none.
    for (name, src) in reference_scores() {
        let eng = parse(&src).score.engrave(LyricFont::Google.metrics(), &StyleOptions::default());
        for width in [300.0, 700.0] {
            let layout = eng.layout(width);
            let map = layout.source_map();
            let timeline = layout.timeline();
            let mut bars = map.bars.iter();
            for (i, p) in timeline.pauses.iter().enumerate() {
                let ctx = format!("{name} at {width}, pause {i}");
                match p.kind {
                    neuma::PauseKind::Bar(_) => {
                        let (bar, e) = (p.bar.expect(&ctx), bars.next().expect(&ctx));
                        assert_eq!((bar.index, bar.line), (e.index, e.line), "{ctx}");
                        assert_eq!((bar.x, bar.y, bar.w, bar.h, bar.cx), (e.x, e.y, e.w, e.h, e.cx), "{ctx}");
                    }
                    _ => {
                        let at = timeline.pauses.iter().filter(|q| q.before_note == p.before_note);
                        let bar = at.filter(|q| matches!(q.kind, neuma::PauseKind::Bar(_))).find_map(|q| q.bar);
                        assert_eq!(p.bar, bar, "{ctx}");
                    }
                }
            }
            assert!(bars.next().is_none(), "{name} at {width}");
        }
    }
}
