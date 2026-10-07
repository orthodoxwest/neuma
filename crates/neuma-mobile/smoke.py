"""Calls neuma through its generated Python bindings: the same calls the Swift and Kotlin
bindings make.

Run by test.sh, which builds the library and generates `neuma.py` next to it.
"""

import neuma

KYRIE = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)"

chant = neuma.Chant(KYRIE, neuma.ChantOptions())
assert chant.diagnostics() == [], chant.diagnostics()

weights = neuma.Weights(mediant=3.0)
wide = chant.layout(600.0, neuma.LayoutOptions())
narrow = chant.layout(120.0, neuma.LayoutOptions())
page = narrow.page()
timeline = narrow.timeline(weights)
assert len(page.lines) > len(wide.page().lines)
assert [n.id for n in wide.timeline(weights).notes] == [n.id for n in timeline.notes]

# Pauses carry kinds and the caller's weights; the timeline is laid end to end.
assert [p.kind for p in timeline.pauses] == [neuma.PauseKind.MEDIANT, neuma.PauseKind.DOUBLE]
assert timeline.pauses[0].duration == 3.0
assert timeline.notes[3].half == 1
assert narrow.note_at_time(timeline.notes[2].start, weights) == timeline.notes[2]

# A one-line preview laid out since leaves each layout's hit tests alone, and every glyph
# has an outline.
preview = chant.layout(120.0, neuma.LayoutOptions(last_line=neuma.LastLine.JUSTIFIED, max_lines=1))
for n in timeline.notes:
    assert narrow.note_at(n.cx, n.cy) == n.id
glyphs = {i.glyph for i in page.items if isinstance(i, neuma.Item.GLYPH)}
assert glyphs and all(neuma.glyph_outline(g).path.startswith("M") for g in glyphs)
assert neuma.glyph_outline(65535) is None
texts = [i for i in page.items if isinstance(i, neuma.Item.TEXT)]
assert any(t.role == neuma.TextRole.INITIAL and t.runs[0].text == "K" for t in texts)

# A porrectus swash names both of its notes.
swash = neuma.Chant("(c4) a(hgh)", neuma.ChantOptions()).layout(400.0, neuma.LayoutOptions()).page()
assert any(isinstance(i, neuma.Item.GLYPH) and i.notes == [0, 1] for i in swash.items)

# Library entries and one-line previews.
entry = neuma.summarize("office-part: Introitus;\nmode: VII;\n%%\n(c3) PU(g)er(h) na(h)tus(g) (::)")
assert entry.kind == neuma.OfficePart.INTROIT and entry.mode.number == 7 and entry.text == "Puer natus"
assert chant.summary().incipit == "Kyrie eleison"
assert len(preview.page().lines) == 1 and preview.page().lines[0] == page.lines[0]

# Psalm tones, set as GABC or engraved from the text.
text = "The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength."
ps = neuma.psalm(text, "8.G", neuma.PsalmOptions())
assert ps.diagnostics == [] and ps.notes[0].role == neuma.ToneRole.INTONATION
accent = next(n for n in ps.notes if n.role == neuma.ToneRole.ACCENT)
utf16 = text.encode("utf-16-le")
assert utf16[2 * accent.source_utf16_start:2 * accent.source_utf16_end].decode("utf-16-le") == "pá"
sung = neuma.Chant.from_psalm(text, "8.G", neuma.PsalmOptions(), neuma.ChantOptions())
assert sung.psalm() == ps and chant.psalm() is None
notes = sung.layout(500.0, neuma.LayoutOptions()).timeline(neuma.Weights()).notes
assert len(notes) == len(ps.notes) and notes[0].source_utf16_start == ps.notes[0].source_utf16_start
try:
    neuma.psalm("a * b", "9.z", neuma.PsalmOptions(intone=neuma.Intone.NEVER))
    raise AssertionError("unknown tone accepted")
except neuma.ToneError.Unknown as e:
    assert "no built-in tone 9.z" in str(e), str(e)
pt = neuma.point("O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.", "8.G")
assert len(pt.halves) == 2 and "·" in pt.text and pt.halves[0].part == neuma.VersePart.MEDIANT

# Editors: fixes, UTF-16 offsets, and links between the source and the score.
src = "(c4) Kŷ-(g)ri(hi) (,) e(h) (::)"
ed = neuma.Chant(src, neuma.ChantOptions(font=neuma.LyricFont.GARAMOND12))
hyphen = next(d for d in ed.diagnostics() if d.code == "gabc::hyphen-in-syllable")
assert hyphen.fix.replacement == "" and hyphen.utf16_start == hyphen.start - 1
layout = ed.layout(500.0, neuma.LayoutOptions())
for n in layout.timeline(neuma.Weights()).notes:
    hit = layout.source_at(n.cx, n.cy)
    assert hit.kind == neuma.ElementKind.NOTE and hit.index == n.id
caret = src.encode("utf-16-le").find("hi".encode("utf-16-le")) // 2 + 1
at = layout.elements_at(caret, neuma.OffsetUnit.UTF16)
assert [e.kind for e in at] == [neuma.ElementKind.NOTE, neuma.ElementKind.SYLLABLE]
assert src.encode()[at[0].start:at[0].end] == b"i"
made = ed.version()
ed.update(src.replace("-(g)", "(g)"))
edited = ed.version()
ed.update(src.replace("-(g)", "(g)"))
assert edited > made and ed.version() == edited
assert all(d.code != "gabc::hyphen-in-syllable" for d in ed.diagnostics())
ed.set_options(neuma.ChantOptions(lyric_size=4.0))
assert ed.version() > edited
assert ed.layout(500.0, neuma.LayoutOptions()).page().height > layout.page().height

print(f"ok: {len(timeline.notes)} notes, {len(page.lines)} lines at 120, {len(glyphs)} glyphs")
