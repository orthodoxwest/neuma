"""Calls neuma through its generated Python bindings: the same calls the Swift and Kotlin
bindings make.

Run by test.sh, which builds the library and generates `neuma.py` next to it.
"""

import neuma

KYRIE = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)"

chant = neuma.Chant(KYRIE, neuma.ChantOptions())
assert chant.diagnostics() == [], chant.diagnostics()
assert chant.note_at(0, 0) is None

opts = neuma.LayoutOptions(weights=neuma.Weights(mediant=3.0))
wide = chant.layout(600.0, opts)
narrow = chant.layout(120.0, opts)
assert len(narrow.lines) > len(wide.lines)
assert [n.id for n in wide.timeline.notes] == [n.id for n in narrow.timeline.notes]

# Pauses carry kinds and the caller's weights; the timeline is laid end to end.
assert [p.kind for p in narrow.timeline.pauses] == [neuma.PauseKind.MEDIANT, neuma.PauseKind.DOUBLE]
assert narrow.timeline.pauses[0].weight == 3.0
assert narrow.timeline.notes[3].half == 1

# Each note's center hits it, and every glyph has an outline.
for n in narrow.timeline.notes:
    assert chant.note_at(n.cx, n.cy) == n.id
glyphs = {i.glyph for i in narrow.items if isinstance(i, neuma.Item.GLYPH)}
assert glyphs and all(neuma.glyph_outline(g).path.startswith("M") for g in glyphs)
assert neuma.glyph_outline(65535) is None
texts = [i for i in narrow.items if isinstance(i, neuma.Item.TEXT)]
assert any(t.role == neuma.TextRole.INITIAL and t.runs[0].text == "K" for t in texts)

# A porrectus swash names both of its notes.
swash = neuma.Chant("(c4) a(hgh)", neuma.ChantOptions()).layout(400.0, neuma.LayoutOptions(timeline=False))
assert swash.timeline is None
assert any(isinstance(i, neuma.Item.GLYPH) and i.notes == [0, 1] for i in swash.items)

# Library entries and one-line previews.
entry = neuma.summarize("office-part: Introitus;\nmode: VII;\n%%\n(c3) PU(g)er(h) na(h)tus(g) (::)")
assert entry.kind == neuma.OfficePart.INTROIT and entry.mode.number == 7 and entry.text == "Puer natus"
assert chant.summary().incipit == "Kyrie eleison"
preview = chant.layout(120.0, neuma.LayoutOptions(last_line=neuma.LastLine.JUSTIFIED, max_lines=1))
assert len(preview.lines) == 1 and preview.lines[0] == narrow.lines[0]

# Psalm tones.
text = "The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength."
ps = neuma.psalm(text, "8.G", neuma.PsalmOptions())
assert ps.diagnostics == [] and ps.notes[0].role == neuma.ToneRole.INTONATION
assert len(neuma.Chant(ps.gabc, neuma.ChantOptions()).layout(500.0, neuma.LayoutOptions()).timeline.notes) == len(ps.notes)
accent = next(n for n in ps.notes if n.role == neuma.ToneRole.ACCENT)
assert text.encode("utf-16-le")[2 * accent.utf16_start:2 * accent.utf16_end].decode("utf-16-le") == "pá"
try:
    neuma.psalm("a * b", "9.z", neuma.PsalmOptions(intone=neuma.Intone.NEVER))
    raise AssertionError("unknown tone accepted")
except neuma.ToneError.Unknown:
    pass
pt = neuma.point("O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.", "8.G")
assert len(pt.halves) == 2 and "·" in pt.text and pt.halves[0].part == neuma.VersePart.MEDIANT

# Editors: fixes, UTF-16 offsets, and links between the source and the score.
src = "(c4) Kŷ-(g)ri(hi) (,) e(h) (::)"
ed = neuma.Chant(src, neuma.ChantOptions(font=neuma.LyricFont.GARAMOND12))
hyphen = next(d for d in ed.diagnostics() if d.code == "gabc::hyphen-in-syllable")
assert hyphen.fix.replacement == "" and hyphen.utf16_start == hyphen.start - 1
page = ed.layout(500.0, neuma.LayoutOptions())
for n in page.timeline.notes:
    hit = ed.source_at(n.cx, n.cy)
    assert hit.kind == neuma.ElementKind.NOTE and hit.index == n.id
caret = src.encode("utf-16-le").find("hi".encode("utf-16-le")) // 2 + 1
at = ed.elements_at(caret, neuma.OffsetUnit.UTF16)
assert [e.kind for e in at] == [neuma.ElementKind.NOTE, neuma.ElementKind.SYLLABLE]
assert src.encode()[at[0].start:at[0].end] == b"i"
ed.update(src.replace("-(g)", "(g)"))
assert all(d.code != "gabc::hyphen-in-syllable" for d in ed.diagnostics())

print(f"ok: {len(narrow.timeline.notes)} notes, {len(narrow.lines)} lines at 120, {len(glyphs)} glyphs")
