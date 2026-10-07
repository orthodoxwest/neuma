//! Golden snapshots: each reference score (`tests/golden`, `tests/corpus` and the Compline
//! example) is engraved with the built-in metrics, laid out at two widths, and its display
//! list written as text to `tests/golden/snapshots/<name>.txt`, so a change to the engraving
//! shows up in review as a diff.
//!
//! After an intended change, rewrite the snapshots and review the diff:
//!
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test -p neuma --test golden
//! git diff crates/neuma/tests/golden
//! ```

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use neuma::glyphs::GlyphId;
use neuma::score::TextStyle;
use neuma::{DisplayList, Font, Item, LayoutOptions, MetricsTable, NoteRef, StyleOptions, parse};

/// A narrow phone and a wide page.
const WIDTHS: [f32; 2] = [360.0, 720.0];

/// Where the reference scores live.
const SOURCES: [&str; 3] = ["tests/golden", "tests/corpus", "../../examples/compline"];

fn snapshots() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/snapshots")
}

/// A number to two places, without a negative zero.
fn num(v: f32) -> String {
    let s = format!("{v:.2}");
    if s == "-0.00" { "0.00".into() } else { s }
}

fn notes(note: Option<NoteRef>, through: Option<NoteRef>) -> String {
    match (note, through) {
        (Some(n), Some(t)) => format!(" n{n}-{t}"),
        (Some(n), None) => format!(" n{n}"),
        _ => String::new(),
    }
}

fn style(s: &TextStyle) -> String {
    let flags = [
        (s.italic, 'i'),
        (s.bold, 'b'),
        (s.small_caps, 'c'),
        (s.underline, 'u'),
        (s.rubric, 'r'),
    ];
    flags.iter().filter(|(on, _)| *on).map(|(_, c)| *c).collect()
}

/// One line per staff line box and per item.
fn dump(out: &mut String, list: &DisplayList) {
    let _ = writeln!(
        out,
        "size {} x {}, staff space {}",
        num(list.width),
        num(list.height),
        num(list.staff_space)
    );
    for l in &list.lines {
        let _ = writeln!(
            out,
            "line top {} bottom {} staff {} baseline {}",
            num(l.top),
            num(l.bottom),
            num(l.staff),
            num(l.baseline)
        );
    }
    for item in &list.items {
        match item {
            Item::Glyph {
                glyph,
                x,
                y,
                role,
                note,
                through,
                ..
            } => {
                let name = GlyphId::from_id(*glyph).map_or("?", GlyphId::name);
                let _ = writeln!(out, "{} {name} {} {}{}", role.class(), num(*x), num(*y), notes(*note, *through));
            }
            Item::Rect {
                x,
                y,
                w,
                h,
                role,
                note,
                through,
            } => {
                let _ = writeln!(
                    out,
                    "{} rect {} {} {} {}{}",
                    role.class(),
                    num(*x),
                    num(*y),
                    num(*w),
                    num(*h),
                    notes(*note, *through)
                );
            }
            Item::Text {
                x,
                baseline,
                size,
                runs,
                role,
                syllable,
            } => {
                let _ = write!(out, "{role:?} {} {} {}", num(*x), num(*baseline), num(*size));
                if let Some(s) = syllable {
                    let _ = write!(out, " s{s}");
                }
                for r in runs {
                    let _ = write!(out, " {:?}", r.text);
                    let flags = style(&r.style);
                    if !flags.is_empty() {
                        let _ = write!(out, ":{flags}");
                    }
                }
                out.push('\n');
            }
        }
    }
}

fn snapshot(src: &str, metrics: &MetricsTable) -> String {
    let parsed = parse(src);
    let engraving = parsed.score.engrave(metrics, &StyleOptions::default());
    let mut out = String::new();
    for d in parsed.diagnostics.iter().chain(&engraving.diagnostics) {
        let _ = writeln!(out, "{d}");
    }
    for width in WIDTHS {
        let _ = writeln!(out, "\n# width {width}");
        dump(&mut out, &engraving.layout(width, &LayoutOptions::default()).display());
    }
    out
}

/// The first differing line, with a little context, for the failure message.
fn first_difference(expected: &str, actual: &str) -> String {
    let (e, a): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let i = e.iter().zip(&a).position(|(x, y)| x != y).unwrap_or(e.len().min(a.len()));
    let from = i.saturating_sub(2);
    let show = |lines: &[&str]| lines[from.min(lines.len())..(i + 3).min(lines.len())].join("\n    ");
    format!(
        "first difference at line {}:\n  expected:\n    {}\n  actual:\n    {}",
        i + 1,
        show(&e),
        show(&a)
    )
}

#[test]
fn golden_snapshots() {
    let metrics = MetricsTable::from_bytes(Font::Google.table_bytes()).unwrap();
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some_and(|v| !v.is_empty() && v != "0");
    let mut sources: Vec<PathBuf> = SOURCES
        .iter()
        .flat_map(|d| fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(d)).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "gabc"))
        .collect();
    sources.sort_by_key(|p| p.file_name().map(ToOwned::to_owned));
    let mut names: Vec<_> = sources.iter().map(|p| p.file_stem().unwrap()).collect();
    names.dedup();
    assert_eq!(names.len(), sources.len(), "two reference scores share a name");
    let mut failures = Vec::new();
    for path in &sources {
        let src = fs::read_to_string(path).unwrap();
        let actual = snapshot(&src, &metrics);
        let snap = snapshots().join(path.file_stem().unwrap()).with_extension("txt");
        let expected = fs::read_to_string(&snap).ok();
        if expected.as_deref() == Some(actual.as_str()) {
            continue;
        }
        if update {
            fs::write(&snap, &actual).unwrap();
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        failures.push(match expected {
            Some(e) => format!("{name}: {}", first_difference(&e, &actual)),
            None => format!("{name}: no snapshot"),
        });
    }
    // Snapshots whose score is gone.
    for entry in fs::read_dir(snapshots()).unwrap() {
        let path = entry.unwrap().path();
        if !names.contains(&path.file_stem().unwrap()) {
            if update {
                fs::remove_file(&path).unwrap();
            } else {
                failures.push(format!("{}: snapshot without a score", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} snapshot(s) differ; if the change is intended, run\n  UPDATE_SNAPSHOTS=1 cargo test -p neuma --test golden\nand review the diff.\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(sources.len() >= 20, "only {} reference scores", sources.len());
}
