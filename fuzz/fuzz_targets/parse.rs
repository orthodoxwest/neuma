//! The GABC parser and the catalogue summary: any text parses without panicking, and every
//! diagnostic points inside the source.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else { return };
    let parsed = neuma::parse(src);
    for d in &parsed.diagnostics {
        assert!(d.span.start <= d.span.end && d.span.end <= src.len(), "{d}");
    }
    let _ = neuma::summarize(src);
});
