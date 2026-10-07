//! The GABC writer: writing a parsed score and parsing it again reaches a fixed point after
//! one round, and the rewritten text keeps every note.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neuma::score::Figure;

fn notes(score: &neuma::Score) -> usize {
    score
        .syllables
        .iter()
        .flat_map(|s| &s.notation)
        .filter(|f| matches!(f, Figure::Note(_)))
        .count()
}

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else { return };
    let first = neuma::parse(src).score;
    let once = first.to_gabc();
    let second = neuma::parse(&once).score;
    let twice = second.to_gabc();
    assert_eq!(once, twice, "the writer isn't stable for {src:?}");
    assert_eq!(notes(&first), notes(&second), "notes lost writing {src:?} as {once:?}");
});
