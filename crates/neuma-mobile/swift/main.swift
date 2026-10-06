// Calls neuma through its generated Swift bindings, as the iOS app would. test.sh compiles
// it with Neuma.swift into one executable on macOS.

func check(_ ok: Bool, _ what: String) {
    if !ok { fatalError(what) }
}

let kyrie = "mode: 8;\n%%\n(c4) Ky(g)ri(h)e(g) *() e(h)le(g)i(h)son(g) (::)"
let chant = Chant(gabc: kyrie, options: defaultChantOptions())
check(chant.diagnostics().isEmpty, "diagnostics")
check(chant.noteAt(x: 0, y: 0) == nil, "no layout yet")

let opts = LayoutOptions(lastLine: .ragged, weights: Weights(mediant: 3))
let wide = chant.layout(width: 600, options: opts)
let narrow = chant.layout(width: 120, options: opts)
check(narrow.lines.count > wide.lines.count, "narrow wraps")
check(wide.notes.map { $0.id } == narrow.notes.map { $0.id }, "stable ids")
check(narrow.pauses.map { $0.kind } == [.mediant, .double], "pause kinds")
check(narrow.pauses[0].weight == 3, "caller weight")
for n in narrow.notes {
    check(chant.noteAt(x: n.x, y: n.y) == n.id, "hit note \(n.id)")
}

var glyphs = Set<UInt16>()
var initial: String?
for item in narrow.items {
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

let swash = Chant(gabc: "(c4) a(hgh)", options: defaultChantOptions())
    .layout(width: 400, options: defaultLayoutOptions())
check(swash.items.contains { item in
    if case let .glyph(_, _, _, _, _, notes) = item { return notes == [0, 1] }
    return false
}, "porrectus")
print("ok: swift, \(narrow.notes.count) notes, \(glyphs.count) glyphs")
