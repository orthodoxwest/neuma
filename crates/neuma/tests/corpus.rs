//! Real scores: they parse without errors and survive a write/parse round trip.

use std::fs;
use std::path::Path;

use neuma::{Severity, parse};

#[test]
fn corpus_round_trips() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut count = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "gabc") {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap();
        let first = parse(&src);
        let errors: Vec<_> = first.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{}: {errors:?}", path.display());
        assert!(first.score.syllables.len() > 5, "{}", path.display());
        let once = first.score.to_gabc();
        let twice = parse(&once).score.to_gabc();
        assert_eq!(once, twice, "{}", path.display());
        count += 1;
    }
    assert!(count >= 3);
}
