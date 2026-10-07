//! The GABC writer: writing a parsed score and parsing it again reaches a fixed point after
//! one round, and the rewritten text keeps every note.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neuma::score::{Bar, BarKind, Figure, Space};

fn notes(score: &neuma::Score) -> usize {
    score
        .syllables
        .iter()
        .flat_map(|s| &s.notation)
        .filter(|f| matches!(f, Figure::Note(_)))
        .count()
}

/// Whether the notes of `gabc` spell `<nlba>` or `</nlba>`. The writer never puts the tag in
/// notes, so this is an augmented liquescent (`<`) before notes `n`, `l`, `b`, `a`: GABC can't
/// write that without it reading back as the tag.
fn spells_nlba_in_notes(gabc: &str) -> bool {
    gabc.split('(').skip(1).any(|g| {
        g.split(')')
            .next()
            .is_some_and(|notes| notes.contains("<nlba>") || notes.contains("</nlba>"))
    })
}

/// Whether the score has two figures in a row that GABC writes as one: `/` before `/`, `//`,
/// `/0` or `/!`; `!` before a space, which reads back as `! `; or `:` before `:`, `:?` or
/// `::`. Valid GABC never has them.
fn ambiguous_neighbours(score: &neuma::Score) -> bool {
    let colon = |f: &Figure| {
        matches!(
            f,
            Figure::Bar(Bar {
                kind: BarKind::Maior | BarKind::DottedMaior | BarKind::Finalis,
                ..
            })
        )
    };
    score.syllables.iter().any(|s| {
        s.notation.windows(2).any(|w| {
            matches!(
                (&w[0], &w[1]),
                (
                    Figure::Space(Space::Small),
                    Figure::Space(Space::Small | Space::Medium | Space::Half | Space::Tiny)
                ) | (Figure::Space(Space::Zero), Figure::Space(Space::Large | Space::LargeNoBreak))
            ) || (matches!(
                &w[0],
                Figure::Bar(Bar {
                    kind: BarKind::Maior,
                    high: false,
                    ..
                })
            ) && colon(&w[1]))
        })
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else { return };
    let first = neuma::parse(src).score;
    let once = first.to_gabc();
    if spells_nlba_in_notes(&once) || ambiguous_neighbours(&first) {
        return;
    }
    let second = neuma::parse(&once).score;
    let twice = second.to_gabc();
    assert_eq!(once, twice, "the writer isn't stable for {src:?}");
    assert_eq!(notes(&first), notes(&second), "notes lost writing {src:?} as {once:?}");
});
