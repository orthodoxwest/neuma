//! Diagnostics for editors: every code is documented, every span lands in the source, and
//! every fix removes the problem it fixes.

use std::fs;
use std::path::Path;

use neuma::score::Figure;
use neuma::{ApproxMeasure, Diagnostic, StyleOptions, parse};

fn diagnostics(src: &str) -> Vec<Diagnostic> {
    let parsed = parse(src);
    let eng = parsed.score.engrave(&ApproxMeasure, &StyleOptions::default());
    parsed.diagnostics.into_iter().chain(eng.diagnostics.iter().cloned()).collect()
}

/// The namespaces of diagnostic codes, one per stage that reports them.
const PREFIXES: &[&str] = &["gabc", "engrave", "text", "pointed", "apply", "point", "book"];

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
                let known = PREFIXES.contains(&prefix);
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
    // Every crate that reports a diagnostic: the engine, the psalm tones, booklets, and the
    // command line, which reports `point::unsure` for `neuma point`.
    for krate in ["neuma", "neuma-tones", "neuma-book", "neuma-cli"] {
        codes_in(&root.join("crates").join(krate).join("src"), &mut codes);
    }
    codes.sort();
    codes.dedup();
    assert!(codes.len() > 50, "{codes:?}");
    for prefix in PREFIXES {
        assert!(
            codes.iter().any(|c| c.starts_with(&format!("{prefix}::"))),
            "no {prefix}:: code found"
        );
    }
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
    ("gabc::unclosed-notes", "(c4) A(fg men(f) (::)"),
    ("gabc::unclosed-notes", "(c4) A(fg(f) b(g)"),
    ("gabc::unclosed-notes", "(c4) Ky(g\nri(h)e(g)"),
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
        for (d, f) in ds.iter().filter_map(|d| d.fix.as_ref().map(|f| (d, f))) {
            assert!(s.get(f.span.clone()).is_some(), "{d}: fix {f:?} is off a char boundary in {s:?}");
        }
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
            // Several openers of a verbatim tag left open: a closer for a later one would
            // close the first, taking the notes between into its text.
            "(c4) <sp>V/ a(g) b(h) <sp>R/ c(g)",
            "(c4) a<v>(g) b(h) c<v>(g) d(h)",
            "(c4) <alt>x a(g) b(h) <alt>y c(g)",
            "(c4) <sp>V/ a(g) b(h) <sp>R/</sp c(g)",
            "(c4) <sp>R/</sp d(g)",
            // The opener a closer would go to is one the lyric never sees: in a translation,
            // inside another verbatim tag, or with its own closer split by a comment.
            "(c4) a[x<sp>y](g) b(h) <sp>V/ c(g)",
            "(c4) a[<v>](g) b(h) c<v>(g)",
            "(c4) <v>\\x <sp>y(g) b(h) <sp>V/ c(g)",
            "(c4) <alt>x <sp>y(g) b(h) <sp>V/ c(g)",
            "(c4) a<sp>V/</sp% c\n>(g) b(h) <sp>R/ c(g)",
            "(c4) <v>\\greheightstar</% c\nv>(,) a(g) b(h) ú(l)ti(k)<v>que(kj) c(g) (::)",
            // Notes left open while the text goes on.
            "(c4) A(fg men(f) (::)",
            "(c4) A(fg men(f) b(g (h)",
            "(c4) A((f)",
            "(c4) A(fgmen(f)",
            "(c4) <v>(</v>A(fg men(f)",
            // A closer left without its `>`.
            "(c4) <i>a</i(g)",
            "(c4) <sp>a</sp(g)",
            "(c4) <v>\\x</v(g)",
            "(c4) <i>a</b(g)",
            // Text whose bytes aren't the source's: comments are dropped from it, and any
            // whitespace reads as one space.
            "(c4) <i>a% c\nb(g)",
            "(c4) a(g) x% c\n<i>b(h)",
            "(c4) a\u{a0}<i>bé(g)",
            "(c4) a\u{202f}b-(g) c(h)",
            "(c4) <i>a\u{3000}b[x(g)",
            // Headers and their separator.
            "name: x;\nmode: 8\n(c4) a(g)",
            "name: a (b);\nmode: 8;\n(c4) a(g)",
            "name: x; % a note\nmode: 8;\n(c4) a(g)",
            "name: x;\nmode: 8; (c4) a(g)",
            "name: x\n(c4) a(g)",
            "a(g) b(h)",
            "name: x;\nmode: 8;\n",
            "\u{feff}name: x;\na(g)",
            "\u{feff}name: x;\n(c4) a(g)",
            "name: x;\n% a note\nmode: 8;\n(c4) a(g)",
            "name: x;\nV: a(g) b(h);\n(c4) c(g)",
            "name: x;\ncenteringmode: 8\n-scheme: english;\nfont: x;\n(c4) a(g)",
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
        no_fix_loses_notes(src);
    }
}

fn note_count(src: &str) -> usize {
    let score = parse(src).score;
    score
        .syllables
        .iter()
        .flat_map(|s| &s.notation)
        .filter(|f| matches!(f, Figure::Note(_)))
        .count()
}

/// Each fix offered for `src`, applied alone, keeps every note: none turns notes into text.
/// (Except the header separator's, which takes header lines that read as notes out of them.)
fn no_fix_loses_notes(src: &str) {
    let notes = note_count(src);
    for d in diagnostics(src) {
        let Some(fix) = d.fix.as_ref().filter(|_| d.code != "gabc::no-separator") else {
            continue;
        };
        let fixed = fix.apply(src).unwrap();
        assert!(note_count(&fixed) >= notes, "{d}: fix {fix:?} loses notes: {src:?} to {fixed:?}");
    }
}

/// The diagnostics with `code` for `src`, as (the source they point at, the fixed source).
fn found(src: &str, code: &str) -> Vec<(String, Option<String>)> {
    diagnostics(src)
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| (src[d.span.clone()].to_string(), d.fix.and_then(|f| f.apply(src))))
        .collect()
}

#[test]
fn lyric_spans_count_the_source_not_the_text() {
    // A comment inside a syllable's text, and whitespace other than a space before it.
    assert_eq!(
        found("(c4) <i>a% c\nb(g)", "gabc::unclosed-tag"),
        [("<i>".into(), Some("(c4) <i>a% c\nb</i>(g)".into()))]
    );
    assert_eq!(
        found("(c4) a\u{a0}<i>bé(g)", "gabc::unclosed-tag"),
        [("<i>".into(), Some("(c4) a\u{a0}<i>bé</i>(g)".into()))]
    );
    assert_eq!(
        found("(c4) a\u{202f}b-(g) c(h)", "gabc::hyphen-in-syllable"),
        [("-".into(), Some("(c4) a\u{202f}b(g) c(h)".into()))]
    );
    assert_eq!(
        found("(c4) b-% c\n(g)", "gabc::hyphen-in-syllable"),
        [("-".into(), Some("(c4) b% c\n(g)".into()))]
    );
    assert_eq!(found("(c4) a\u{a0}[x(g)", "gabc::translation-ignored")[0].0, "[x");
}

#[test]
fn the_separator_goes_where_it_surely_belongs() {
    let fixed = |src: &str| found(src, "gabc::no-separator").into_iter().map(|(_, f)| f).collect::<Vec<_>>();
    assert_eq!(
        fixed("\u{feff}name: x;\n(c4) a(g)"),
        [Some("\u{feff}name: x;\n%%\n(c4) a(g)".into())]
    );
    // A byte-order mark doesn't hide the header from the clef's fix either.
    assert_eq!(found("\u{feff}name: x;\na(g)", "gabc::no-clef"), [("g".into(), None)]);
    assert_eq!(
        fixed("name: x;\n% a note\nmode: 8;\n(c4) a(g)"),
        [Some("name: x;\n% a note\nmode: 8;\n%%\n(c4) a(g)".into())]
    );
    // A line that may be notes, or header-looking lines past the run, leave it unsure.
    assert_eq!(fixed("name: x;\nV: a(g) b(h);\n(c4) c(g)"), [None]);
    assert_eq!(fixed("name: x;\ncenteringmode: 8\n-scheme: english;\nfont: x;\n(c4) a(g)"), [None]);
    assert_eq!(fixed("name: x;\nV: a(g) b(h);\nmode: 8;\n(c4) c(g)"), [None]);
    assert_eq!(
        fixed("name:Kyrie II. (Rex Magne);\nmode: 1;\n(c4) a(g)"),
        [Some("name:Kyrie II. (Rex Magne);\nmode: 1;\n%%\n(c4) a(g)".into())]
    );
    assert_eq!(
        fixed("name: a (b);\nmode: 8;\n(c4) a(g)"),
        [Some("name: a (b);\nmode: 8;\n%%\n(c4) a(g)".into())]
    );
}

#[test]
fn only_the_first_unclosed_verbatim_tag_is_fixed() {
    let fixes = |src: &str| {
        diagnostics(src)
            .into_iter()
            .filter(|d| d.code == "gabc::unclosed-tag")
            .map(|d| d.fix.and_then(|f| f.apply(src)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        fixes("(c4) <sp>V/ a(g) b(h) <sp>R/ c(g)"),
        [Some("(c4) <sp>V/ a</sp>(g) b(h) <sp>R/ c(g)".into()), None]
    );
    assert_eq!(
        fixes("(c4) a<v>(g) b(h) c<v>(g) d(h)"),
        [Some("(c4) a<v></v>(g) b(h) c<v>(g) d(h)".into()), None]
    );
    // A closer missing its `>` is completed where it stands, and isn't part of the name.
    assert_eq!(fixes("(c4) <sp>R/</sp d(g)"), [Some("(c4) <sp>R/</sp> d(g)".into())]);
    let unknown = diagnostics("(c4) <sp>Q/</sp d(g)")
        .into_iter()
        .find(|d| d.code == "gabc::unknown-special")
        .unwrap();
    assert!(unknown.message.contains("`<sp>Q/</sp>`"), "{}", unknown.message);
}
