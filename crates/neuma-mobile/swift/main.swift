// Calls neuma through its generated Swift bindings, as an iOS app would. test.sh compiles
// it with Neuma.swift into one executable on macOS.

import Foundation

func check(_ ok: Bool, _ what: String) {
    if !ok { fatalError(what) }
}

let kyrie = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)"
let chant = Chant(gabc: kyrie, options: ChantOptions())
check(chant.diagnostics().isEmpty, "diagnostics")

let weights = Weights(mediant: 3)
let wide = chant.layout(width: 600, options: LayoutOptions())
let narrow = chant.layout(width: 120, options: LayoutOptions())
let page = narrow.page()
let timeline = narrow.timeline(weights: weights)
let notes = timeline.notes
check(page.lines.count > wide.page().lines.count, "narrow wraps")
check(wide.timeline(weights: weights).notes.map { $0.id } == notes.map { $0.id }, "stable ids")
check(timeline.pauses.map { $0.kind } == [.mediant, .double], "pause kinds")
check(timeline.pauses[0].duration == 3, "caller weight")
check(narrow.noteAtTime(t: notes[2].start, weights: weights) == notes[2], "playhead")
// A one-line preview laid out since leaves each layout's hit tests alone.
let preview = chant.layout(width: 120, options: LayoutOptions(lastLine: .justified, maxLines: 1))
for n in notes {
    check(narrow.noteAt(x: n.cx, y: n.cy) == n.id, "hit note \(n.id)")
}

var glyphs = Set<Int32>()
var initial: String?
for item in page.items {
    switch item {
    case let .glyph(glyph, _, _, _, _, _):
        glyphs.insert(glyph)
    case let .text(_, _, _, runs, role, _):
        if role == .initial { initial = runs.first?.text }
    case .rect:
        break
    }
}
check(!glyphs.isEmpty && glyphs.allSatisfy { glyphOutline(id: $0)?.path.hasPrefix("M") == true }, "outlines")
check(initial == "K", "initial")

let swash = Chant(gabc: "(c4) a(hgh)", options: ChantOptions())
    .layout(width: 400, options: LayoutOptions()).page()
check(swash.items.contains { item in
    if case let .glyph(_, _, _, _, _, notes) = item { return notes == [0, 1] }
    return false
}, "porrectus")
let entry = summarize(gabc: "office-part: Introitus;\nmode: VII;\n%%\n(c3) PU(g)er(h) na(h)tus(g) (::)")
check(entry.kind == .introit && entry.mode?.number == 7 && entry.text == "Puer natus", "summary")
check(preview.page().lines.count == 1 && preview.page().lines[0] == page.lines[0], "preview")
let text = "The Lord is King, and hath put on glorious ap·pá-rel; * the Lord hath put on his apparel, and gird·ed him-sélf with strength."
let ps = try! psalm(text: text, tone: "8.G", options: PsalmOptions())
check(ps.diagnostics.isEmpty && ps.notes[0].role == .intonation, "psalm")
let sung = try! Chant.fromPsalm(text: text, tone: "8.G", psalm: PsalmOptions(), options: ChantOptions())
check(sung.psalm() == ps && chant.psalm() == nil, "psalm setting")
check(sung.layout(width: 500, options: LayoutOptions()).timeline(weights: Weights()).notes.count == ps.notes.count, "psalm timeline")
do {
    _ = try psalm(text: "a * b", tone: "9.z", options: PsalmOptions(intone: .never))
    check(false, "unknown tone")
} catch let e as ToneError {
    check("\(e)".contains("9.z"), "unknown tone message")
} catch {
    check(false, "unknown tone error")
}
let pt = try! point(text: "O come, let us sing unto the Lord * let us heartily rejoice in the strength of our salvation.", tone: "8.G")
check(pt.halves.count == 2 && pt.text.contains("·"), "point")
let src = "(c4) Kŷ-(g)ri(hi) (,) e(h) (::)"
let ed = Chant(gabc: src, options: ChantOptions(font: .garamond12))
let hyphen = ed.diagnostics().first { $0.code == "gabc::hyphen-in-syllable" }!
check(hyphen.fix?.replacement == "" && hyphen.utf16Start == hyphen.start - 1, "fix")
let layout = ed.layout(width: 500, options: LayoutOptions())
for n in layout.timeline(weights: Weights()).notes {
    check(layout.sourceAt(x: n.cx, y: n.cy)?.index == n.id, "source of note \(n.id)")
}
let caret = Int32((src as NSString).range(of: "hi").location + 1)
let at = layout.elementsAt(offset: caret, unit: .utf16)
check(at.map { $0.kind } == [.note, .syllable], "caret")
let made = ed.version()
ed.update(src: src.replacingOccurrences(of: "-(g)", with: "(g)"))
let edited = ed.version()
ed.update(src: src.replacingOccurrences(of: "-(g)", with: "(g)")) // the same text: no change
check(edited > made && ed.version() == edited, "version moves on real changes only")
check(!ed.diagnostics().contains { $0.code == "gabc::hyphen-in-syllable" }, "update")
ed.setOptions(options: ChantOptions(lyricSize: 4))
check(ed.version() > edited, "set options changed")
check(ed.layout(width: 500, options: LayoutOptions()).page().height > layout.page().height, "set options")
print("ok: swift, \(notes.count) notes, \(glyphs.count) glyphs")
