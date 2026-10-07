//! Note hit boxes: a porrectus swash's ends don't overlap the notes around them.

use neuma::{ApproxMeasure, Initial, LayoutOptions, MappedNote, NoteMap, StyleOptions, Weights, parse};

fn note_map(src: &str) -> NoteMap {
    let style = StyleOptions {
        initial: Initial::None,
        ..StyleOptions::default()
    };
    let eng = parse(src).score.engrave(&ApproxMeasure, &style);
    eng.layout(2000.0, &LayoutOptions::default()).notes(&Weights::SOLESMES)
}

/// Overlap of two boxes along x and y; both positive means they share area.
fn overlap(a: &MappedNote, b: &MappedNote) -> (f32, f32) {
    ((a.w + b.w) / 2.0 - (a.x - b.x).abs(), (a.h + b.h) / 2.0 - (a.y - b.y).abs())
}

#[test]
fn porrectus_ends_do_not_overlap_their_neighbours() {
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
        let ns = note_map(&format!("(c4) a({neume}) b(g)")).notes;
        for end in [first, first + 1] {
            for (j, other) in ns.iter().enumerate() {
                if j == end {
                    continue;
                }
                let (ox, oy) = overlap(&ns[end], other);
                assert!(ox <= 1e-3 || oy <= 1e-3, "{neume}: notes {end} and {j} overlap by {ox} x {oy}");
            }
        }
    }
}

#[test]
fn porrectus_hit_testing_finds_each_note() {
    let map = note_map("(c4) a(hgh)");
    let ns = &map.notes;
    assert_eq!(ns.len(), 3);
    for n in ns {
        assert_eq!(map.note_at(n.x, n.y), Some(n.id));
    }
    // The swash's end keeps its notehead's center: the swash's right end, on its line.
    let (start, end) = (&ns[0], &ns[1]);
    assert_eq!(end.y - start.y, 6.0);
    assert!((end.x - start.x - (neuma::glyphs::GlyphId::Porrectus1.width() - 1.0) * 6.0).abs() < 1e-3);
    // Just above the swash's end, inside the stacked note's box: the stacked note.
    let (end, top) = (&ns[1], &ns[2]);
    let y = top.y + top.h / 2.0 - 0.05;
    assert!(y < end.y);
    assert_eq!(map.note_at(top.x, y), Some(top.id));
}
