// Checks neuma.d.mts against how the README uses the package: `tsc -p crates/neuma-wasm/js`.
// Nothing here runs.
import { Chant, engineStats, init, psalm, setLayoutBudget, type NeumaError, type SvgParts } from "./neuma.mjs";
import * as neuma from "./neuma.mjs";

await init();
const chant = new Chant("(c4) a(g)", { initial: null, font: "eb-garamond-12" });
const whole: string = chant.layout(300).svg;
const parts: SvgParts = chant.layout(300, { svg: "lines", ids: false }).svgParts;
const view = chant.view({ svg: "lines", ids: false });
const page = view.layout(400, { weights: { note: 2, mora: null } });
const top: number = page.svgParts.lines[0].top;
const hit = page.sourceAt(10, 10);
if (hit) console.log(hit.utf16Start, hit.kind === "note");
console.log(whole, parts, top, chant.view().layout(300).svg.length, page.elementsAt(3, { unit: "utf8" }));
console.log(psalm("a * b", "8.G", { intone: null, accents: "outsideFlex" }).notes[0]?.role);
// `svg` chosen at run time.
const mode = (globalThis as { lines?: boolean }).lines ? "lines" : "whole";
const either = chant.layout(300, { svg: mode });
console.log(either.svgParts ? either.svgParts.lines.length : either.svg.length);
const anyView = chant.view({ svg: mode, ids: false });
const shownNow = anyView.layout(300);
console.log(shownNow.svgParts?.head ?? shownNow.svg);
setLayoutBudget({ current: 10 });
setLayoutBudget();
setLayoutBudget(5);
console.log(engineStats().budget.stale, chant.version > page.version, page.timeline().notes[0]?.cx);
const bar = page.timeline().pauses[0]?.bar;
if (bar) console.log(bar.line, bar.right - bar.left, bar.index);
try {
  Chant.fromTone("9.z");
} catch (e) {
  console.log((e as NeumaError).code === "tone");
}
{
  using shown = chant.layout(200);
  shown.noteAt(1, 2);
}
// @ts-expect-error a misspelled option
new Chant("(c4) a(g)", { intial: 1 });
// @ts-expect-error a value not among those taken
chant.layout(300, { svg: "parts" });
// @ts-expect-error a page in parts has no `svg`
chant.layout(300, { svg: "lines" }).svg.length;
// @ts-expect-error pages come from a chant
new neuma.Page();
