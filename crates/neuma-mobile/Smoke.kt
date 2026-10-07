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

    val weights = Weights(mediant = 3f)
    val wide = chant.layout(600f, LayoutOptions())
    val narrow = chant.layout(120f, LayoutOptions())
    val page = narrow.page()
    val timeline = narrow.timeline(weights)
    val notes = timeline.notes
    check(page.lines.size > wide.page().lines.size, "narrow wraps")
    check(wide.timeline(weights).notes.map { it.id } == notes.map { it.id }, "stable ids")
    check(timeline.pauses.map { it.kind } == listOf(PauseKind.MEDIANT, PauseKind.DOUBLE), "pause kinds")
    check(timeline.pauses[0].duration == 3f, "caller weight")
    check(narrow.noteAtTime(notes[2].start, weights) == notes[2], "playhead")
    // A one-line preview laid out since leaves each layout's hit tests alone.
    val preview = chant.layout(120f, LayoutOptions(lastLine = LastLine.JUSTIFIED, maxLines = 1))
    for (n in notes) check(narrow.noteAt(n.cx, n.cy) == n.id, "hit note ${n.id}")

    val glyphs = page.items.filterIsInstance<Item.Glyph>().map { it.glyph }.toSet()
    check(glyphs.isNotEmpty() && glyphs.all { glyphOutline(it)!!.path.startsWith("M") }, "outlines")
    val initial = page.items.filterIsInstance<Item.Text>().first { it.role == TextRole.INITIAL }
    check(initial.runs[0].text == "K", "initial")

    val swash = Chant("(c4) a(hgh)", ChantOptions()).layout(400f, LayoutOptions()).page()
    check(swash.items.filterIsInstance<Item.Glyph>().any { it.notes == listOf(0, 1) }, "porrectus")
    val entry = summarize("office-part: Introitus;\nmode: VII;\n%%\n(c3) PU(g)er(h) na(h)tus(g) (::)")
    check(entry.kind == OfficePart.INTROIT && entry.mode?.number == 7 && entry.text == "Puer natus", "summary")
    check(preview.page().lines.size == 1 && preview.page().lines[0] == page.lines[0], "preview")
    val text = "The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength."
    val ps = psalm(text, "8.G", PsalmOptions())
    check(ps.diagnostics.isEmpty() && ps.notes[0].role == ToneRole.INTONATION, "psalm")
    val accent = ps.notes.first { it.role == ToneRole.ACCENT }
    check(text.substring(accent.sourceUtf16Start, accent.sourceUtf16End) == "pá", "psalm note text")
    val sung = Chant.fromPsalm(text, "8.G", PsalmOptions(), ChantOptions())
    check(sung.psalm() == ps && chant.psalm() == null, "psalm setting")
    sung.layout(500f, LayoutOptions()).use { check(it.timeline(Weights()).notes.size == ps.notes.size, "psalm timeline") }
    val unknown = try { psalm("a * b", "9.z", PsalmOptions(intone = Intone.NEVER)); null } catch (e: ToneException.Unknown) { e }
    check(unknown?.message == "no built-in tone 9.z", "unknown tone: ${unknown?.message}")
    val pt = point("O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.", "8.G")
    check(pt.halves.size == 2 && pt.text.contains("·"), "point")
    // A pointed psalter's display: the tone once, and verses as styled runs.
    val shown = psalmDisplay("1 Wash me thoróughly · from my wíckedness, † and cleanse me from my sín. * [Sit.] For I ac·knowledge my fáults.", "8.G", PsalmOptions())
    val runs = shown.verses[0].runs
    check(runs.joinToString("") { it.text }.startsWith("Wash me thoróughly ·\u00a0from") && shown.toneLabel == "Tone 8 G" && toneLabel("per") == "Tonus peregrinus", "psalm display")
    check(runs.count { (it.kind as? PsalmRunKind.Syllable)?.syllable?.flexDrop == true } == 2 && runs.any { it.kind is PsalmRunKind.Flex }, "flex")
    val outside = psalmDisplay("Wash me thoróughly · from my wíckedness, † and cleanse me from my sín. * For I ac·knowledge my fáults.", "8.G", PsalmOptions(accents = Accents.OUTSIDE_FLEX))
    check(outside.verses[0].runs.joinToString("") { it.text }.startsWith("Wash me thoroughly ·\u00a0from my wickedness,"), "accents")
    Chant.fromTone("8.G", ChantOptions()).layout(400f, LayoutOptions()).use { check(it.timeline(Weights()).notes.size == 12, "tone") }
    val src = "(c4) Kŷ-(g)ri(hi) (,) e(h) (::)"
    val ed = Chant(src, ChantOptions(font = LyricFont.GARAMOND12))
    val hyphen = ed.diagnostics().first { it.code == "gabc::hyphen-in-syllable" }
    check(src.substring(hyphen.utf16Start, hyphen.utf16End) == "-", "utf16 span")
    check(hyphen.fix?.replacement == "", "fix")
    val layout = ed.layout(500f, LayoutOptions())
    for (n in layout.timeline(Weights()).notes) check(layout.sourceAt(n.cx, n.cy)?.index == n.id, "source of note ${n.id}")
    val at = layout.elementsAt(src.indexOf("hi") + 1, OffsetUnit.UTF16)
    check(at.map { it.kind } == listOf(ElementKind.NOTE, ElementKind.SYLLABLE), "caret")
    check(src.substring(at[1].utf16Start, at[1].utf16End) == "ri(hi)", "syllable span")
    val made = ed.version()
    ed.update(src.replace("-(g)", "(g)"))
    val edited = ed.version()
    ed.update(src.replace("-(g)", "(g)"))
    check(edited > made && ed.version() == edited, "version moves on real changes only")
    check(ed.diagnostics().none { it.code == "gabc::hyphen-in-syllable" }, "update")
    ed.setOptions(ChantOptions(lyricSize = 4f, font = LyricFont.GARAMOND12))
    check(ed.version() > edited, "set options changed")
    check(ed.layout(500f, LayoutOptions()).page().height > layout.page().height, "set options")
    layout.close()
    ed.close()
    chant.close()
    println("ok: kotlin, ${notes.size} notes, ${glyphs.size} glyphs")
}
