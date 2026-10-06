// neuma: engraved Gregorian chant from GABC, in one self-contained ES module.
//
//   import { init, Chant } from "./neuma.mjs";
//   await init();                         // once; later calls are synchronous
//   const chant = new Chant(gabc, { initial: 1 });
//   const page = chant.layout(600);       // { width, height, svg, timeline }
//   host.innerHTML = page.svg;
//   chant.noteAt(x, y);                   // note id under a point, or null
//
// Note ids are stable across layouts of one Chant, so per-note state survives a resize.
// Every note's SVG elements carry `data-note="<id>"`. Positions are in SVG user units.

/*__NEUMA_WASM__*/
const WASM_GZIP_BASE64 = "";

let wasm = null;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function instantiate(bytes) {
  const module = new WebAssembly.Module(bytes);
  wasm = new WebAssembly.Instance(module, {}).exports;
}

/** Initializes synchronously from the raw (uncompressed) `.wasm` bytes. */
export function initSync(bytes) {
  if (!wasm) instantiate(bytes);
}

/** Initializes from the module's inlined copy, or from `bytes` if given. */
export async function init(bytes) {
  if (wasm) return;
  if (bytes) return initSync(bytes);
  const packed = Uint8Array.from(atob(WASM_GZIP_BASE64), (c) => c.charCodeAt(0));
  const stream = new Blob([packed]).stream().pipeThrough(new DecompressionStream("gzip"));
  initSync(new Uint8Array(await new Response(stream).arrayBuffer()));
}

function ready() {
  if (!wasm) throw new Error("neuma: call init() first");
  return wasm;
}

function putInput(text) {
  const w = ready();
  const bytes = encoder.encode(text);
  const ptr = w.neuma_input(bytes.length);
  new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
}

function takeOutput() {
  const w = ready();
  const ptr = w.neuma_output_ptr();
  const len = w.neuma_output_len();
  return decoder.decode(new Uint8Array(w.memory.buffer, ptr, len));
}

/** Weight names, in the order the engine takes them. */
const WEIGHTS = ["note", "mora", "episema", "virgula", "quarter", "half", "full", "double", "mediant", "flex"];

/** The default relative durations: one pulse a note, two for a dotted note. */
export const DEFAULT_WEIGHTS = Object.freeze({
  note: 1, mora: 2, episema: 1.5, virgula: 0.5, quarter: 0.5, half: 1, full: 2, double: 3, mediant: 2, flex: 1,
});

export class Chant {
  #handle;
  #diagnostics;

  /**
   * Engraves `gabc` once.
   * @param {string} gabc
   * @param {{ initial?: number, annotation?: boolean, lyricSize?: number, font?: string }} [options]
   *   initial: drop-cap height in staves (0 for none, default 1). annotation: show the
   *   annotation or mode above it (default true). lyricSize: in staff spaces (default 2.7).
   *   font: which EB Garamond the page loads, "google" (Google Fonts, default) or
   *   "eb-garamond-12" (the EB Garamond 12 files), so lyrics are spaced for it.
   */
  constructor(gabc, { initial = 1, annotation = true, lyricSize = 2.7, font = "google" } = {}) {
    putInput(String(gabc));
    this.#handle = ready().chant_new(initial >>> 0, annotation ? 1 : 0, lyricSize, font === "eb-garamond-12" ? 1 : 0);
    this.#diagnostics = JSON.parse(takeOutput());
  }

  /** Problems found while reading the score: `{ severity, start, end, code, message }`. */
  get diagnostics() {
    return this.#diagnostics;
  }

  /**
   * Lays the score out at `width` SVG units.
   * @param {number} width
   * @param {{ scale?: number, lastLine?: "ragged"|"justified", weights?: object, prefix?: string }} [options]
   *   scale: units per staff space (default 6). weights: any of DEFAULT_WEIGHTS's keys.
   *   prefix: class and id prefix for the SVG (default "neuma").
   * @returns {{ width: number, height: number, svg: string, timeline: object }}
   *   timeline: `{ notes, pauses, lines, duration }`, with times in weight units.
   */
  layout(width, { scale = 6, lastLine = "ragged", weights = {}, prefix = "" } = {}) {
    const w = ready();
    putInput(prefix);
    const values = WEIGHTS.map((k) => (k in weights ? Number(weights[k]) : NaN));
    if (!w.chant_layout(this.#handle, width, scale, lastLine === "justified" ? 1 : 0, ...values)) {
      throw new Error("neuma: this Chant was freed");
    }
    const page = JSON.parse(takeOutput());
    w.chant_svg(this.#handle);
    page.svg = takeOutput();
    return page;
  }

  /** The id of the note under (`x`, `y`) in the last layout, or the nearest on that line. */
  noteAt(x, y) {
    const id = ready().chant_note_at(this.#handle, x, y);
    return id < 0 ? null : id;
  }

  /** Releases the engraving. */
  free() {
    if (this.#handle !== undefined) ready().chant_free(this.#handle);
    this.#handle = undefined;
  }
}
