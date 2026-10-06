// Calls neuma through its generated Kotlin bindings on the JVM, as the Android app would.
// test.sh compiles and runs it when NEUMA_KOTLINC and NEUMA_JNA are set.

import org.orthodoxwest.neuma.*

fun check(ok: Boolean, what: String) {
    if (!ok) throw AssertionError(what)
}

fun main() {
    val kyrie = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)"
    val chant = Chant(kyrie, defaultChantOptions())
    check(chant.diagnostics().isEmpty(), "diagnostics")
    check(chant.noteAt(0f, 0f) == null, "no layout yet")

    val opts = LayoutOptions(lastLine = LastLine.RAGGED, weights = Weights(mediant = 3f))
    val wide = chant.layout(600f, opts)
    val narrow = chant.layout(120f, opts)
    check(narrow.lines.size > wide.lines.size, "narrow wraps")
    check(wide.notes.map { it.id } == narrow.notes.map { it.id }, "stable ids")
    check(narrow.pauses.map { it.kind } == listOf(PauseKind.MEDIANT, PauseKind.DOUBLE), "pause kinds")
    check(narrow.pauses[0].weight == 3f, "caller weight")
    for (n in narrow.notes) check(chant.noteAt(n.x, n.y) == n.id, "hit note ${n.id}")

    val glyphs = narrow.items.filterIsInstance<Item.Glyph>().map { it.glyph }.toSet()
    check(glyphs.isNotEmpty() && glyphs.all { glyphOutline(it)!!.path.startsWith("M") }, "outlines")
    val initial = narrow.items.filterIsInstance<Item.Text>().first { it.role == TextRole.INITIAL }
    check(initial.runs[0].text == "K", "initial")

    val swash = Chant("(c4) a(hgh)", defaultChantOptions()).layout(400f, defaultLayoutOptions())
    check(swash.items.filterIsInstance<Item.Glyph>().any { it.notes == listOf(0u, 1u) }, "porrectus")
    chant.close()
    println("ok: kotlin, ${narrow.notes.size} notes, ${glyphs.size} glyphs")
}
