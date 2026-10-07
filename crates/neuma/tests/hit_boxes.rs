//! Note hit boxes: a porrectus swash's ends find themselves, except where a neighbour's
//! notehead is.

use neuma::{ApproxMeasure, Initial, Layout, StyleOptions, Timeline, TimelineNote, parse};

fn note_map(src: &str) -> (Layout, Timeline) {
    let style = StyleOptions::default().with_initial(Initial::None);
    let layout = parse(src).score.engrave(&ApproxMeasure, &style).layout(2000.0);
    let timeline = layout.timeline();
    (layout, timeline)
}

/// Whether (`x`, `y`) is inside the note's notehead box.
fn inside(n: &TimelineNote, x: f32, y: f32) -> bool {
    (x - n.cx).abs() < n.w / 2.0 && (y - n.cy).abs() < n.h / 2.0
}

#[test]
fn porrectus_ends_hit_test_as_themselves_except_where_a_neighbour_is() {
    // Porrectus, porrectus flexus and torculus resupinus at each swash width, with the
    // note before reached by a stem, and liquescent ends; with where the swash starts.
    for (neume, first) in [
        ("hgh", 0),
        ("hfh", 0),
        ("heh", 0),
        ("hdh", 0),
        ("jij", 0),
        ("hgi", 0),
        ("hghg", 0),
        ("hfhg", 0),
        ("hgh~", 0),
        ("hghg~", 0),
        ("ihih", 0),
        ("ghgh", 1),
        ("fhgh", 1),
        ("ghgh~", 1),
        ("fgfg", 1),
    ] {
        let (layout, map) = note_map(&format!("(c4) a({neume}) b(g)"));
        let ns = &map.notes;
        for end in [first, first + 1] {
            // Sample the swash end's whole notehead box: a point inside a neighbour's box
            // finds that neighbour, and any other point finds the swash end.
            let e = &ns[end];
            const N: usize = 24;
            for i in 0..N {
                for j in 0..N {
                    let x = e.cx - e.w / 2.0 + e.w * (i as f32 + 0.5) / N as f32;
                    let y = e.cy - e.h / 2.0 + e.h * (j as f32 + 0.5) / N as f32;
                    let found = layout.note_at(x, y);
                    let neighbours: Vec<u32> = ns.iter().filter(|n| n.id != e.id && inside(n, x, y)).map(|n| n.id).collect();
                    if neighbours.is_empty() {
                        assert_eq!(found, Some(e.id), "{neume}: ({x}, {y}) in note {end}'s box");
                    } else {
                        assert!(
                            found.is_some_and(|f| neighbours.contains(&f)),
                            "{neume}: ({x}, {y}) is in {neighbours:?} but finds {found:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn porrectus_hit_testing_finds_each_note() {
    let (layout, map) = note_map("(c4) a(hgh)");
    let ns = &map.notes;
    assert_eq!(ns.len(), 3);
    for n in ns {
        assert_eq!(layout.note_at(n.cx, n.cy), Some(n.id));
    }
    // The swash's end keeps its notehead's center: the swash's right end, on its line.
    let (start, end) = (&ns[0], &ns[1]);
    assert_eq!(end.cy - start.cy, 6.0);
    assert!((end.cx - start.cx - (neuma::glyphs::GlyphId::Porrectus1.width() - 1.0) * 6.0).abs() < 1e-3);
    // Off the swash's end on the side away from the stacked note: still the swash's end.
    assert_eq!(layout.note_at(end.cx + 2.5, end.cy + 2.3), Some(end.id));
    // Just above the swash's end, inside the stacked note's box: the stacked note.
    let (end, top) = (&ns[1], &ns[2]);
    let y = top.cy + top.h / 2.0 - 0.05;
    assert!(y < end.cy);
    assert_eq!(layout.note_at(top.cx, y), Some(top.id));
}
