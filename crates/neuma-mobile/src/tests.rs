use super::*;

const KYRIE: &str = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)";

fn options() -> LayoutOptions {
    LayoutOptions::default()
}

fn chant(src: &str) -> Arc<Chant> {
    Chant::new(src.to_string(), ChantOptions::default())
}

fn timeline(p: &Page) -> &Timeline {
    p.timeline.as_ref().expect("a timeline")
}

#[test]
fn lays_out_with_stable_ids() {
    let c = chant(KYRIE);
    assert!(c.diagnostics().is_empty(), "{:?}", c.diagnostics());
    let wide = c.layout(600.0, options());
    let narrow = c.layout(120.0, options());
    assert!(narrow.lines.len() > wide.lines.len());
    let ids = |p: &Page| timeline(p).notes.iter().map(|n| n.id).collect::<Vec<_>>();
    assert_eq!(ids(&wide), ids(&narrow));
    assert_eq!(wide.staff_space, 6.0);
    // Every note has ink, and the initial and lyrics are text.
    for n in &timeline(&wide).notes {
        let inked = wide.items.iter().any(|i| match i {
            Item::Glyph { notes, .. } | Item::Rect { notes, .. } => notes.contains(&n.id),
            Item::Text { .. } => false,
        });
        assert!(inked, "note {}", n.id);
    }
    assert!(wide.items.iter().any(|i| matches!(
        i,
        Item::Text {
            role: TextRole::Initial,
            ..
        }
    )));
    assert_eq!(wide.alt_text.split_whitespace().next(), Some("Kyrie"));
}

#[test]
fn timeline_and_hit_testing() {
    let c = chant(KYRIE);
    assert_eq!(c.note_at(0.0, 0.0), None);
    let page = c.layout(600.0, options());
    let t = timeline(&page);
    let kinds: Vec<_> = t.pauses.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, [PauseKind::Mediant, PauseKind::Double]);
    let last = t.notes.last().unwrap();
    assert!(t.duration >= last.start + last.duration);
    for n in &t.notes {
        assert_eq!(c.note_at(n.cx, n.cy), Some(n.id));
    }
    assert_eq!(t.notes[3].half, 1);
    // Without the timeline, hit tests answer all the same.
    let quick = c.layout(
        300.0,
        LayoutOptions {
            timeline: false,
            ..options()
        },
    );
    assert!(quick.timeline.is_none() && !quick.items.is_empty());
    assert!(c.note_at(0.0, 0.0).is_some());
}

#[test]
fn porrectus_ink_names_both_notes() {
    let page = chant("(c4) a(hgh)").layout(400.0, options());
    let swash = page.items.iter().find_map(|i| match i {
        Item::Glyph { notes, .. } if notes.len() == 2 => Some(notes.clone()),
        Item::Glyph { .. } | Item::Rect { .. } | Item::Text { .. } => None,
    });
    assert_eq!(swash, Some(vec![0, 1]));
}

#[test]
fn glyph_outlines_are_absolute_paths() {
    let page = chant(KYRIE).layout(600.0, options());
    for i in &page.items {
        if let Item::Glyph { glyph, .. } = i {
            let g = glyph_outline(*glyph).unwrap();
            assert!(g.path.starts_with('M') && g.width > 0.0);
            assert!(
                g.path.chars().filter(char::is_ascii_alphabetic).all(|c| "MLCZ".contains(c)),
                "{}",
                g.path
            );
        }
    }
    assert_eq!(glyph_outline(-1), None);
    assert_eq!(glyph_outline(i32::MAX), None);
}

#[test]
fn bad_options_keep_defaults() {
    let c = Chant::new(
        KYRIE.to_string(),
        ChantOptions {
            initial: 9,
            annotation: false,
            lyric_size: f32::NAN,
            font: Some(LyricFont::Garamond12),
        },
    );
    let mut opts = options();
    opts.scale = f32::INFINITY;
    opts.max_lines = -3;
    opts.weights.note = -1.0;
    opts.weights.mora = f32::NAN;
    let page = c.layout(f32::NAN, opts);
    assert_eq!(page.staff_space, 6.0);
    assert!(timeline(&page).notes.iter().all(|n| n.duration == 1.0));
    let mut huge = options();
    huge.weights.note = f32::MAX;
    assert!(timeline(&c.layout(400.0, huge)).notes.iter().all(|n| n.duration == 1000.0));
    assert!(page.width.is_finite() && page.height.is_finite());
    // A negative initial is none, as in the engine.
    let none = Chant::new(
        KYRIE.to_string(),
        ChantOptions {
            initial: -1,
            ..ChantOptions::default()
        },
    );
    let drop_cap = |c: &Chant| {
        c.layout(600.0, options()).items.iter().any(|i| {
            matches!(
                i,
                Item::Text {
                    role: TextRole::Initial,
                    ..
                }
            )
        })
    };
    assert!(!drop_cap(&none) && drop_cap(&chant(KYRIE)));
}

#[test]
fn diagnostics_carry_byte_spans() {
    let c = chant("(c4) Dó(g)mi(h)nus(Q)");
    let d = c.diagnostics();
    assert!(!d.is_empty());
    assert_eq!(&"(c4) Dó(g)mi(h)nus(Q)"[d[0].start as usize..d[0].end as usize], "Q");
}

#[test]
fn record_defaults_match_the_engine() {
    // The records' `#[uniffi(default = …)]` literals, written out again here, must stay
    // equal to the engine's defaults, which `Default` reads.
    let w = Weights::default();
    assert_eq!(
        [
            w.note, w.mora, w.episema, w.virgula, w.quarter, w.half, w.full, w.double, w.mediant, w.flex
        ],
        [1.0, 2.0, 1.5, 0.5, 0.5, 1.0, 2.0, 3.0, 2.0, 1.0]
    );
    let engine = neuma::ChantOptions::default();
    assert_eq!(neuma::ChantOptions::from(ChantOptions::default()), engine);
    assert_eq!(engine.style.lyric_size, 2.45);
    assert_eq!(engine.style.initial, neuma::Initial::Lines(1));
    assert!(engine.style.annotation);
    assert_eq!(
        neuma::LayoutOptions::from(LayoutOptions::default()),
        neuma::LayoutOptions::default()
    );
    assert_eq!(neuma::LayoutOptions::default().scale, 6.0);
    let psalm = neuma_tones::PsalmOptions::default();
    assert!(psalm.auto_point && PsalmOptions::default().auto_point);
}

#[test]
fn summaries_and_previews() {
    let src = "name: Kyrie;\noffice-part: Kyrie;\nmode: 8G;\n%%\n(c4) KY(g)ri(h)e(g) (,) e(h)le(g)i(h)son(g) (::)";
    let s = summarize(src.to_string());
    assert_eq!(
        (s.kind, s.mode.as_ref().map(|m| (m.number, m.differentia.clone()))),
        (Some(OfficePart::Kyrie), Some((Some(8), Some("G".to_string()))))
    );
    assert_eq!(s.text, "Kyrie eleison");
    // g and h under a do clef on the top line are sol and la below do.
    assert_eq!((s.lowest, s.highest), (Some(-5), Some(-3)));
    assert_eq!(chant(src).summary(), s);

    let long = chant(&format!("(c4) {}(::)", "a(g) ".repeat(60)));
    let full = long.layout(200.0, options());
    let mut one = options();
    one.max_lines = 1;
    let preview = long.layout(200.0, one);
    assert!(full.lines.len() > 1 && preview.lines.len() == 1);
    assert_eq!(preview.lines[0], full.lines[0]);
    assert!(timeline(&preview).notes.iter().all(|n| n.line == 0));
}

#[test]
fn sets_psalms() {
    let text =
        "1 The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength.";
    let s = psalm(text.to_string(), "8.G".to_string(), PsalmOptions::default()).unwrap();
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let page = chant(&s.gabc).layout(600.0, options());
    assert_eq!(timeline(&page).notes.len(), s.notes.len());
    assert_eq!(s.notes[0].role, ToneRole::Intonation);
    assert_eq!(s.notes[0].number, Some(1));
    let accent = s.notes.iter().position(|n| n.role == ToneRole::Accent).unwrap();
    let n = &s.notes[accent];
    assert_eq!(&text[n.start as usize..n.end as usize], "pá");
    let utf16: Vec<u16> = text.encode_utf16().collect();
    assert_eq!(String::from_utf16_lossy(&utf16[n.utf16_start as usize..n.utf16_end as usize]), "pá");
    let never = PsalmOptions {
        intone: Some(Intone::Never),
        ..PsalmOptions::default()
    };
    assert_eq!(
        psalm(text.to_string(), "9.z".to_string(), never),
        Err(ToneError::Unknown { name: "9.z".into() })
    );
    assert!(matches!(
        psalm_with_tone(text.to_string(), "name: x".to_string(), never),
        Err(ToneError::Invalid { .. })
    ));
    let s2 = psalm(text.to_string(), "8.G".to_string(), never).unwrap();
    assert_ne!(s2.notes[0].role, ToneRole::Intonation);
    assert!(tone_names().contains(&"8.G".to_string()));
}

#[test]
fn points_psalms() {
    let plain = "O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.";
    let p = point(plain.to_string(), "8.G".to_string()).unwrap();
    assert_eq!(p.halves.len(), 2);
    assert!(p.halves.iter().all(|h| !h.kept && h.confidence > 0.0 && h.confidence <= 1.0));
    assert_eq!(p.halves[1].part, VersePart::Termination);
    assert!(p.text.contains('·'));
    // Setting the plain text points it the same way.
    let a = psalm(plain.to_string(), "8.G".to_string(), PsalmOptions::default()).unwrap();
    let b = psalm(p.text.clone(), "8.G".to_string(), PsalmOptions::default()).unwrap();
    assert_eq!(a.gabc, b.gabc);
    let manual = PsalmOptions {
        auto_point: false,
        ..PsalmOptions::default()
    };
    let unpointed = psalm(plain.to_string(), "8.G".to_string(), manual).unwrap();
    assert!(unpointed.diagnostics.iter().any(|d| d.code == "apply::no-accent"));
    assert!(matches!(
        point(plain.to_string(), "9.z".to_string()),
        Err(ToneError::Unknown { .. })
    ));
}

#[test]
fn diagnostics_carry_utf16_offsets_and_fixes() {
    let src = "(c4) Dó-(g)mi(h)nus(h)";
    let d = chant(src).diagnostics();
    let h = d.iter().find(|d| d.code == "gabc::hyphen-in-syllable").unwrap();
    assert_eq!(&src[h.start as usize..h.end as usize], "-");
    assert_eq!(h.utf16_start, h.start - 1);
    let fix = h.fix.as_ref().unwrap();
    assert_eq!(
        (fix.utf16_start, fix.utf16_end, fix.replacement.as_str()),
        (h.utf16_start, h.utf16_end, "")
    );
    assert!(chant("(c4) a(g)").diagnostics().iter().all(|d| d.fix.is_none()));
}

#[test]
fn source_and_score_link_both_ways() {
    let src = "(c4) Kŷ(g)ri(hi) (,) e(h) (::)";
    let c = chant(src);
    assert!(c.source_at(0.0, 0.0).is_none() && c.elements_at(0, OffsetUnit::Utf8).is_empty());
    let page = c.layout(500.0, options());
    for n in &timeline(&page).notes {
        let hit = c.source_at(n.cx, n.cy).unwrap();
        assert_eq!((hit.kind, hit.index), (ElementKind::Note, n.id));
        assert_eq!((hit.start, hit.utf16_end), (n.source_start, n.source_utf16_end));
    }
    let hi = src.find("hi").unwrap() as i32;
    let at = c.elements_at(hi + 1, OffsetUnit::Utf8);
    assert_eq!(
        at.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [ElementKind::Note, ElementKind::Syllable]
    );
    assert_eq!(&src[at[0].start as usize..at[0].end as usize], "i");
    // `ŷ` is two bytes but one UTF-16 unit.
    assert_eq!(c.elements_at(hi, OffsetUnit::Utf16), at);
    assert_eq!(at[0].utf16_start, at[0].start - 1);
    let bar = c.elements_at(src.find(',').unwrap() as i32, OffsetUnit::Utf8);
    assert_eq!(bar[0].kind, ElementKind::Bar);
    assert_eq!(c.elements_at(-5, OffsetUnit::Utf16), c.elements_at(0, OffsetUnit::Utf16));
}

#[test]
fn updates_in_place() {
    let c = chant("(c4) a-(g)");
    assert!(c.diagnostics().iter().any(|d| d.code == "gabc::hyphen-in-syllable"));
    c.layout(400.0, options());
    assert!(c.note_at(0.0, 0.0).is_some());
    c.update("(c4) a(g) b(h)".to_string());
    assert!(c.diagnostics().is_empty());
    assert_eq!(c.note_at(0.0, 0.0), None, "an update drops the last layout");
    let page = c.layout(400.0, options());
    let fresh = chant("(c4) a(g) b(h)").layout(400.0, options());
    assert_eq!(page, fresh);
    assert_eq!(c.summary().notes, 2);
}
