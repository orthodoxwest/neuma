// neuma: engraved Gregorian chant from GABC, in one self-contained ES module.
//
//   import { init, Chant } from "./neuma.mjs";
//   await init();                         // once; later calls are synchronous
//   const chant = new Chant(gabc, { initial: 1 });
//   const page = chant.layout(600);       // { width, height, svg }, and page.timeline
//   host.innerHTML = page.svg;
//   page.noteAt(x, y);                    // note id under a point, or null
//
// Note ids are stable across layouts of one Chant, so per-note state survives a resize.
// Every note's SVG ink carries its id in `data-note`; select it with `[data-note~="<id>"]`,
// since a porrectus swash lists both notes it draws. Positions are in SVG user units.
//
// Each page answers for itself: its `timeline` (made when first read), and its hit tests,
// `noteAt`, `sourceAt` and `elementsAt`, whatever was laid out since (a thumbnail, say).
// For an editor: `chant.update(gabc)` on each change, `layout(w, { svg: "lines" })` to patch
// only the lines that changed, `page.sourceAt(x, y)` for a click and `page.elementsAt(caret)`
// for the caret. Offsets named `start`/`end` count UTF-8 bytes; those named
// `utf16Start`/`utf16End`, and carets, count UTF-16 units (JavaScript string indices).
// Every position is in output units from the layout's top left, y down; a point that is a
// center is named `cx`, `cy`.

/*__NEUMA_WASM__*/
const WASM_GZIP_BASE64 = "";

let module = null;
let wasm = null;
// Bumped on each new instance, so a Chant made before a restart can't reach a new one.
let generation = 0;
let crashed = null;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

// Errors thrown from inside an engine export, as opposed to by the glue or the caller's own
// arguments (a `toString` that throws, a string too long to build) before the engine ran.
const engineErrors = new WeakSet();

/** The instance's exports, each function wrapped to note the errors it throws. */
function exportsOf(instance) {
  const out = {};
  for (const [name, value] of Object.entries(instance.exports)) {
    out[name] = typeof value !== "function" ? value : (...args) => {
      try {
        return value(...args);
      } catch (e) {
        if (e !== null && typeof e === "object") engineErrors.add(e);
        throw e;
      }
    };
  }
  return out;
}

function instantiate() {
  wasm = exportsOf(new WebAssembly.Instance(module, {}));
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

/**
 * Whether `e` may have stopped the engine partway through: a trap (`unreachable`,
 * out-of-bounds memory), or a stack overflow inside an engine call, which engines report as
 * a `RangeError` ("Maximum call stack size exceeded") rather than a trap. A `RangeError` from
 * the caller's arguments, before the engine ran, leaves it as it was.
 */
const isCrash = (e) =>
  e instanceof WebAssembly.RuntimeError || (e instanceof RangeError && engineErrors.has(e));

/**
 * Runs `f` against the engine. A trap or stack overflow can leave the instance's memory
 * half-updated, so the instance is dropped and `init()` must start a fresh one.
 */
function guarded(f) {
  const w = ready();
  try {
    return f(w);
  } catch (e) {
    if (isCrash(e)) {
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
 * A score's library entry, without engraving it for display: cheap enough to index a
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
 * Sets psalm text (a verse per line, the mediant marked `*`, optionally pointed with `†`, `·`,
 * acutes and `–`) to a psalm tone, and returns the score as GABC for `new Chant(gabc)`.
 * Half-verses with no pointing marks are pointed automatically (see `point`), unless
 * `autoPoint: false`; diagnostics then include `point::unsure` for halves to check.
 * @param {string} text
 * @param {string} tone a built-in tone such as "8.G" (see `toneNames()`), or a whole tone
 *   block (`name:`, `clef:`, `mediant:`, `termination:` lines) for a tone of your own.
 * @param {{ intone?: "first"|"every"|"never", autoPoint?: boolean }} [options]
 * @returns {{ gabc: string, notes: Array<PsalmNote>, diagnostics: Array<object> }}
 *   `notes[i]` describes note `i` of the engraved chant (`timeline.notes[i].id === i`):
 *   `{ verse, number, part: "flex"|"mediant"|"termination",
 *   role: "intonation"|"tenor"|"preparatory"|"accent"|"ending", sourceStart, sourceEnd,
 *   sourceUtf16Start, sourceUtf16End }`, its source the sung syllable's in `text`, named as
 *   the timeline names a note's. The diagnostics' offsets are in `text` too. To engrave the
 *   psalm with its spans in `text`, use `Chant.fromPsalm`.
 */
export function psalm(text, tone, { intone = "first", autoPoint = true } = {}) {
  return guarded((w) => {
    putInput(String(tone) + "\0" + String(text));
    w.neuma_psalm(isBlock(tone), intone === "every" ? 1 : intone === "never" ? 2 : 0, autoPoint ? 1 : 0);
    const out = JSON.parse(takeOutput());
    if (out.error) throw new Error(out.error);
    return out;
  });
}

/**
 * Points psalm text for a tone: marks each half-verse's accents and cadence start, keeping
 * halves that already carry marks. `confidence` is the model's probability (0 to 1) for each
 * half it pointed; below about 0.8 a half is worth checking.
 * @param {string} text a verse per line, the mediant marked `*`
 * @param {string} tone as for `psalm`
 * @returns {{ text: string, halves: Array<{ verse: number, part: "mediant"|"termination",
 *   confidence: number, kept: boolean, sourceStart: number, sourceEnd: number,
 *   sourceUtf16Start: number, sourceUtf16End: number }>, diagnostics: Array<object> }}
 *   Each half's source runs from its first sung syllable to its last in `text`.
 */
export function point(text, tone) {
  return guarded((w) => {
    putInput(String(tone) + "\0" + String(text));
    w.neuma_point(isBlock(tone));
    const out = JSON.parse(takeOutput());
    if (out.error) throw new Error(out.error);
    return out;
  });
}

// A tone block always has `key: value` lines; a tone name never has a colon.
const isBlock = (tone) => (String(tone).includes(":") ? 1 : 0);

/** The built-in psalm tones' names, such as "8.G". Call after `init()`. */
export function toneNames() {
  return guarded((w) => {
    w.neuma_tone_names();
    return takeOutput().split("\n");
  });
}

/**
 * The note sounding at time `t` (in weight units) of a timeline: null during a pause, before
 * the first note and after the last. For a playhead that follows audio; a binary search.
 * @param {{ notes: Array<{ start: number, duration: number }> }} timeline
 * @param {number} t
 */
export function noteAtTime(timeline, t) {
  const notes = timeline.notes;
  let lo = 0;
  let hi = notes.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (notes[mid].start <= t) lo = mid + 1;
    else hi = mid;
  }
  const note = lo === 0 ? null : notes[lo - 1];
  return note && t < note.start + note.duration ? note : null;
}

/** The engine's chant options from the glue's. */
function chantArgs({ initial = 1, annotation = true, lyricSize = 2.45, font = "google" } = {}) {
  return [Math.min(Math.max(Math.trunc(Number(initial)) || 0, 0), 255), annotation ? 1 : 0, lyricSize, font === "eb-garamond-12" ? 1 : 0];
}

/** The diagnostics, then for a chant set from a psalm its `{ gabc, notes }`. */
function diagnosticsAndPsalm(out) {
  const nul = out.indexOf("\0");
  return nul < 0 ? [JSON.parse(out), undefined] : [JSON.parse(out.slice(0, nul)), JSON.parse(out.slice(nul + 1))];
}

export class Chant {
  #handle;
  #generation;
  #diagnostics;
  #summary;
  #psalm;
  /** Bumped on each change, so a page laid out before it can tell. */
  #version = 0;
  /** The last page and the arguments it was laid out with. */
  #last = null;
  /** Each line's SVG from the last layout in parts, which the engine refers to for lines it kept. */
  #lines = [];

  /**
   * Engraves `gabc` once.
   * @param {string} gabc
   * @param {{ initial?: number, annotation?: boolean, lyricSize?: number, font?: string }} [options]
   *   initial: drop-cap height in staves (0 for none, default 1). annotation: show the
   *   annotation or mode above it (default true). lyricSize: in staff spaces (default 2.45).
   *   font: which EB Garamond the page loads, "google" (Google Fonts, default) or
   *   "eb-garamond-12" (the EB Garamond 12 files), so lyrics are spaced for it.
   */
  constructor(gabc, options = {}) {
    if (gabc === PSALM) return;
    const args = chantArgs(options);
    this.#handle = guarded((w) => {
      putInput(String(gabc));
      return w.chant_new(...args);
    });
    this.#generation = generation;
    this.#diagnostics = JSON.parse(takeOutput());
  }

  /**
   * Sets psalm text to a tone, as `psalm` does, and engraves it with its spans in `text`:
   * the timeline's and hit tests' sources, and the diagnostics' offsets, are in `text`.
   * `update(text)` sets the new text to the same tone. `psalm` holds the setting's
   * `{ gabc, notes }`.
   * @param {string} text
   * @param {string} tone as for `psalm`
   * @param {{ intone?: "first"|"every"|"never", autoPoint?: boolean, initial?: number,
   *   annotation?: boolean, lyricSize?: number, font?: string }} [options] `psalm`'s options
   *   and the constructor's.
   */
  static fromPsalm(text, tone, { intone = "first", autoPoint = true, ...options } = {}) {
    const chant = new Chant(PSALM);
    const args = chantArgs(options);
    const handle = guarded((w) => {
      putInput(String(tone) + "\0" + String(text));
      return w.chant_from_psalm(isBlock(tone), intone === "every" ? 1 : intone === "never" ? 2 : 0, autoPoint ? 1 : 0, ...args);
    });
    const out = takeOutput();
    if (handle >>> 0 === 0xffffffff) throw new Error(JSON.parse(out).error);
    chant.#handle = handle;
    chant.#generation = generation;
    [chant.#diagnostics, chant.#psalm] = diagnosticsAndPsalm(out);
    return chant;
  }

  #live() {
    if (this.#handle === undefined) throw new Error("neuma: this Chant was freed");
    if (this.#generation !== generation) throw new Error("neuma: this Chant belongs to an engine that stopped");
    return this.#handle;
  }

  /**
   * Problems found while reading the score: `{ severity, start, end, utf16Start, utf16End,
   * code, message, fix }`. `start`/`end` count UTF-8 bytes of the source and
   * `utf16Start`/`utf16End` UTF-16 units (string indices). `fix` is null, or an edit that
   * fixes the problem: `{ start, end, utf16Start, utf16End, replacement, title }` (replace
   * `utf16Start`..`utf16End` with `replacement`).
   */
  get diagnostics() {
    return this.#diagnostics;
  }

  /** For a chant made with `fromPsalm`, the setting's `{ gabc, notes }` (see `psalm`). */
  get psalm() {
    return this.#psalm;
  }

  #changed(out) {
    [this.#diagnostics, this.#psalm] = diagnosticsAndPsalm(out);
    this.#summary = undefined;
    this.#version += 1;
    this.#last = null;
  }

  /**
   * Replaces the score with `src` (GABC, or psalm text for a chant made with `fromPsalm`),
   * keeping this Chant's options, as an editor does on each change. Lay it out again to see
   * it; `diagnostics`, `summary` and `psalm` follow the new source.
   * @param {string} src
   */
  update(src) {
    const handle = this.#live();
    const out = guarded((w) => {
      putInput(String(src));
      if (!w.chant_update(handle)) throw new Error("neuma: this Chant was freed");
      return takeOutput();
    });
    this.#changed(out);
  }

  /**
   * Engraves the score again with new options (those the constructor takes), as when the
   * reader changes the lyric size. Options that engrave the same change nothing.
   */
  setOptions(options = {}) {
    const handle = this.#live();
    const args = chantArgs(options);
    const out = guarded((w) => {
      if (!w.chant_set_options(handle, ...args)) throw new Error("neuma: this Chant was freed");
      return takeOutput();
    });
    this.#changed(out);
  }

  /** The score's library entry, as `summarize` returns it. */
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
   * Lays the score out at `width` output units (staff spaces times `scale`). Asked again
   * with the same arguments and no change between, it returns the same page.
   * @param {number} width
   * @param {{ scale?: number, lastLine?: "ragged"|"justified", maxLines?: number, weights?: object, prefix?: string,
   *   svg?: "whole"|"lines", ids?: boolean }} [options]
   *   scale: output units per staff space (default 6; any other value that isn't a
   *   positive number is 6 too). maxLines: keep only the first lines, as
   *   broken for the whole score, for a preview such as an incipit (default 0, all).
   *   weights: the page's timeline's, any of DEFAULT_WEIGHTS's keys; a missing, null or
   *   non-numeric value keeps the default. prefix: class and id prefix for the SVG
   *   (default "neuma"). svg: "lines" gives `svgParts` instead of `svg` (see the README), to
   *   patch a page line by line. ids: false leaves out `data-note` and `data-syllable`, so a
   *   line's SVG doesn't change when notes are added or removed above it.
   * @returns {Page}
   */
  layout(width, { scale = 6, lastLine = "ragged", maxLines = 0, weights = {}, prefix = "", svg = "whole", ids = true } = {}) {
    const handle = this.#live();
    const values = WEIGHTS.map((k) => {
      const v = weights?.[k];
      return typeof v === "number" ? v : NaN;
    });
    const at = [Number(width), Number(scale), lastLine === "justified" ? 1 : 0, maxLines >>> 0];
    const flags = (svg === "lines" ? 2 | 8 : 0) | (ids ? 0 : 4);
    const key = JSON.stringify([at, values, String(prefix), flags]);
    if (this.#last?.key === key) return this.#last.page;
    const out = guarded((w) => {
      putInput(String(prefix));
      if (!w.chant_layout(handle, ...at, flags)) throw new Error("neuma: this Chant was freed");
      return takeOutput();
    });
    const nul = out.indexOf("\0");
    const { width: w, height: h } = JSON.parse(out.slice(0, nul));
    const version = this.#version;
    const page = new Page(PAGE, w, h, (op, ...args) => this.#ask(version, at, values, op, ...args));
    if (svg === "lines") {
      const [head, defs, rest, ...tail] = out.slice(nul + 1).split("\0");
      const lines = [];
      // A line the last layout had comes as \u0001 and its index there, so its SVG isn't
      // copied out of the engine and decoded again.
      for (let i = 0; i + 1 < tail.length; i += 2) {
        const s = tail[i + 1];
        lines.push({ top: Number(tail[i]), svg: s.charCodeAt(0) === 1 ? this.#lines[Number(s.slice(1))] : s });
      }
      this.#lines = lines.map((l) => l.svg);
      page.svgParts = { head, defs, rest, lines };
    } else {
      page.svg = out.slice(nul + 1);
    }
    this.#last = { key, page };
    return page;
  }

  /** Asks the engine about a page laid out with `at` at `version`. */
  #ask(version, at, values, op, ...args) {
    const handle = this.#live();
    if (version !== this.#version) {
      throw new Error("neuma: this page was laid out before the chant changed; lay it out again");
    }
    return guarded((w) => {
      switch (op) {
        case "timeline":
          w.chant_timeline(handle, ...at, ...values);
          return JSON.parse(takeOutput());
        case "noteAt": {
          const id = w.chant_note_at(handle, ...at, ...args);
          return id < 0 ? null : id;
        }
        case "sourceAt":
          w.chant_source_at(handle, ...at, ...args);
          return JSON.parse(takeOutput());
        default:
          w.chant_elements_at(handle, ...at, ...args);
          return JSON.parse(takeOutput());
      }
    });
  }

  /** Releases the engraving. */
  free() {
    if (this.#handle !== undefined && this.#generation === generation && wasm) wasm.chant_free(this.#handle);
    this.#handle = undefined;
  }
}

const PSALM = Symbol("psalm");
const PAGE = Symbol("page");

/**
 * A layout of a Chant: `width`, `height`, and `svg` or `svgParts`. Its timeline and hit
 * tests answer for this page, whatever the chant has laid out since; after the chant
 * changes (`update`, `setOptions`) they throw, and the page should be laid out again.
 */
export class Page {
  #ask;
  #timeline;

  /** Pages come from `Chant.layout`. */
  constructor(token, width, height, ask) {
    if (token !== PAGE) throw new TypeError("neuma: pages come from Chant.layout");
    this.width = width;
    this.height = height;
    this.#ask = ask;
  }

  /**
   * The playback timeline, made when first read: `{ notes, pauses, lines, duration }`, with
   * times in weight units. Each note is `{ id, cx, cy, w, h, start, duration, … }`, `cx`,
   * `cy` its notehead's center; each pause `{ beforeNote, kind, start, duration }`.
   */
  get timeline() {
    if (this.#timeline === undefined) this.#timeline = this.#ask("timeline");
    return this.#timeline;
  }

  /** The id of the note under (`x`, `y`), or the nearest on that line; null off the lines. */
  noteAt(x, y) {
    return this.#ask("noteAt", x, y);
  }

  /**
   * The note, bar or syllable under (`x`, `y`): a notehead, else a bar, else a syllable's
   * box, else the nearest syllable on that line; null outside the lines.
   * @returns {{ kind: "note"|"bar"|"syllable", index: number, start: number, end: number,
   *   utf16Start: number, utf16End: number, line: number, x: number, y: number, w: number,
   *   h: number, cx: number } | null}
   *   `utf16Start`..`utf16End` is the source to select (string indices); `x`, `y`, `w`, `h`
   *   the box drawn, from its top left, and `cx` a note's notehead center.
   */
  sourceAt(x, y) {
    return this.#ask("sourceAt", x, y);
  }

  /**
   * What to highlight for a caret in the source: the notes and the bar whose source holds
   * it, then each box of its syllable (one per line it spans), most specific first. A caret
   * just after a note, as after typing it, counts as on it.
   * @param {number} caret a string index (UTF-16 units), such as `textarea.selectionStart`
   * @param {{ unit?: "utf16"|"utf8" }} [options] `unit: "utf8"` takes a byte offset instead.
   * @returns {Array<object>} elements as `sourceAt` returns them
   */
  elementsAt(caret, { unit = "utf16" } = {}) {
    // Past either end means at it: saturate to a u32 (the engine clamps to the source's
    // length) rather than let `>>>` wrap, and read NaN as 0.
    const at = Math.min(Math.max(Math.trunc(Number(caret)) || 0, 0), 0xffffffff);
    return this.#ask("elementsAt", at, unit === "utf8" ? 0 : 1);
  }
}
