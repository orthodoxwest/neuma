//! Diagnostics for editors: every code is documented, every span lands in the source, and
//! every fix removes the problem it fixes.

use std::fs;
use std::path::Path;

use neuma::{ApproxMeasure, Diagnostic, StyleOptions, parse};

fn diagnostics(src: &str) -> Vec<Diagnostic> {
    let parsed = parse(src);
    let eng = parsed.score.engrave(&ApproxMeasure, &StyleOptions::default());
    parsed.diagnostics.into_iter().chain(eng.diagnostics).collect()
}

/// Every `"prefix::code"` string literal in a crate's sources.
fn codes_in(dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            codes_in(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = fs::read_to_string(&path).unwrap();
            for (i, _) in text.match_indices("::") {
                let start = text[..i].rfind('"').map_or(i, |q| q + 1);
                let end = text[i..].find('"').map_or(i, |q| i + q);
                let code = &text[start..end];
                let (prefix, name) = code.split_once("::").unwrap_or(("", ""));
                let known = ["gabc", "engrave", "text", "pointed", "apply", "point"].contains(&prefix);
                if known && !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
                    out.push(code.to_string());
                }
            }
        }
    }
}

#[test]
fn every_code_is_documented() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let doc = fs::read_to_string(root.join("docs/diagnostics.md")).unwrap();
    let mut codes = Vec::new();
    for krate in ["neuma", "neuma-tones"] {
        codes_in(&root.join("crates").join(krate).join("src"), &mut codes);
    }
    codes.sort();
    codes.dedup();
    assert!(codes.len() > 40, "{codes:?}");
    for code in &codes {
        assert!(doc.contains(&format!("| `{code}` |")), "{code} isn't in docs/diagnostics.md");
    }
    // And the tables list nothing the engine no longer emits.
    let retired = doc.split("## Retired codes").nth(1).unwrap_or("");
    for line in doc.lines().filter(|l| l.starts_with("| `")) {
        let code = line.split('`').nth(1).unwrap();
        assert!(
            codes.iter().any(|c| c == code) || retired.contains(code),
            "{code} is documented but never emitted"
        );
    }
}

/// Sources that each draw one fixable diagnostic.
const FIXABLE: &[(&str, &str)] = &[
    ("gabc::no-separator", "name: Kyrie;\nmode: 8;\n(c4) Ky(g)ri(h)e(g)"),
    ("gabc::no-separator", "name: Kyrie;"),
    ("gabc::unterminated-header", "name: Kyrie\nmode: 8;\n%%\n(c4) Ky(g)"),
    ("gabc::unterminated-header", "name: Kyrie % comment\n\n%%\n(c4) Ky(g)"),
    ("gabc::unclosed-notes", "(c4) Ky(g)ri(h\n"),
    ("gabc::hyphen-in-syllable", "(c4) Ky-(g)ri(h)e(g)"),
    ("gabc::hyphen-in-syllable", "(c4) Ky(g) -ri(h)e(g)"),
    ("gabc::no-clef", "Ky(g)ri(h)e(g)"),
    ("gabc::no-clef", "%%\n  (g) e(h)"),
    ("gabc::unclosed-tag", "(c4) <i>Ky(g)ri(h)e(g) (::)"),
    ("gabc::unclosed-tag", "(c4) Ky(g) <sp>ae(h) (::)"),
    ("gabc::unclosed-tag", "(c4) Ky(g) <v>\\emph{x}(h) (::)"),
    ("gabc::unclosed-tag", "(c4) <i>Ky[Lord(g) ri(h)"),
    ("gabc::unclosed-tag", "(c4) <i>Ky</i(g) ri(h)"),
    ("gabc::unclosed-tag", "(c4) <i>Ky<b(g) ri(h)"),
    ("gabc::unclosed-tag", "(c4) <sp>ae</sp(g) ri(h)"),
];

#[test]
fn fixes_remove_their_diagnostic() {
    for &(code, src) in FIXABLE {
        let before = diagnostics(src);
        let d = before
            .iter()
            .find(|d| d.code == code)
            .unwrap_or_else(|| panic!("{code} not reported for {src:?}: {before:?}"));
        let fix = d.fix.as_ref().unwrap_or_else(|| panic!("{code} has no fix for {src:?}"));
        assert!(!fix.title.is_empty());
        let fixed = fix.apply(src).unwrap();
        let after = diagnostics(&fixed);
        let count = |ds: &[Diagnostic]| ds.iter().filter(|d| d.code == code).count();
        assert_eq!(count(&after), count(&before) - 1, "{code}: {src:?} fixed to {fixed:?}: {after:?}");
        // The fix introduces no new problem.
        assert!(after.len() < before.len(), "{code}: {src:?} fixed to {fixed:?}: {after:?}");
    }
}

#[test]
fn spans_land_in_the_source() {
    let mut sources: Vec<String> = FIXABLE.iter().map(|(_, s)| s.to_string()).collect();
    sources.extend([
        "def-macro: \\foo;\noriscus-orientation: legacy;\nstaff-lines: 5;\nnabc-lines: 1;\nlanguage: Klingon;\n%%\n(c4) a(g)".to_string(),
        "(c4) a(gz) b(h~>{ij}) [x] c(g|vihi) d(gxg_0[ll:1]) e(;7) f(/[]) g(c9) h(oo) <foo>i(g) <v>\\x</v>(g)".to_string(),
    ]);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "gabc") {
            sources.push(fs::read_to_string(p).unwrap());
        }
    }
    for src in &sources {
        for d in diagnostics(src) {
            assert!(src.get(d.span.clone()).is_some(), "{d} is outside {src:?}");
            if d.code != "apply::empty" {
                assert!(!d.span.is_empty() || d.fix.is_some(), "{d} has an empty span in {src:?}");
            }
            if let Some(f) = &d.fix {
                assert!(f.apply(src).is_some(), "{d}: fix {f:?}");
            }
        }
    }
}

/// Applies fixes, the first or the last offered each time, until none is left; panics if that
/// comes back to a source seen before, or takes more than a few steps beyond one per fix
/// offered at the start.
fn fix_until_done(src: &str, last: bool) {
    let mut seen = std::collections::HashSet::new();
    let mut s = src.to_string();
    let offered = diagnostics(src).iter().filter(|d| d.fix.is_some()).count();
    for _ in 0..2 * offered + 8 {
        let ds = diagnostics(&s);
        let mut fixes = ds.iter().filter_map(|d| d.fix.as_ref().map(|f| (d, f)));
        let next = if last { fixes.next_back() } else { fixes.next() };
        let Some((d, fix)) = next else { return };
        assert!(seen.insert(s.clone()), "fixing {src:?} came back to {s:?}");
        s = fix.apply(&s).unwrap_or_else(|| panic!("{d}: fix {fix:?} doesn't apply to {s:?}"));
    }
    panic!("fixing {src:?} didn't end: got to {s:?}");
}

#[test]
fn fixes_never_loop() {
    let mut sources: Vec<String> = FIXABLE.iter().map(|(_, s)| s.to_string()).collect();
    sources.extend(
        [
            // A closer at the syllable's end would be taken into a translation, a `<` or a
            // verbatim tag that runs on to it.
            "(c4) <i>a[b(g)",
            "(c4) a(g) <c>[</c>Pax(h) vo(g)bis(h) (::)",
            "(c4) <b><i>a[x(g) b(h)",
            "(c4) <i>[x(g) b(h)",
            "(c4) <i>a<b(g)",
            "(c4) <i>a<v>x(g)",
            "(c4) <i>a<sp>x(g)",
            // A closer left without its `>`.
            "(c4) <i>a</i(g)",
            "(c4) <sp>a</sp(g)",
            "(c4) <v>\\x</v(g)",
            "(c4) <i>a</b(g)",
            // Headers and their separator.
            "name: x;\nmode: 8\n(c4) a(g)",
            "name: a (b);\nmode: 8;\n(c4) a(g)",
            "name: x; % a note\nmode: 8;\n(c4) a(g)",
            "name: x;\nmode: 8; (c4) a(g)",
            "name: x\n(c4) a(g)",
            "a(g) b(h)",
            "name: x;\nmode: 8;\n",
        ]
        .map(String::from),
    );
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "gabc") {
            sources.push(fs::read_to_string(p).unwrap());
        }
    }
    // `NEUMA_CORPUS=<dir>` checks every `.gabc` in a directory as well.
    if let Ok(dir) = std::env::var("NEUMA_CORPUS") {
        for e in fs::read_dir(dir).unwrap().flatten() {
            if e.path().extension().is_some_and(|x| x == "gabc") {
                sources.push(fs::read_to_string(e.path()).unwrap_or_default());
            }
        }
    }
    for src in &sources {
        fix_until_done(src, false);
        fix_until_done(src, true);
    }
}
