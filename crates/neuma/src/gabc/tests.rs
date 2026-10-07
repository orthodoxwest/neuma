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
    let ns: Vec<&Note> = f
        .iter()
        .filter_map(|f| if let Figure::Note(n) = f { Some(n) } else { None })
        .collect();
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
    let ns: Vec<&Note> = f
        .iter()
        .filter_map(|f| if let Figure::Note(n) = f { Some(n) } else { None })
        .collect();
    assert_eq!(ns.len(), 5);
    assert!(ns[..3].iter().all(|n| n.shape == NoteShape::Stropha));
    assert!(ns[3..].iter().all(|n| n.shape == NoteShape::Virga));
    assert_eq!((ns[3].morae, ns[4].morae), (0, 1));
}

#[test]
fn clefs_alterations_custos() {
    let f = notes("(cb3 f4 c1@c4 ix iX i## gy? g+ z0)");
    assert!(matches!(
        &f[0],
        Figure::Clef(Clef {
            kind: ClefKind::Do,
            line: 3,
            flat: true,
            ..
        })
    ));
    assert!(matches!(
        &f[2],
        Figure::Clef(Clef {
            kind: ClefKind::Fa,
            line: 4,
            ..
        })
    ));
    assert!(matches!(
        &f[4],
        Figure::Clef(Clef {
            kind: ClefKind::Do,
            line: 1,
            ..
        })
    ));
    let alts: Vec<&Alteration> = f
        .iter()
        .filter_map(|f| if let Figure::Alteration(a) = f { Some(a) } else { None })
        .collect();
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
    let bars: Vec<BarKind> = f
        .iter()
        .filter_map(|f| if let Figure::Bar(b) = f { Some(b.kind) } else { None })
        .collect();
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
    let breaks: Vec<(bool, CustosRule)> = f
        .iter()
        .filter_map(|f| {
            if let Figure::Break(b) = f {
                Some((b.justify, b.custos))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        breaks,
        [
            (true, CustosRule::Default),
            (false, CustosRule::Force),
            (true, CustosRule::Suppress)
        ]
    );
    let spaces: Vec<Space> = f
        .iter()
        .filter_map(|f| if let Figure::Space(s) = f { Some(*s) } else { None })
        .filter(|s| *s != Space::Large)
        .collect();
    assert_eq!(
        spaces,
        [
            Space::Small,
            Space::Medium,
            Space::Half,
            Space::Tiny,
            Space::Zero,
            Space::LargeNoBreak,
            Space::Scaled(2.5)
        ]
    );
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
        "(c4) <e>A</e>(g) <sp>A/</sp>(h) <e>Aj</e>(g)\n",
        "\u{feff}name: a; b;;\n%%\n(c4) A(g)\n",
        "(cb3) <i>Al</i>(g_'1)le(hv.0)<sp>V/</sp> lu{i}a(i//j/0k!l/[1.5]m) $(x$)(gx g+ z0 Z- [nocustos]) <e>j</e>(Gw -f~ go1 gr gsss hvv.) <nlba>A(g)men(:?)</nlba> <eu>e(g)u(h)</eu>(::)\n",
    ];
    for src in srcs {
        let first = parse(src);
        let written = first.score.to_gabc();
        let second = parse(&written);
        assert_eq!(strip_spans(first.score), strip_spans(second.score), "\n{written}");
    }
}

#[test]
fn header_bom_and_semicolons() {
    let p = parse("\u{feff}name: a; b;;\n%%\n(c4) A(g)\n");
    assert_eq!(p.score.header.get("name"), Some("a; b"));
    let p = parse("\u{feff}(c4) A(g)");
    assert_eq!(
        p.score.syllables[0].text.runs.iter().map(|r| r.text.as_str()).collect::<String>(),
        ""
    );
    let p = parse("name: a;\n\u{feff}%%\n(c4) A(g)");
    assert_eq!(p.score.header.get("name"), Some("a"));
}

#[test]
fn repeated_underscores_extend_the_episema() {
    let marked = |src: &str| -> Vec<bool> {
        notes(src)
            .into_iter()
            .filter_map(|f| if let Figure::Note(n) = f { Some(n.episema.is_some()) } else { None })
            .collect()
    };
    assert_eq!(marked("(c4) a(fgf___)"), [true, true, true]);
    assert_eq!(marked("(c4) a(fgf__)"), [false, true, true]);
    assert_eq!(marked("(c4) a(fgf_)"), [false, false, true]);
    // A space ends the group.
    assert_eq!(marked("(c4) a(f/gf___)"), [false, true, true]);
    assert_eq!(marked("(c4) a(fg_0f_)"), [false, true, true]);
    // `!` joins notes into one glyph without ending the group.
    assert_eq!(marked("(c4) a(f!g__)"), [true, true]);
    // An accidental isn't a note, so the run stops at it.
    assert_eq!(marked("(c4) a(fgxg___)"), [false, true]);
    // Digits after the run modify its episema, on every note it covers.
    let below = |src: &str| -> Vec<bool> {
        notes(src)
            .into_iter()
            .filter_map(|f| {
                if let Figure::Note(n) = f {
                    Some(n.episema.is_some_and(|e| e.placement == Placement::Below))
                } else {
                    None
                }
            })
            .collect()
    };
    assert_eq!(below("(c4) a(fgf__0)"), [false, true, true]);
}

#[test]
fn parentheses_in_verbatim_and_alt_text_are_lyric() {
    // From GregoBase: an editorial note in parentheses set with `<v>`.
    let p = parse("(c4) <i><v>(</v>Non repetitur.<v>)</v></i>(d) A(g)\n");
    let s = &p.score.syllables;
    assert_eq!(s.len(), 3);
    assert_eq!(s[1].text.plain(), "(Non repetitur.)");
    assert!(s[1].text.runs[0].style.italic);
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
    let p = parse("(c4) A<alt>(octava)</alt>(g) B(h)\n");
    assert_eq!(p.score.syllables.len(), 3);
    assert_eq!(p.score.syllables[1].text.plain(), "A");
    // An unclosed tag doesn't swallow the rest of the score.
    let p = parse("(c4) A<v>(g) B(h)\n");
    assert_eq!(p.score.syllables.len(), 3);
}

#[test]
fn header_missing_semicolon_ends_at_next_field() {
    // From GregoBase: `name` lacks its `;`, which hid every field after it.
    let p = parse("initial-style: 1;\nname: Angelus Domini\nbook: Antiphonale, p. 15;\nannotation: 1f;\n%%\n(c4) A(g)\n");
    assert_eq!(p.score.header.get("name"), Some("Angelus Domini"));
    assert_eq!(p.score.header.get("book"), Some("Antiphonale, p. 15"));
    assert_eq!(p.score.header.get("annotation"), Some("1f"));
    let n = p.diagnostics.iter().filter(|d| d.code == "gabc::unterminated-header").count();
    assert_eq!(n, 1);
    // Gregorio also ends a multi-line value at a `;` that ends a line.
    let p = parse("commentary: one\ntwo;\nmode: 1;\n%%\n(c4) A(g)\n");
    assert_eq!(p.score.header.get("commentary"), Some("one\ntwo"));
    assert_eq!(p.score.header.get("mode"), Some("1"));
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
}

#[test]
fn notes_with_gregorio_6_syntax() {
    // `//[-0.5]` is a cut and then a scaled space, not a medium space and a tag.
    let f = notes("(c4) a(jH0//[-0.5]{ix}F0hi)");
    let spaces: Vec<Space> = f
        .iter()
        .filter_map(|f| if let Figure::Space(s) = f { Some(*s) } else { None })
        .collect();
    assert_eq!(spaces, [Space::Small, Space::Scaled(-0.5)]);
    // `<nlba>` inside notes keeps its notes on one line and adds none.
    let p = parse("(c4) a(f.___</nlba>) b(<nlba>g h</nlba> i)\n");
    let count = |s: &Syllable| s.notation.iter().filter(|f| matches!(f, Figure::Note(_))).count();
    assert_eq!(count(&p.score.syllables[1]), 1);
    assert_eq!(count(&p.score.syllables[2]), 3);
    assert!(matches!(&p.score.syllables[1].notation[0], Figure::Note(n) if n.liquescent == Liquescent::None));
    let spaces: Vec<&Figure> = p.score.syllables[2]
        .notation
        .iter()
        .filter(|f| matches!(f, Figure::Space(_)))
        .collect();
    assert_eq!(spaces, [&Figure::Space(Space::LargeNoBreak), &Figure::Space(Space::Large)]);
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
}

#[test]
fn verbatim_stars_and_crosses_print() {
    // From GregoBase: `<v>\greheightstar</v>` is the asterisk after the intonation.
    let p = parse(r"(c4) Ec(g)ce <v>\greheightstar</v>(h) quam(g) <v>\ \GreDagger</v>(g) bo<v>\ddag\ </v>(h)");
    let s = &p.score.syllables;
    assert_eq!(s[2].text.plain(), "ce *");
    assert_eq!(s[4].text.plain(), "†");
    assert_eq!(s[5].text.plain(), "bo‡");
    assert!(s[2].text.runs.iter().any(|r| r.text == "*" && r.style.rubric));
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
    let again = parse(&p.score.to_gabc());
    assert_eq!(again.score.syllables[2].text, s[2].text);
    assert_eq!(again.score.syllables[4].text, s[4].text);
    assert_eq!(again.score.syllables[5].text, s[5].text);
}

#[test]
fn verbatim_sign_names_need_a_backslash() {
    // Without the backslash, TeX prints the word, not the sign.
    let p = parse("(c4) a<v>star</v>(g) b<v>{dag}</v>(h) c<v>$\\star$</v>(g)");
    let s = &p.score.syllables;
    assert_eq!(s[1].text.plain(), "astar");
    assert!(!s[2].text.plain().contains('†'));
    assert_eq!(s[3].text.plain(), "c*");
}

#[test]
fn comma_digit_is_a_dominican_bar() {
    let bars: Vec<BarKind> = notes("(c4) a(f;3 f,4 f,0)")
        .iter()
        .filter_map(|f| if let Figure::Bar(b) = f { Some(b.kind) } else { None })
        .collect();
    assert_eq!(bars, [BarKind::Dominican(3), BarKind::Dominican(4), BarKind::Minima]);
    let p = parse("(c4) a(f;8)");
    // `;7` and `;8` reach from the top line up past the staff.
    assert!(
        p.diagnostics
            .iter()
            .any(|d| d.code == "gabc::dominican-bar" && d.message.contains("partly above the staff"))
    );
    assert!(parse("(c4) a(f;3)").diagnostics.is_empty());
}

#[test]
fn episema_tuning_is_only_a_note() {
    // From GregoBase: `[oh:h]` after an episema, `[ll:1]` on a stem, and an `[oh:h{]…[oh}]` block.
    let p = parse("(c4) a(d_[oh:h]e_[uh:l] e[ll:1]d ix[oh:h{]g_d//f_eg.[oh}])");
    assert!(!p.diagnostics.is_empty());
    assert!(
        p.diagnostics
            .iter()
            .all(|d| d.code == "gabc::tuning-ignored" && d.severity == crate::diag::Severity::Info),
        "{:?}",
        p.diagnostics
    );
    let p = parse("(c4) a(g[oll:1]h[nv:x])");
    assert_eq!(p.diagnostics.iter().filter(|d| d.code == "gabc::unsupported-tag").count(), 2);
}

#[test]
fn nabc_is_reported_once() {
    let p = parse("nabc-lines: 1;\n%%\n(c4) A(f|vi) B(g|pe) C(h|ta)\n");
    let nabc: Vec<_> = p.diagnostics.iter().filter(|d| d.code == "gabc::nabc").collect();
    assert_eq!(nabc.len(), 1, "{:?}", p.diagnostics);
    assert_eq!(p.score.syllables.len(), 4);
}

#[test]
fn unclosed_verbatim_tags_parse_in_linear_time() {
    // Each unclosed `<alt>` used to rescan the rest of the score for a closer.
    let src = format!("(c4) {}", "a<alt>(g) ".repeat(50_000));
    let t = std::time::Instant::now();
    let p = parse(&src);
    assert_eq!(p.score.syllables.len(), 50_001);
    // Linear parsing takes well under a second here, even unoptimized; the quadratic scan took
    // seconds in a release build. The bound is loose so a slow runner can't trip it.
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
}

#[test]
fn plain_verbatim_text_collapses_spaces() {
    let p = parse("(c4) A<v>(non\n   repetitur)  </v>b(g)\n");
    assert_eq!(p.score.syllables[1].text.plain(), "A(non repetitur) b");
}

#[test]
fn double_slash_before_a_tag_is_the_larger_space() {
    // From GregoBase: `//` then a ledger-line tag, not a cut and a malformed scaled space.
    let p = parse("(c4) a(jk//[oll:1{1]lkl[oll:}])");
    let spaces: Vec<Space> = p.score.syllables[1]
        .notation
        .iter()
        .filter_map(|f| if let Figure::Space(s) = f { Some(*s) } else { None })
        .collect();
    assert_eq!(spaces, [Space::Medium]);
    let codes: Vec<&str> = p.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["gabc::unsupported-tag", "gabc::unsupported-tag"]);
    let once = p.score.to_gabc();
    assert_eq!(parse(&once).score.to_gabc(), once);
}

#[test]
fn zero_width_notes_are_reported_once() {
    let p = parse("(c4) a(gF0/[-0.5]{ix}F0hi) b(h/[-0.5]{iy}hg)\n");
    let n = p.diagnostics.iter().filter(|d| d.code == "gabc::zero-width").count();
    assert_eq!(n, 1, "{:?}", p.diagnostics);
    let d = p.diagnostics.iter().find(|d| d.code == "gabc::zero-width").unwrap();
    assert!(d.message.ends_with("in all 2 groups"), "{}", d.message);
    // A single group is one diagnostic, worded for that group alone.
    let p = parse("(c4) a(gF0/[-0.5]{ix}F0hi)\n");
    let zw: Vec<_> = p.diagnostics.iter().filter(|d| d.code == "gabc::zero-width").collect();
    assert_eq!(zw.len(), 1, "{:?}", p.diagnostics);
    assert_eq!(zw[0].message, "notes in `{…}` are drawn with their own width");
}

/// Inputs the `round_trip` fuzz target and the corpus run found: writing the parsed score and
/// parsing it again reaches a fixed point after one round.
#[test]
fn writer_is_stable_on_fuzz_finds() {
    let cases = [
        // An escaped line break or tab in a lyric is a space.
        "$\ns(",
        "us$\n- `psalm-13i) *() na(m) *(,",
        "a$\tb(g)",
        "a$\rb(g) c$\n(h)",
        // A multi-line header value whose first line has a `;` is written on one line.
        ":\n;\0\n\u{7f}\n%%",
        "name: a\nb;\nc;;\n%%\n(c4) a(g)",
        // One whose first line ends in a space, which reading trims.
        ":\n! \n!\n%%",
        // Found in GregoBase by the corpus run: a word space before `</nlba>`, and a centered
        // special character.
        "(c4) <nlba>* Dul(h)ce(hji) </nlba>li(g)gnum,(ge) (:) vé(f)<nlba>ni(fgf)ent,(f) </nlba>(:)",
        "(c4) f{<sp>'oe</sp>}(h')de(g)ra(fe..) <sp>A/</sp>{<sp>A/</sp>}(g)",
        // An inclinatum's lean on a note another sign made an oriscus.
        "(G1o",
        "(c4) a(G1oh G2s G0v)",
    ];
    for src in cases {
        let once = parse(src).score.to_gabc();
        assert_eq!(parse(&once).score.to_gabc(), once, "{src:?}");
    }
    let p = parse("a$\tb(g)");
    assert_eq!(p.score.syllables[0].text.runs[0].text, "a b");
    // The word after `</nlba>` still starts a word, and the centering survives.
    let once = parse(&parse("(c4) <nlba>a(h)b(g) </nlba>c(g) f{<sp>'oe</sp>}(h)").score.to_gabc()).score;
    assert!(once.syllables[3].word_start);
    assert_eq!(once.syllables[4].text.center, Some(1..3));
    // A multi-line value that reads back keeps its lines.
    let p = parse("commentary: one\ntwo;;\n%%\n(c4) a(g)");
    assert_eq!(p.score.header.get("commentary"), Some("one\ntwo"));
    assert!(p.score.to_gabc().starts_with("commentary: one\ntwo;;\n"));
}
