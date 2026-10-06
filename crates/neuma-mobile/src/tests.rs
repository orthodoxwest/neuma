use super::*;

const KYRIE: &str = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)";

fn options() -> LayoutOptions {
    default_layout_options()
}

fn chant(src: &str) -> Arc<Chant> {
    Chant::new(src.to_string(), default_chant_options())
}

#[test]
fn lays_out_with_stable_ids() {
    let c = chant(KYRIE);
    assert!(c.diagnostics().is_empty(), "{:?}", c.diagnostics());
    let wide = c.layout(600.0, options());
    let narrow = c.layout(120.0, options());
    assert!(narrow.lines.len() > wide.lines.len());
    let ids = |p: &Page| p.notes.iter().map(|n| n.id).collect::<Vec<_>>();
    assert_eq!(ids(&wide), ids(&narrow));
    assert_eq!(wide.staff_space, 6.0);
    // Every note has ink, and the initial and lyrics are text.
    for n in &wide.notes {
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
    let kinds: Vec<_> = page.pauses.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, [PauseKind::Mediant, PauseKind::Double]);
    let last = page.notes.last().unwrap();
    assert!(page.duration >= last.start + last.duration);
    for n in &page.notes {
        assert_eq!(c.note_at(n.x, n.y), Some(n.id));
    }
    assert_eq!(page.notes[3].half, 1);
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
    assert_eq!(glyph_outline(u16::MAX), None);
}

#[test]
fn bad_options_keep_defaults() {
    let c = Chant::new(
        KYRIE.to_string(),
        ChantOptions {
            initial: 9,
            annotation: false,
            lyric_size: f32::NAN,
            font: LyricFont::Google,
        },
    );
    let mut opts = options();
    opts.scale = f32::INFINITY;
    opts.weights.note = -1.0;
    opts.weights.mora = f32::NAN;
    let page = c.layout(f32::NAN, opts);
    assert_eq!(page.staff_space, 6.0);
    assert!(page.notes.iter().all(|n| n.duration == 1.0));
    let mut huge = default_layout_options();
    huge.weights.note = f32::MAX;
    assert!(c.layout(400.0, huge).notes.iter().all(|n| n.duration == 1000.0));
    assert!(page.width.is_finite() && page.height.is_finite());
}

#[test]
fn diagnostics_carry_byte_spans() {
    let c = chant("(c4) Dó(g)mi(h)nus(Q)");
    let d = c.diagnostics();
    assert!(!d.is_empty());
    assert_eq!(&"(c4) Dó(g)mi(h)nus(Q)"[d[0].start as usize..d[0].end as usize], "Q");
}

#[test]
fn weight_field_defaults_match_the_engine() {
    // The record's `#[uniffi(default = …)]` literals must stay equal to the engine's.
    let w = default_weights();
    assert_eq!(
        [
            w.note, w.mora, w.episema, w.virgula, w.quarter, w.half, w.full, w.double, w.mediant, w.flex
        ],
        [1.0, 2.0, 1.5, 0.5, 0.5, 1.0, 2.0, 3.0, 2.0, 1.0]
    );
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
    assert!(preview.notes.iter().all(|n| n.line == 0));
}

#[test]
fn sets_psalms() {
    let text =
        "1 The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength.";
    let s = psalm(text.to_string(), "8.G".to_string(), Intone::FirstVerse).unwrap();
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let page = chant(&s.gabc).layout(600.0, options());
    assert_eq!(page.notes.len(), s.notes.len());
    assert_eq!(s.notes[0].role, ToneRole::Intonation);
    assert_eq!(s.notes[0].number, Some(1));
    let accent = s.notes.iter().position(|n| n.role == ToneRole::Accent).unwrap();
    assert_eq!(&text[s.notes[accent].start as usize..s.notes[accent].end as usize], "pá");
    assert_eq!(
        psalm(text.to_string(), "9.z".to_string(), Intone::Never),
        Err(ToneError::Unknown { name: "9.z".into() })
    );
    assert!(matches!(
        psalm_with_tone(text.to_string(), "name: x".to_string(), Intone::Never),
        Err(ToneError::Invalid { .. })
    ));
    assert!(tone_names().contains(&"8.G".to_string()));
}
