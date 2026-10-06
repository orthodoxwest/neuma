"""Calls neuma through its generated Python bindings: the same calls the Swift and Kotlin
bindings make.

Run by test.sh, which builds the library and generates `neuma.py` next to it.
"""

import neuma

KYRIE = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)"

chant = neuma.Chant(KYRIE, neuma.default_chant_options())
assert chant.diagnostics() == [], chant.diagnostics()
assert chant.note_at(0, 0) is None

opts = neuma.LayoutOptions(last_line=neuma.LastLine.RAGGED, weights=neuma.Weights(mediant=3.0))
wide = chant.layout(600.0, opts)
narrow = chant.layout(120.0, opts)
assert len(narrow.lines) > len(wide.lines)
assert [n.id for n in wide.notes] == [n.id for n in narrow.notes]

# Pauses carry kinds and the caller's weights; the timeline is laid end to end.
assert [p.kind for p in narrow.pauses] == [neuma.PauseKind.MEDIANT, neuma.PauseKind.DOUBLE]
assert narrow.pauses[0].weight == 3.0
assert narrow.notes[3].half == 1

# Each note's center hits it, and every glyph has an outline.
for n in narrow.notes:
    assert chant.note_at(n.x, n.y) == n.id
glyphs = {i.glyph for i in narrow.items if isinstance(i, neuma.Item.GLYPH)}
assert glyphs and all(neuma.glyph_outline(g).path.startswith("M") for g in glyphs)
assert neuma.glyph_outline(65535) is None
texts = [i for i in narrow.items if isinstance(i, neuma.Item.TEXT)]
assert any(t.role == neuma.TextRole.INITIAL and t.runs[0].text == "K" for t in texts)

# A porrectus swash names both of its notes.
swash = neuma.Chant("(c4) a(hgh)", neuma.default_chant_options()).layout(400.0, neuma.default_layout_options())
assert any(isinstance(i, neuma.Item.GLYPH) and i.notes == [0, 1] for i in swash.items)

# Catalogue entries and one-line previews.
entry = neuma.summarize("office-part: Introitus;\nmode: VII;\n%%\n(c3) PU(g)er(h) na(h)tus(g) (::)")
assert entry.kind == neuma.OfficePart.INTROIT and entry.mode.number == 7 and entry.text == "Puer natus"
assert chant.summary().incipit == "Kyrie eleison"
preview = chant.layout(120.0, neuma.LayoutOptions(last_line=neuma.LastLine.RAGGED, weights=neuma.Weights(), max_lines=1))
assert len(preview.lines) == 1 and preview.lines[0] == narrow.lines[0]

# Psalm tones.
ps = neuma.psalm("The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength.", "8.G", neuma.Intone.FIRST_VERSE)
assert ps.diagnostics == [] and ps.notes[0].role == neuma.ToneRole.INTONATION
assert len(neuma.Chant(ps.gabc, neuma.default_chant_options()).layout(500.0, neuma.default_layout_options()).notes) == len(ps.notes)
try:
    neuma.psalm("a * b", "9.z", neuma.Intone.NEVER)
    raise AssertionError("unknown tone accepted")
except neuma.ToneError.Unknown:
    pass

print(f"ok: {len(narrow.notes)} notes, {len(narrow.lines)} lines at 120, {len(glyphs)} glyphs")
