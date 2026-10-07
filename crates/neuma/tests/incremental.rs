//! Incremental engraving and layout against fresh ones: random edits at random places, each
//! followed by a cached and a fresh render that must come out the same, byte for byte.
//!
//! The scores are the test corpus, the examples, and one long score made of them all with
//! some rarer notation. `NEUMA_CORPUS=<dir>` runs every `.gabc` in a directory as well (the
//! GregoBase corpus takes a few minutes in release); `NEUMA_EDITS=<n>` sets the edits per score.

use neuma::{ApproxMeasure, Chant, ChantOptions, Initial, LastLine, LayoutOptions, StyleOptions, SvgOptions, SvgParts, Utf16Index, parse};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A chant that measures as the fresh engravings do.
fn chant(style: &StyleOptions) -> Chant {
    let options = ChantOptions::default()
        .with_style(style.clone())
        .with_measure(Arc::new(ApproxMeasure));
    Chant::with_options("", options)
}

/// Parts as they draw, leaving out which lines were reused.
fn drawn(p: &SvgParts) -> (f32, f32, &str, &str, Vec<(f32, &str)>, &str) {
    let lines = p.lines.iter().map(|l| (l.top, &*l.svg)).collect();
    (p.width, p.height, &p.head, &p.defs, lines, &p.rest)
}

/// xorshift64*: a small, seeded generator, so a failure names the edit that caused it.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// What gets typed: letters, pitches, neume and bar signs, markup and whole syllables.
const TYPED: &[&str] = &[
    "a",
    "e",
    "i",
    "o",
    "u",
    "s",
    "t",
    "é",
    "œ",
    "ǽ",
    " ",
    "\n",
    "(",
    ")",
    "()",
    "g",
    "h",
    "j",
    "k",
    "f",
    "d",
    "c",
    "m",
    "G",
    "H",
    "/",
    "//",
    "!",
    "'",
    "_",
    ".",
    "..",
    "~",
    "<",
    ">",
    "v",
    "V",
    "o",
    "w",
    "q",
    "r",
    "x",
    "y",
    "#",
    "-",
    "@",
    "+",
    ",",
    ";",
    ":",
    "::",
    ";3",
    "`",
    "*",
    "†",
    "z",
    "Z",
    "z0",
    "Z-",
    "{",
    "}",
    "[",
    "]",
    "|",
    "<i>",
    "</i>",
    "<b>",
    "<sp>V/</sp>",
    "<nlba>",
    "</nlba>",
    "c3",
    "c4",
    "f3",
    "cb3",
    "(c3)",
    "(f3)",
    "(z)",
    "(::)",
    "(;)",
    "(,)",
    "[nocustos]",
    "[oh:h]",
    "gx",
    "hy",
    "i#",
    "a(g)",
    " al(gh)le(hj)",
    "(hg..)",
    "(fgh/ih) ",
    "(e.)",
    "(g_)",
    "(hihhg)",
    "%%\n",
    "name: x;\n",
    "language: en;\n",
];

/// One random edit of `src`: an insertion, a deletion or a replacement, at char boundaries.
fn edit(src: &str, rng: &mut Rng) -> String {
    let bounds: Vec<usize> = src.char_indices().map(|(i, _)| i).chain([src.len()]).collect();
    let at = bounds[rng.below(bounds.len())];
    let typed = TYPED[rng.below(TYPED.len())];
    let cut = |rng: &mut Rng| {
        let k = bounds.partition_point(|&b| b < at);
        bounds[(k + 1 + rng.below(8)).min(bounds.len() - 1)]
    };
    match rng.below(3) {
        0 => format!("{}{typed}{}", &src[..at], &src[at..]),
        1 => {
            let to = cut(rng);
            format!("{}{}", &src[..at], &src[to..])
        }
        _ => {
            let to = cut(rng);
            format!("{}{typed}{}", &src[..at], &src[to..])
        }
    }
}

fn gabc_in(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && !p.ends_with("target") {
            gabc_in(&p, out);
        } else if p.extension().is_some_and(|x| x == "gabc") {
            out.push(p);
        }
    }
}

/// The scores to edit: each in the corpus and examples, and all of them as one.
fn scores() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    gabc_in(&root.join("tests/corpus"), &mut paths);
    gabc_in(&root.join("../../examples"), &mut paths);
    paths.sort();
    let mut out: Vec<(String, String)> = paths
        .iter()
        .map(|p| (p.display().to_string(), fs::read_to_string(p).unwrap()))
        .collect();
    let mut long = String::from("name: all;\nmode: 2;\n%%\n");
    for (_, src) in &out {
        long.push_str(src.split_once("%%").map_or(src.as_str(), |(_, body)| body));
        long.push('\n');
    }
    // Notation the scores above lack: clef changes, flats, forced breaks with and without a
    // custos, a break of its own, an unbreakable stretch and a long melisma.
    long.push_str(
        "(c3) Ve(gh)ni(hg) (z) cre(gxg)á(hy/h)tor(g) (Z-) Spí(f+)ri(gh)tus,(g.) (:) (z0) \
         <nlba>men(ef)tes(g) tu(h)ó(ij)rum(h)</nlba> (;) ví(ihgfgh/ihghgf/ghgfeg)si(g)ta.(g) (f3) \
         Im(hh)ple(hi) [nocustos](z) su(i)pér(ji)na(hg) (,) grá(gh)ti(g)a(f) (::)\n",
    );
    out.push(("all".to_string(), long));
    out
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

const WIDTHS: [f32; 4] = [900.0, 600.0, 330.0, 140.0];

/// Edits `src` again and again, checking the cached engraving and layout against fresh ones
/// each time.
fn check(name: &str, src: &str, seed: u64, edits: usize) {
    let mut rng = Rng(seed | 1);
    let mut src = src.to_string();
    let initial = [Initial::Lines(1), Initial::None, Initial::Lines(2)][rng.below(3)];
    let style = StyleOptions::default().with_initial(initial);
    let mut chant = chant(&style);
    let mut shown: Option<SvgParts> = None;
    let mut _held = None;
    let mut width = WIDTHS[rng.below(WIDTHS.len())];
    for step in 0..edits {
        // Now and then the column changes too, as when a window is resized.
        if rng.below(6) == 0 {
            width = WIDTHS[rng.below(WIDTHS.len())];
        }
        let opts = LayoutOptions::default().with_last_line(if rng.below(5) == 0 { LastLine::Justified } else { LastLine::Ragged });
        let what = || format!("{name}, seed {seed}, edit {step}, width {width}:\n{src}");
        let parsed = parse(&src);
        let eng = parsed.score.engrave(&ApproxMeasure, &style);
        chant.update(&src);
        // Parsed again around the edit, as a fresh parse reads it.
        assert!(chant.score() == &parsed.score, "{}", what());
        let found: Vec<_> = parsed.diagnostics.iter().chain(&eng.diagnostics).cloned().collect();
        assert_eq!(chant.diagnostics(), &found[..], "{}", what());
        assert!(chant.engraving() == &*eng, "{}", what());
        let cached = chant.layout_with(width, &opts);
        let fresh = eng.layout_with(width, &opts);
        let svg = SvgOptions::default();
        assert_eq!(cached.svg_with(&svg), fresh.svg_with(&svg), "{}", what());
        assert_eq!(cached.svg_parts_with(&svg), fresh.svg_parts_with(&svg), "{}", what());
        // An editor's lines, mostly without ids; now and then with them.
        let parts = SvgOptions::default().with_ids(rng.below(8) == 0);
        let cached_parts = match &shown {
            Some(shown) => cached.svg_parts_reusing(shown, &parts),
            None => cached.svg_parts_with(&parts),
        };
        assert_eq!(drawn(&cached_parts), drawn(&fresh.svg_parts_with(&parts)), "{}", what());
        shown = Some(cached_parts);
        assert_eq!(cached.timeline(), fresh.timeline(), "{}", what());
        assert_eq!(cached.source_map(), fresh.source_map(), "{}", what());
        assert_eq!(cached.utf16(), Some(&Utf16Index::new(&src)), "{}", what());
        // A page still showing the layout shares its engraving, which the next edit then
        // reads rather than takes.
        _held = (rng.below(2) == 0).then_some(cached);
        src = edit(&src, &mut rng);
    }
}

#[test]
fn a_cached_engraving_and_layout_are_fresh_ones() {
    let edits = env_usize("NEUMA_EDITS", 40);
    for (i, (name, src)) in scores().iter().enumerate() {
        let rounds = if name == "all" { 8 } else { 2 };
        for r in 0..rounds {
            check(name, src, 0x9e37_79b9 + (i * 101 + r) as u64, edits);
        }
    }
}

/// Edits that random ones seldom make: each changes what engraving carries past the edit, or
/// what a syllable reads of its neighbours.
#[test]
fn edits_whose_effects_reach_past_them() {
    let tail = " ve(g)ni(h) cre(ih)á(g)tor(f) Spí(g)ri(h)tus,(g) (;) men(f)tes(g) tu(h)ó(g)rum(f) (::)";
    let cases = [
        // An oriscus turns towards the next note, past a bar.
        ("(c4) a(go) (,) b(h)", "(c4) a(go) (,) b(f)"),
        ("(c4) a(go) (,) b(h)", "(c4) a(go) (,) (z) b(f)"),
        // A break of its own, and `<nlba>`, change the segment before them.
        ("(c4) a(g) b(h) (z) c(g) d(h)", "(c4) a(g) b(j) (z) c(g) d(h)"),
        ("(c4) a(g) b(h) (Z) c(g) d(h)", "(c4) a(g) bb(h) (Z) c(g) d(h)"),
        (
            "(c4) a(g) b(h) <nlba>c(g) d(h)</nlba> e(g)",
            "(c4) a(g) b(hi) <nlba>c(g) d(h)</nlba> e(g)",
        ),
        ("(c4) a(g) b(h) <nlba>c(g) d(h)</nlba> e(g)", "(c4) a(g) b(h) c(g) d(h)</nlba> e(g)"),
        ("(c4) a(g) b(h) (z) c(g)", "(c4) a(g) b(h) c(g)"),
        ("(c4) a(g) (z) (z) c(g)", "(c4) a(g) (z) c(g)"),
        // A clef, a flat or a custos carried past the edit.
        ("(c4) a(g) b(h) (c3) c(g) d(h)", "(c4) a(g) b(hi) (c3) c(g) d(h)"),
        ("(c4) a(g) b(h) c(g) d(h)", "(c4) a(g) (c3) b(h) c(g) d(h)"),
        ("(c4) a(gx) b(g) c(g) d(h)", "(c4) a(g) b(g) c(g) d(h)"),
        ("(c4) a(ix) b(i) c(i) (,) d(i)", "(c4) a(ix) b(i) cc(i) (,) d(i)"),
        ("(c4) a(g) b(gx)c(g)d(g) e(h)", "(c4) a(g) b(g)c(g)d(g) e(h)"),
        ("(c4) a(g) b(g)c(g)d(g) e(h)", "(c4) a(g) b(gx)c(g)d(g) e(h)"),
        ("(c4) a(g) b(gx)c(g)d(g) e(h)", "(c4) a(g) b(gy)c(g)d(g) e(h)"),
        ("(z) (c4) a(g) b(h)", "(z) (c4) a(g) b(i)"),
        ("(c4) (z) a(g) b(h)", "(c4) (z) a(gh) b(h)"),
        ("(c4) a(g) b(z0) c(h)", "(c4) a(g) b(z0) c(j)"),
        ("(c4) a(g) b(h) [nocustos](z) c(g)", "(c4) a(g) b(hi) [nocustos](z) c(g)"),
        ("(c4) a(g) b(h) (z) c(g)", "(c4) a(g) b(h) (z)"),
        // Words joined and parted, and the first syllable, which the initial takes.
        ("(c4) a(g) b(h) c(g)", "(c4) a(g)b(h) c(g)"),
        ("(c4) a(g)b(h) c(g)", "(c4) a(g) b(h) c(g)"),
        // A word's end where the syllables on either side have no text, which only the
        // break's cost reads.
        ("(c4) a(g) (hg) (fg) c(g)", "(c4) a(g) (hg)(fg) c(g)"),
        (
            "(c4) a(g) b(ghgfghgfghgfghgfghgfghgfghgf) (hg) c(h)",
            "(c4) a(g) b(ghgfghgfghgfghgfghgfghgfghgf)(hg) c(h)",
        ),
        ("(c4) Al(g)le(h) c(g)", "(c4) Bl(g)le(h) c(g)"),
        ("(c4) A(g) b(h) c(g)", "x(c4) A(g) b(h) c(g)"),
        ("name: a;\n%%\n(c4) a(g) b(h)", "name: ab;\n%%\n(c4) a(g) b(h)"),
        ("(c4) a(g) <i>b(h) c(g)", "(c4) a(g) b(h) c(g)"),
        ("(c4) a*(g) b(h) c(g)", "(c4) a(g) b(h) c(g)"),
    ];
    let styles = [Initial::Lines(1), Initial::None, Initial::Lines(2)];
    // Each with a header too, which is what lets a chant parse only around the edit.
    for (header, (before, after)) in ["", "name: x;\n%%\n"].into_iter().flat_map(|h| cases.iter().map(move |c| (h, c))) {
        for initial in styles {
            let style = StyleOptions::default().with_initial(initial);
            let (before, after) = (format!("{header}{before}{tail}"), format!("{header}{after}{tail}"));
            let mut chant = chant(&style);
            let mut shown: Option<SvgParts> = None;
            for src in [&before, &after, &before] {
                let parsed = parse(src);
                let fresh = parsed.score.engrave(&ApproxMeasure, &style);
                chant.update(src);
                assert!(chant.score() == &parsed.score, "{initial:?}: {before:?} to {src:?}");
                assert!(chant.engraving() == &*fresh, "{initial:?}: {before:?} to {src:?}");
                let opts = LayoutOptions::default();
                let svg = SvgOptions::default();
                for width in [900.0, 150.0] {
                    let cached = chant.layout_with(width, &opts);
                    let fresh = fresh.layout_with(width, &opts);
                    assert_eq!(cached.svg_with(&svg), fresh.svg_with(&svg), "{initial:?}: {before:?} to {src:?}");
                    let parts = svg.clone().with_ids(false);
                    let cached = match &shown {
                        Some(shown) => cached.svg_parts_reusing(shown, &parts),
                        None => cached.svg_parts_with(&parts),
                    };
                    assert_eq!(
                        drawn(&cached),
                        drawn(&fresh.svg_parts_with(&parts)),
                        "{initial:?}: {before:?} to {src:?}"
                    );
                    shown = Some(cached);
                }
            }
        }
    }
}

/// Every score in `NEUMA_CORPUS`, when it is set.
#[test]
fn a_cached_engraving_and_layout_are_fresh_ones_across_a_corpus() {
    let Ok(dir) = std::env::var("NEUMA_CORPUS") else { return };
    let edits = env_usize("NEUMA_EDITS", 12);
    let mut paths = Vec::new();
    gabc_in(Path::new(&dir), &mut paths);
    paths.sort();
    for (i, p) in paths.iter().enumerate() {
        let src = fs::read_to_string(p).unwrap_or_default();
        check(&p.display().to_string(), &src, i as u64 + 1, edits);
    }
}
