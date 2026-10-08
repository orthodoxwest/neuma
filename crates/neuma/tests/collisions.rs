//! Nothing collides: no note, stem or ledger line over a lyric's letters, and no ink off the
//! page. Letters are boxed by their advances and a height per kind of letter, which is
//! cruder than their outlines (`crates/neuma-metrics/examples/collisions.rs` uses those, over
//! a whole corpus) but needs no font file.

use std::path::Path;

use neuma::glyphs::{GlyphId, UNITS_PER_SPACE};
use neuma::{Ink, Item, LyricFont, StyleOptions, TextMeasure, TextRole, parse};

fn scores() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut out: Vec<(String, String)> = ["golden", "corpus"]
        .iter()
        .flat_map(|d| std::fs::read_dir(root.join(d)).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "gabc"))
        .map(|p| (p.display().to_string(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    // Notes under the staff over letters of every height, and a tall initial over them.
    out.push((
        "low notes".into(),
        "(c4) Lá(b)l(a)ti(b)ò(c)nem(a) Ál(b)tí(a)ssi(b)mi(a) (,) ge(bab)ni(a)tus(b.) (::)".into(),
    ));
    // Initials with tails over a second line that starts high (GregoBase 14123 and 6402).
    out.push((
        "q tail".into(),
        "mode: 4;\n%%\n(c2)Quem(dv) quǽ(cb)ri(cd)tis(d) in(c) se(e)púl(fgf~)chro(edf) chri(d)stí(cb)co(cd)le.(d.) (:) Je(cd)sum(d) Na(c)za(ev)ré(fg)num(f) cru(fe)ci(d)fí(ed)xum(cd,) o(cf) cæ(f)lí(ed)co(cd)læ.(d.) (::)".into(),
    ));
    out.push((
        "j tail".into(),
        "%%\n(c2) Jo(cfe)seph,(f) (::) fi(g)li(f) Da(df)vid,(f) (;) no(f)li(f) ti(e)mé(c!dfd)re(efe) (;1) ac(ef)cí(gh)pe(g)re(ghg) Ma(f)rí(d)am(c) cón(d)ju(f)gem(e) tu(c)am:(c) (::)".into(),
    ));
    out.sort();
    out
}

/// How high a letter reaches above the baseline, in ems: generous for each kind.
fn top(c: char) -> f32 {
    match c {
        'a' | 'c' | 'e' | 'g' | 'm' | 'n' | 'o' | 'q' | 'r' | 's' | 'u' | 'v' | 'w' | 'x' | 'y' | 'z' | 'æ' | 'œ' => 0.45,
        '-' | '.' | ',' | ' ' => 0.3,
        c if c.is_ascii() => 0.73,
        c if c.is_lowercase() => 0.7,
        _ => 0.88,
    }
}

#[test]
fn notes_keep_off_the_lyrics_and_ink_stays_on_the_page() {
    let metrics = LyricFont::Google.metrics();
    for (name, src) in scores() {
        let eng = parse(&src).score.engrave(metrics, &StyleOptions::default());
        for width in [300.0, 500.0, 800.0, 1200.0] {
            let list = eng.layout(width).display();
            let sp = list.staff_space;
            let ctx = format!("{name} at {width}");
            let mut ink: Vec<[f32; 4]> = Vec::new();
            let mut letters: Vec<([f32; 4], char)> = Vec::new();
            for item in &list.items {
                match item {
                    Item::Glyph { glyph, x, y, scale, .. } => {
                        let (a, b, c, d) = GlyphId::from_id(*glyph).unwrap().ink();
                        let k = scale * UNITS_PER_SPACE;
                        ink.push([x + a * k, y + b * k, x + c * k, y + d * k]);
                    }
                    Item::Rect { x, y, w, h, role, .. } if *role != Ink::Staff => ink.push([*x, *y, x + w, y + h]),
                    Item::Rect { .. } => {}
                    Item::Text {
                        x,
                        baseline,
                        size,
                        runs,
                        role,
                        ..
                    } => {
                        let mut pen = *x;
                        for r in runs {
                            let mut prefix = String::new();
                            for c in r.text.chars() {
                                let left = pen + metrics.advance(&prefix, r.style) * size;
                                prefix.push(c);
                                let right = pen + metrics.advance(&prefix, r.style) * size;
                                if !c.is_whitespace() && matches!(role, TextRole::Lyric | TextRole::Rubric | TextRole::Hyphen) {
                                    letters.push(([left, baseline - top(c) * size, right, *baseline], c));
                                }
                                // Text stays on the page, descenders included.
                                assert!(
                                    left >= -0.1 * sp && right <= list.width + 0.1 * sp,
                                    "{ctx}: {c:?} at {left}..{right}"
                                );
                                assert!(baseline + 0.3 * size <= list.height + 0.1 * sp, "{ctx}: {c:?} below the page");
                            }
                            pen += metrics.advance(&r.text, r.style) * size;
                        }
                    }
                    _ => {}
                }
            }
            for b in &ink {
                let inside = b[0] >= -0.1 * sp && b[1] >= -0.1 * sp && b[2] <= list.width + 0.1 * sp && b[3] <= list.height + 0.1 * sp;
                assert!(inside, "{ctx}: ink at {b:?} off a {} x {} page", list.width, list.height);
            }
            for (l, c) in &letters {
                for b in &ink {
                    let pen = (l[2].min(b[2]) - l[0].max(b[0])).min(l[3].min(b[3]) - l[1].max(b[1]));
                    assert!(pen <= 0.05 * sp, "{ctx}: {c:?} at {l:?} under ink at {b:?}");
                }
            }
        }
    }
}

#[test]
fn the_initial_clears_the_staves_and_notes_below_it() {
    // A drop cap's tail (a Q's, a J's) hangs below the first line's lyrics, over the second
    // line: it must clear that line's staff lines as well as its notes.
    let metrics = LyricFont::Google.metrics();
    // How far a capital's ink reaches below its baseline, and past its advance on the right,
    // in ems (EB Garamond's outlines, rounded up).
    let reach = |c: char| match c {
        'Q' => (0.25, 0.13),
        'J' => (0.21, 0.0),
        'g' | 'j' | 'p' | 'q' | 'y' => (0.3, 0.0),
        _ => (0.02, 0.0),
    };
    for (name, src) in scores() {
        let eng = parse(&src).score.engrave(metrics, &StyleOptions::default());
        for width in [300.0, 500.0, 800.0, 1200.0] {
            let list = eng.layout(width).display();
            let sp = list.staff_space;
            let Some(cap) = list.items.iter().find_map(|i| match i {
                Item::Text {
                    role: TextRole::Initial,
                    x,
                    baseline,
                    size,
                    runs,
                    ..
                } => {
                    let c = runs[0].text.chars().next()?;
                    let (depth, tail) = reach(c);
                    let right = x + (metrics.advance(&runs[0].text, runs[0].style) + tail) * size;
                    Some([*x, baseline - 0.65 * size, right, baseline + depth * size])
                }
                _ => None,
            }) else {
                continue;
            };
            for item in &list.items {
                let b = match item {
                    Item::Glyph { glyph, x, y, scale, .. } => {
                        let (a, b, c, d) = GlyphId::from_id(*glyph).unwrap().ink();
                        let k = scale * UNITS_PER_SPACE;
                        [x + a * k, y + b * k, x + c * k, y + d * k]
                    }
                    Item::Rect { x, y, w, h, .. } => [*x, *y, x + w, y + h],
                    _ => continue,
                };
                let pen = (cap[2].min(b[2]) - cap[0].max(b[0])).min(cap[3].min(b[3]) - cap[1].max(b[1]));
                assert!(pen <= 0.05 * sp, "{name} at {width}: the initial {cap:?} over ink at {b:?}");
            }
        }
    }
}
