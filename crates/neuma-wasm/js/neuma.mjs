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
// Every note's SVG ink carries its id in `data-note`; select it with `[data-note~="<id>"]`,
// since a porrectus swash lists both notes it draws. Positions are in SVG user units.

/*__NEUMA_WASM__*/
const WASM_GZIP_BASE64 = "";

let module = null;
let wasm = null;
// Bumped on each new instance, so a Chant made before a restart can't reach a new one.
let generation = 0;
let crashed = null;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function instantiate() {
  wasm = new WebAssembly.Instance(module, {}).exports;
  generation += 1;
  crashed = null;
}

/**
 * Initializes synchronously from the raw (uncompressed) `.wasm` bytes. After the engine
 * stops on an internal error, calling it (or `init`) again starts a fresh engine; Chants
 * made before that must be made again.
 */
export function initSync(bytes) {
  if (wasm) return;
  if (!module) module = new WebAssembly.Module(bytes);
  instantiate();
}

/** Initializes from the module's inlined copy, or from `bytes` if given. */
export async function init(bytes) {
  if (wasm) return;
  if (module) return instantiate();
  if (bytes) return initSync(bytes);
  const packed = Uint8Array.from(atob(WASM_GZIP_BASE64), (c) => c.charCodeAt(0));
  const stream = new Blob([packed]).stream().pipeThrough(new DecompressionStream("gzip"));
  initSync(new Uint8Array(await new Response(stream).arrayBuffer()));
}

function ready() {
  if (crashed) throw new Error("neuma: the engine stopped on an internal error; call init() again", { cause: crashed });
  if (!wasm) throw new Error("neuma: call init() first");
  return wasm;
}

/** Runs `f` against the engine. A trap leaves the instance unusable, so it is dropped. */
function guarded(f) {
  const w = ready();
  try {
    return f(w);
  } catch (e) {
    if (e instanceof WebAssembly.RuntimeError) {
      wasm = null;
      crashed = e;
    }
    throw e;
  }
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

/**
 * A score's catalogue entry, without engraving it for display: cheap enough to index a
 * whole library. See the README for the fields.
 * @param {string} gabc
 */
export function summarize(gabc) {
  return guarded((w) => {
    putInput(String(gabc));
    w.neuma_summarize();
    return JSON.parse(takeOutput());
  });
}

/**
 * Sets pointed psalm text (a verse per line, marked with `*`, `†`, `·`, acutes and `–`) to a
 * psalm tone, and returns the score as GABC for `new Chant(gabc)`.
 * @param {string} text
 * @param {string} tone a built-in tone such as "8.G" (see `TONES`), or a whole tone block
 *   (`name:`, `clef:`, `mediant:`, `termination:` lines) for a tone of your own.
 * @param {{ intone?: "first"|"every"|"never" }} [options]
 * @returns {{ gabc: string, notes: Array<{ verse: number, number: number|null,
 *   part: "flex"|"mediant"|"termination", role: "intonation"|"tenor"|"preparatory"|"accent"|"ending",
 *   start: number, end: number }>, diagnostics: Array<object> }}
 *   `notes[i]` describes note `i` of the engraved chant (`timeline.notes[i].id === i`);
 *   `start` and `end` are the sung syllable's UTF-8 bytes in `text`.
 */
export function psalm(text, tone, { intone = "first" } = {}) {
  return guarded((w) => {
    const custom = String(tone).includes("\n") ? 1 : 0;
    putInput(String(tone) + "\0" + String(text));
    w.neuma_psalm(custom, intone === "every" ? 1 : intone === "never" ? 2 : 0);
    const out = JSON.parse(takeOutput());
    if (out.error) throw new Error(out.error);
    return out;
  });
}

/** The built-in psalm tones' names, such as "8.G". Call after `init()`. */
export function tones() {
  return guarded((w) => {
    w.neuma_tones();
    return takeOutput().split("\n");
  });
}

export class Chant {
  #handle;
  #generation;
  #diagnostics;
  #summary;

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
    this.#handle = guarded((w) => {
      putInput(String(gabc));
      return w.chant_new(initial >>> 0, annotation ? 1 : 0, lyricSize, font === "eb-garamond-12" ? 1 : 0);
    });
    this.#generation = generation;
    this.#diagnostics = JSON.parse(takeOutput());
  }

  #live() {
    if (this.#handle === undefined) throw new Error("neuma: this Chant was freed");
    if (this.#generation !== generation) throw new Error("neuma: this Chant belongs to an engine that stopped");
    return this.#handle;
  }

  /** Problems found while reading the score: `{ severity, start, end, code, message }`. */
  get diagnostics() {
    return this.#diagnostics;
  }

  /** The score's catalogue entry, as `summarize` returns it. */
  get summary() {
    if (this.#summary === undefined) {
      const handle = this.#live();
      this.#summary = guarded((w) => {
        w.chant_summary(handle);
        return JSON.parse(takeOutput());
      });
    }
    return this.#summary;
  }

  /**
   * Lays the score out at `width` SVG units.
   * @param {number} width
   * @param {{ scale?: number, lastLine?: "ragged"|"justified", maxLines?: number, weights?: object, prefix?: string }} [options]
   *   scale: units per staff space (default 6). maxLines: keep only the first lines, as
   *   broken for the whole score, for a preview such as an incipit (default 0, all). weights: any of DEFAULT_WEIGHTS's keys; a
   *   missing, null or non-numeric value keeps the default.
   *   prefix: class and id prefix for the SVG (default "neuma").
   * @returns {{ width: number, height: number, svg: string, timeline: object }}
   *   timeline: `{ notes, pauses, lines, duration }`, with times in weight units.
   */
  layout(width, { scale = 6, lastLine = "ragged", maxLines = 0, weights = {}, prefix = "" } = {}) {
    const handle = this.#live();
    if (!(scale > 0 && Number.isFinite(scale))) scale = 6;
    const values = WEIGHTS.map((k) => {
      const v = weights[k];
      return typeof v === "number" ? v : NaN;
    });
    return guarded((w) => {
      putInput(prefix);
      if (!w.chant_layout(handle, width, scale, lastLine === "justified" ? 1 : 0, maxLines >>> 0, ...values)) {
        throw new Error("neuma: this Chant was freed");
      }
      const page = JSON.parse(takeOutput());
      w.chant_svg(handle);
      page.svg = takeOutput();
      return page;
    });
  }

  /** The id of the note under (`x`, `y`) in the last layout, or the nearest on that line. */
  noteAt(x, y) {
    const handle = this.#live();
    const id = guarded((w) => w.chant_note_at(handle, x, y));
    return id < 0 ? null : id;
  }

  /** Releases the engraving. */
  free() {
    if (this.#handle !== undefined && this.#generation === generation && wasm) wasm.chant_free(this.#handle);
    this.#handle = undefined;
  }
}
