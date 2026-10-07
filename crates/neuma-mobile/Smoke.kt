// Calls neuma through its generated Kotlin bindings on the JVM, as an Android app would.
// test.sh compiles and runs it when NEUMA_KOTLINC and NEUMA_JNA are set.

import org.orthodoxwest.neuma.*

fun check(ok: Boolean, what: String) {
    if (!ok) throw AssertionError(what)
}

fun main() {
    val kyrie = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)"
    val chant = Chant(kyrie, ChantOptions())
    check(chant.diagnostics().isEmpty(), "diagnostics")
    check(chant.noteAt(0f, 0f) == null, "no layout yet")

    val opts = LayoutOptions(weights = Weights(mediant = 3f))
    val wide = chant.layout(600f, opts)
    val narrow = chant.layout(120f, opts)
    val notes = narrow.timeline!!.notes
    check(narrow.lines.size > wide.lines.size, "narrow wraps")
    check(wide.timeline!!.notes.map { it.id } == notes.map { it.id }, "stable ids")
    check(narrow.timeline!!.pauses.map { it.kind } == listOf(PauseKind.MEDIANT, PauseKind.DOUBLE), "pause kinds")
    check(narrow.timeline!!.pauses[0].weight == 3f, "caller weight")
    for (n in notes) check(chant.noteAt(n.cx, n.cy) == n.id, "hit note ${n.id}")

    val glyphs = narrow.items.filterIsInstance<Item.Glyph>().map { it.glyph }.toSet()
    check(glyphs.isNotEmpty() && glyphs.all { glyphOutline(it)!!.path.startsWith("M") }, "outlines")
    val initial = narrow.items.filterIsInstance<Item.Text>().first { it.role == TextRole.INITIAL }
    check(initial.runs[0].text == "K", "initial")

    val swash = Chant("(c4) a(hgh)", ChantOptions()).layout(400f, LayoutOptions(timeline = false))
    check(swash.timeline == null, "no timeline")
    check(swash.items.filterIsInstance<Item.Glyph>().any { it.notes == listOf(0, 1) }, "porrectus")
    val entry = summarize("office-part: Introitus;\nmode: VII;\n%%\n(c3) PU(g)er(h) na(h)tus(g) (::)")
    check(entry.kind == OfficePart.INTROIT && entry.mode?.number == 7 && entry.text == "Puer natus", "summary")
    val preview = chant.layout(120f, LayoutOptions(lastLine = LastLine.JUSTIFIED, maxLines = 1))
    check(preview.lines.size == 1 && preview.lines[0] == narrow.lines[0], "preview")
    val text = "The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength."
    val ps = psalm(text, "8.G", PsalmOptions())
    check(ps.diagnostics.isEmpty() && ps.notes[0].role == ToneRole.INTONATION, "psalm")
    check(Chant(ps.gabc, ChantOptions()).layout(500f, LayoutOptions()).timeline!!.notes.size == ps.notes.size, "psalm notes")
    val accent = ps.notes.first { it.role == ToneRole.ACCENT }
    check(text.substring(accent.utf16Start, accent.utf16End) == "pá", "psalm note text")
    check(try { psalm("a * b", "9.z", PsalmOptions(intone = Intone.NEVER)); false } catch (e: ToneException.Unknown) { true }, "unknown tone")
    val pt = point("O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.", "8.G")
    check(pt.halves.size == 2 && pt.text.contains("·"), "point")
    val src = "(c4) Kŷ-(g)ri(hi) (,) e(h) (::)"
    val ed = Chant(src, ChantOptions(font = LyricFont.GARAMOND12))
    val hyphen = ed.diagnostics().first { it.code == "gabc::hyphen-in-syllable" }
    check(src.substring(hyphen.utf16Start, hyphen.utf16End) == "-", "utf16 span")
    check(hyphen.fix?.replacement == "", "fix")
    val edPage = ed.layout(500f, LayoutOptions())
    for (n in edPage.timeline!!.notes) check(ed.sourceAt(n.cx, n.cy)?.index == n.id, "source of note ${n.id}")
    val at = ed.elementsAt(src.indexOf("hi") + 1, OffsetUnit.UTF16)
    check(at.map { it.kind } == listOf(ElementKind.NOTE, ElementKind.SYLLABLE), "caret")
    check(src.substring(at[1].utf16Start, at[1].utf16End) == "ri(hi)", "syllable span")
    ed.update(src.replace("-(g)", "(g)"))
    check(ed.diagnostics().none { it.code == "gabc::hyphen-in-syllable" }, "update")
    ed.close()
    chant.close()
    println("ok: kotlin, ${notes.size} notes, ${glyphs.size} glyphs")
}
