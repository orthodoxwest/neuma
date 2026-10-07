//! How lines are made up: which get a staff, where the initial goes, and what stays inside
//! the page.

use neuma::{Font, Ink, Item, LayoutOptions, StyleOptions, TextRole, parse};

fn display(src: &str, width: f32) -> neuma::DisplayList {
    parse(src)
        .score
        .engrave(Font::Google.table(), &StyleOptions::default())
        .layout(width, &LayoutOptions::default())
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
            Item::Text { .. } => None,
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
