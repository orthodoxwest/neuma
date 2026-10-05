use super::*;
use crate::score::*;

fn notes(src: &str) -> Vec<Figure> {
    let p = parse(src);
    p.score.syllables.into_iter().flat_map(|s| s.notation).collect()
}

#[test]
fn header_and_body() {
    let p = parse("name: Test;\nannotation: Ant.;\nannotation: VIII G;\nlong: one\ntwo;;\n%%\n(c4) A(g)men.(h) (::)\n");
    assert_eq!(p.score.header.get("name"), Some("Test"));
    assert_eq!(p.score.header.get_all("annotation").collect::<Vec<_>>(), ["Ant.", "VIII G"]);
    assert_eq!(p.score.header.get("long"), Some("one\ntwo"));
    let syl: Vec<String> = p.score.syllables.iter().map(|s| s.text.plain()).collect();
    assert_eq!(syl, ["", "A", "men.", ""]);
    assert!(p.score.syllables[1].word_start);
    assert!(!p.score.syllables[2].word_start);
    assert!(p.score.syllables[3].word_start);
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
}

#[test]
fn pitches_and_positions() {
    assert_eq!(pitch_position('a'), Some(-6));
    assert_eq!(pitch_position('d'), Some(-3));
    assert_eq!(pitch_position('h'), Some(1));
    assert_eq!(pitch_position('M'), Some(6));
    assert_eq!(position_letter(-3), Some('d'));
}

#[test]
fn note_signs() {
    let f = notes("(gv. h_0 i'1 Jw ko1 -f~ gr g..1)");
    let ns: Vec<&Note> = f.iter().filter_map(|f| if let Figure::Note(n) = f { Some(n) } else { None }).collect();
    assert_eq!(ns[0].shape, NoteShape::Virga);
    assert_eq!(ns[0].morae, 1);
    assert_eq!(ns[1].episema.unwrap().placement, Placement::Below);
    assert_eq!(ns[2].ictus, Some(Placement::Above));
    assert_eq!(ns[3].shape, NoteShape::Quilisma);
    assert_eq!(ns[3].position, 3);
    assert_eq!(ns[4].orientation, Some(true));
    assert!(ns[5].initio_debilis);
    assert_eq!(ns[5].liquescent, Liquescent::Deminutus);
    assert!(ns[6].cavum);
    assert_eq!((ns[7].morae, ns[7].mora_placement), (2, Placement::Above));
}

#[test]
fn repeated_strophae_and_virgae() {
    let f = notes("(gsss hvv.)");
    let ns: Vec<&Note> = f.iter().filter_map(|f| if let Figure::Note(n) = f { Some(n) } else { None }).collect();
    assert_eq!(ns.len(), 5);
    assert!(ns[..3].iter().all(|n| n.shape == NoteShape::Stropha));
    assert!(ns[3..].iter().all(|n| n.shape == NoteShape::Virga));
    assert_eq!((ns[3].morae, ns[4].morae), (0, 1));
}

#[test]
fn clefs_alterations_custos() {
    let f = notes("(cb3 f4 c1@c4 ix iX i## gy? g+ z0)");
    assert!(matches!(&f[0], Figure::Clef(Clef { kind: ClefKind::Do, line: 3, flat: true, .. })));
    assert!(matches!(&f[2], Figure::Clef(Clef { kind: ClefKind::Fa, line: 4, .. })));
    assert!(matches!(&f[4], Figure::Clef(Clef { kind: ClefKind::Do, line: 1, .. })));
    let alts: Vec<&Alteration> = f.iter().filter_map(|f| if let Figure::Alteration(a) = f { Some(a) } else { None }).collect();
    assert_eq!(alts.len(), 4);
    assert_eq!((alts[0].kind, alts[0].soft), (AlterationKind::Flat, false));
    assert_eq!((alts[1].kind, alts[1].soft), (AlterationKind::Flat, true));
    assert_eq!((alts[2].kind, alts[2].soft), (AlterationKind::Sharp, true));
    assert!(alts[3].parenthesized);
    assert!(f.iter().any(|f| matches!(f, Figure::Custos { position: Some(0), .. })));
    assert!(f.iter().any(|f| matches!(f, Figure::Custos { position: None, .. })));
}

#[test]
fn bars_breaks_spaces() {
    let f = notes("(` `0 ^ , ,0 ; ;3 : :? :: z Z+ z- g/h//i/0j/!k!l! m/[2.5]g)");
    let bars: Vec<BarKind> = f.iter().filter_map(|f| if let Figure::Bar(b) = f { Some(b.kind) } else { None }).collect();
    assert_eq!(
        bars,
        [
            BarKind::Virgula,
            BarKind::Virgula,
            BarKind::Minimis,
            BarKind::Minima,
            BarKind::Minima,
            BarKind::Minor,
            BarKind::Dominican(3),
            BarKind::Maior,
            BarKind::DottedMaior,
            BarKind::Finalis
        ]
    );
    let breaks: Vec<(bool, CustosRule)> = f.iter().filter_map(|f| if let Figure::Break(b) = f { Some((b.justify, b.custos)) } else { None }).collect();
    assert_eq!(breaks, [(true, CustosRule::Default), (false, CustosRule::Force), (true, CustosRule::Suppress)]);
    let spaces: Vec<Space> = f.iter().filter_map(|f| if let Figure::Space(s) = f { Some(*s) } else { None }).filter(|s| *s != Space::Large).collect();
    assert_eq!(spaces, [Space::Small, Space::Medium, Space::Half, Space::Tiny, Space::Zero, Space::LargeNoBreak, Space::Scaled(2.5)]);
}

#[test]
fn lyric_markup() {
    let p = parse("(c4) <i>Al</i>(g)le(h)<sp>V/</sp> lu{i}a(i) $(x$)(g) <e>j</e>(g) <b>Do</b>(f)\n");
    let s = &p.score.syllables;
    assert!(s[1].text.runs[0].style.italic);
    // Text before `(` belongs to the syllable those notes sing, so the versicle sign and
    // "lu{i}a" share one syllable.
    assert_eq!(s[2].text.plain(), "le");
    assert_eq!(s[3].text.plain(), "℣ luia");
    assert!(s[3].text.runs[0].consonant && s[3].text.runs[0].style.rubric);
    assert_eq!(s[3].text.center, Some(4..5));
    assert_eq!(s[4].text.plain(), "(x)");
    assert!(s[5].text.runs[0].consonant && s[5].text.runs[0].style.italic);
    assert!(s[6].text.runs[0].style.bold);
}

#[test]
fn styles_carry_across_syllables() {
    let p = parse("(c4) <i>Glo(g)ri(h)a</i>(i) Pa(g)\n");
    let s = &p.score.syllables;
    assert!(s[1].text.runs[0].style.italic && s[2].text.runs[0].style.italic && s[3].text.runs[0].style.italic);
    assert!(!s[4].text.runs[0].style.italic);
}

#[test]
fn hyphen_lint_matches_psalm_134() {
    let p = parse("(c4) BE(e)-HOLD(fgh) ser-(h)vants(h)\n");
    let n = p.diagnostics.iter().filter(|d| d.code == "gabc::hyphen-in-syllable").count();
    assert_eq!(n, 2);
}

#[test]
fn comments_and_nlba() {
    let p = parse("% a comment\nname: x; % trailing\n%%\n(c4) <nlba>A(g)men(h)</nlba> % gone\n(::)\n");
    assert_eq!(p.score.header.get("name"), Some("x"));
    let s = &p.score.syllables;
    assert!(!s[1].no_break_before);
    assert!(s[2].no_break_before);
    assert_eq!(s.len(), 4);
}

#[test]
fn unknown_input_recovers() {
    let p = parse("(c4) A(g?q[foo:1]|nabc) B(g\n");
    assert!(!p.diagnostics.is_empty());
    assert_eq!(p.score.syllables.len(), 3);
}

fn strip_spans(mut s: Score) -> Score {
    for syl in &mut s.syllables {
        syl.span = 0..0;
        for f in &mut syl.notation {
            match f {
                Figure::Clef(c) => c.span = 0..0,
                Figure::Note(n) => n.span = 0..0,
                Figure::Alteration(a) => a.span = 0..0,
                Figure::Bar(b) => b.span = 0..0,
                Figure::Break(b) => b.span = 0..0,
                Figure::Custos { span, .. } => *span = 0..0,
                Figure::Space(_) | Figure::NoCustos => {}
            }
        }
    }
    s
}

#[test]
fn round_trip() {
    let srcs = [
        "name: Psalm;\nmode: 8;\n%%\n(c4) BE(e)HOLD(fgh) now,(h.) (,) praise(h) the(h) Lord(h.) (;) Lord.(hgfe.) (::)\n",
        "(cb3) <i>Al</i>(g_'1)le(hv.0)<sp>V/</sp> lu{i}a(i//j/0k!l/[1.5]m) $(x$)(gx g+ z0 Z- [nocustos]) <e>j</e>(Gw -f~ go1 gr gsss hvv.) <nlba>A(g)men(:?)</nlba> <eu>e(g)u(h)</eu>(::)\n",
    ];
    for src in srcs {
        let first = parse(src);
        let written = first.score.to_gabc();
        let second = parse(&written);
        assert_eq!(strip_spans(first.score), strip_spans(second.score), "\n{written}");
    }
}
