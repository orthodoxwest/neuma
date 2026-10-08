// neuma: engraved Gregorian chant from GABC, in one self-contained ES module.
//
//   import { init, Chant } from "./neuma.mjs";
//   await init();                         // once; later calls are synchronous
//   const chant = new Chant(gabc, { initial: 1 });
//   const page = chant.layout(600);       // { width, height, svg }, and page.timeline()
//   host.innerHTML = page.svg;
//   page.noteAt(x, y);                    // note id under a point, or null
//
// Note ids are stable across layouts of one Chant, so per-note state survives a resize.
// Every note's SVG ink carries its id in `data-note`; select it with `[data-note~="<id>"]`,
// since a porrectus swash lists both notes it draws. Positions are in SVG user units.
//
// Each page answers for the score it shows for as long as it is held: its `timeline()` (made
// on the first call) and its hit tests, `noteAt`, `sourceAt` and `elementsAt`, whatever the
// chant has laid out or become since; `page.stale` says the chant has changed. The engine
// keeps only the most recently used layouts (`setLayoutBudget`); a page whose layout was
// dropped lays itself out again when asked, to the same answers, so no page ever throws for
// its age. A place that shows the score and lays it out again on each change (an editor's
// preview) does it through a view: `const view = chant.view({ svg: "lines", ids: false })`,
// then `view.layout(width)` on each change, which patches only the lines that changed. Then
// `page.sourceAt(x, y)` for a click and `page.elementsAt(caret)` for the caret. Offsets
// named `start`/`end` count UTF-8 bytes; those named `utf16Start`/`utf16End`, and carets,
// count UTF-16 units (JavaScript string indices). Every position is in output units from
// the layout's top left, y down; a point that is a center is named `cx`, `cy`.

/*__NEUMA_WASM__*/
const WASM_GZIP_BASE64 = "";
const wasmUrl = () => null;

let module = null;
let wasm = null;
// Bumped on each new instance, so a Chant made before a restart can't reach a new one.
let generation = 0;
let crashed = null;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * An error the glue throws, `code` saying which: "invalid-option" (a TypeError: an option
 * this function doesn't take, or a value not among those it takes), "tone" (a tone that
 * can't be read), "unsupported" (this build of the module leaves the feature out),
 * "not-initialized" (`init()` hasn't run), "engine-stopped" (the engine stopped on an
 * internal error: call `init()` again), "no-constructor" (a TypeError: views and pages come
 * from a Chant), "fetch" (`init()` in `neuma-external.mjs` couldn't fetch `neuma.wasm`).
 */
function fail(Kind, code, message, cause) {
  const e = new Kind(`neuma: ${message}`, cause === undefined ? undefined : { cause });
  e.code = code;
  return e;
}

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
  if (layoutBudget.current !== DEFAULT_LAYOUT_BUDGET.current || layoutBudget.stale !== DEFAULT_LAYOUT_BUDGET.stale) {
    wasm.neuma_set_layout_budget(layoutBudget.current, layoutBudget.stale);
  }
}

/**
 * Initializes synchronously from the raw (uncompressed) `.wasm` bytes. After the engine
 * stops on an internal error, calling it (or `init`) again starts a fresh engine; Chants,
 * views and pages made before carry on in it, engraved again when next used.
 */
export function initSync(bytes) {
  if (wasm) return;
  if (!module) module = new WebAssembly.Module(bytes);
  instantiate();
}

/**
 * Initializes from the module's inlined copy (in `neuma-external.mjs`, from `neuma.wasm`
 * beside it, failing with the code "fetch" if it can't be fetched), or from `bytes` if given.
 * Under Node, whose fetch takes no file: URLs, `neuma-external.mjs` needs the bytes.
 */
export async function init(bytes) {
  if (wasm) return;
  if (module) return instantiate();
  if (bytes) return initSync(bytes);
  const url = wasmUrl();
  if (url) {
    let response;
    try {
      response = await fetch(url);
    } catch (e) {
      // Node's fetch takes no file: URLs, so the module can't fetch its engine from disk there.
      const hint = url.protocol === "file:" ? "; this runtime can't fetch a file: URL, so pass the bytes: init(readFileSync(<path to neuma.wasm>))" : "";
      throw fail(Error, "fetch", `fetching ${url} failed${hint}`, e);
    }
    if (!response.ok) throw fail(Error, "fetch", `fetching ${url} failed: ${response.status}`);
    return initSync(new Uint8Array(await response.arrayBuffer()));
  }
  const packed = Uint8Array.from(atob(WASM_GZIP_BASE64), (c) => c.charCodeAt(0));
  const stream = new Blob([packed]).stream().pipeThrough(new DecompressionStream("gzip"));
  initSync(new Uint8Array(await new Response(stream).arrayBuffer()));
}

function ready() {
  if (crashed) throw fail(Error, "engine-stopped", "the engine stopped on an internal error; call init() again", crashed);
  if (!wasm) throw fail(Error, "not-initialized", "call init() first");
  return wasm;
}

/** The engine's export `name`, or the error for a build that leaves `what` out. */
function need(w, name, what) {
  if (typeof w[name] !== "function") throw fail(Error, "unsupported", `this build of the module has no ${what}`);
  return w[name];
}

/** The engine's `{ error }`, if it gave one, as an error to throw. */
function engineResult(out) {
  if (out.error) throw fail(Error, "tone", out.error);
  return out;
}

const invalid = (message) => fail(TypeError, "invalid-option", message);

/**
 * `given`, an options object (or undefined or null, for none), over `defaults`: an option
 * left out, undefined or null takes its default; one `defaults` doesn't have throws.
 */
function options(given, defaults, what) {
  const out = { ...defaults };
  if (given === undefined || given === null) return out;
  if (typeof given !== "object") throw invalid(`${what} takes an options object; got ${JSON.stringify(given)}`);
  for (const key of Object.keys(given)) {
    if (!Object.hasOwn(defaults, key)) {
      throw invalid(`${what} has no option ${JSON.stringify(key)}; its options are ${Object.keys(defaults).join(", ")}`);
    }
    const v = given[key];
    if (v !== undefined && v !== null) out[key] = v;
  }
  return out;
}

/** The index of `value` among `allowed`, the values option `name` takes; else throws. */
function oneOf(name, value, allowed) {
  const i = allowed.indexOf(value);
  if (i < 0) throw invalid(`${name} must be ${allowed.map((v) => `"${v}"`).join(", ")}; got ${JSON.stringify(value)}`);
  return i;
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

// A pointer or length from the engine arrives as a signed 32-bit number; `>>> 0` reads it as
// the unsigned one it is, for memory past 2 GiB.

function putInput(text) {
  const w = ready();
  const bytes = encoder.encode(text);
  const ptr = w.neuma_input(bytes.length) >>> 0;
  new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
}

function takeOutput() {
  const w = ready();
  const ptr = w.neuma_output_ptr() >>> 0;
  const len = w.neuma_output_len() >>> 0;
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
 * @param {{ intone?: "first"|"every"|"never", autoPoint?: boolean,
 *   accents?: "all"|"none"|"outsideFlex" }} [options] `accents`: which acutes the printed
 *   text keeps (they place the cadence's accents either way); "outsideFlex" prints none in a
 *   flex half-verse, as a pointed psalter does.
 * @returns {{ gabc: string, notes: Array<PsalmNote>, diagnostics: Array<object> }}
 *   `notes[i]` describes note `i` of the engraved chant (`timeline.notes[i].id === i`):
 *   `{ verse, number, part: "flex"|"mediant"|"termination",
 *   role: "intonation"|"tenor"|"preparatory"|"accent"|"ending", sourceStart, sourceEnd,
 *   sourceUtf16Start, sourceUtf16End }`, its source the sung syllable's in `text`, named as
 *   the timeline names a note's. The diagnostics' offsets are in `text` too. To engrave the
 *   psalm with its spans in `text`, use `Chant.fromPsalm`.
 */
export function psalm(text, tone, psalmOptions) {
  const o = options(psalmOptions, PSALM_DEFAULTS, "psalm");
  return guarded((w) => {
    const run = need(w, "neuma_psalm", "psalm tones");
    putInput(String(tone) + "\0" + String(text));
    run(isBlock(tone), psalmFlags(o), o.autoPoint ? 1 : 0);
    return engineResult(JSON.parse(takeOutput()));
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
    const run = need(w, "neuma_point", "automatic pointing");
    putInput(String(tone) + "\0" + String(text));
    run(isBlock(tone));
    return engineResult(JSON.parse(takeOutput()));
  });
}

/**
 * Points psalm text for a tone, verse by verse, as a pointed psalter prints it under the tone
 * (draw the tone itself with `Chant.fromTone`). Half-verses with no marks are pointed
 * automatically, unless `autoPoint: false`; diagnostics are those `psalm` gives.
 * @param {string} text a verse per line, the mediant marked `*`
 * @param {string} tone as for `psalm`
 * @param {{ intone?: "first"|"every"|"never", autoPoint?: boolean,
 *   accents?: "all"|"none"|"outsideFlex" }} [options] as for `psalm`
 * @returns {{ toneLabel: string, verses: Array<{ number: number|null, sourceStart: number,
 *   sourceEnd: number, sourceUtf16Start: number, sourceUtf16End: number, runs: Array<object> }>,
 *   diagnostics: Array<object> }}
 *   `toneLabel` names the tone as a psalter prints it ("Tone 8 G", "Tonus peregrinus").
 *   Each verse's `runs`, their `text` joined, is its line after the number; the space between
 *   a mark and its syllable is U+00A0, and U+2060 (word joiner) follows each `–` and spelling
 *   hyphen, so a line never breaks there. A copy of the text keeps the invisible U+2060, and a
 *   renderer that doesn't shape text should drop it. A run's `kind` says how
 *   to style it: "text" (spaces, a word's hyphen), "syllable", "point" (`·`,
 *   bold red), "held" (`–`, bold red), "mediant" (`*`, red), "flex" (`†`, red) or "rubric"
 *   (red italic). A syllable's run also has `part`, `role` (its place in the tone, as
 *   `psalm`'s notes name it), `accent`, `flexDrop` (in a flex, where the voice drops:
 *   italic), `wordStart`, and its source in `text`.
 */
export function psalmDisplay(text, tone, psalmOptions) {
  const o = options(psalmOptions, PSALM_DEFAULTS, "psalmDisplay");
  return guarded((w) => {
    const run = need(w, "neuma_psalm_display", "psalm tones");
    putInput(String(tone) + "\0" + String(text));
    run(isBlock(tone), psalmFlags(o), o.autoPoint ? 1 : 0);
    return engineResult(JSON.parse(takeOutput()));
  });
}

// A tone block always has `key: value` lines; a tone name never has a colon.
const isBlock = (tone) => (String(tone).includes(":") ? 1 : 0);

const PSALM_DEFAULTS = { intone: "first", autoPoint: true, accents: "all" };
const INTONE = ["first", "every", "never"];
const ACCENTS = ["all", "none", "outsideFlex"];

// `intone` and `accents` as the module takes them, in one number: bits 0–1 and 2–3. A value
// that isn't one of the documented ones throws rather than falling back to the default.
const psalmFlags = ({ intone, accents }) => oneOf("intone", intone, INTONE) | (oneOf("accents", accents, ACCENTS) << 2);

/**
 * A tone's name as a psalter prints it beside the tone: "Tone 8 G" for "8.G", "Tonus
 * peregrinus" for "per"; a custom tone's other names as written.
 * @param {string} tone as for `psalm`
 * @returns {string}
 */
export function toneLabel(tone) {
  return guarded((w) => {
    const run = need(w, "neuma_tone_label", "psalm tones");
    putInput(String(tone));
    run(isBlock(tone));
    return engineResult(JSON.parse(takeOutput())).label;
  });
}

/** The built-in psalm tones' names, such as "8.G". Call after `init()`. */
export function toneNames() {
  return guarded((w) => {
    need(w, "neuma_tone_names", "psalm tones")();
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

const CHANT_DEFAULTS = { initial: 1, annotation: true, lyricSize: 2.45, font: "google" };
const FONTS = ["google", "eb-garamond-12"];

/** The engine's chant options from the glue's, read as `what` takes them. */
function chantArgs(given, what) {
  const { initial, annotation, lyricSize, font } = options(given, CHANT_DEFAULTS, what);
  return [Math.min(Math.max(Math.trunc(Number(initial)) || 0, 0), 255), annotation ? 1 : 0, lyricSize, oneOf("font", font, FONTS)];
}

/** Chant versions: counted here, so they keep growing across engine restarts. */
let versions = 0;

/** The diagnostics, then for a chant set from a psalm its `{ gabc, notes, diagnostics }`. */
function diagnosticsAndPsalm(out) {
  const nul = out.indexOf("\0");
  return nul < 0 ? [JSON.parse(out), undefined] : [JSON.parse(out.slice(0, nul)), JSON.parse(out.slice(nul + 1))];
}

/** `Chant`'s layout, for `View`. */
let layOut;
/** Lays a page out again for its hit tests, through its chant or from its recipe. */
let relayOut;
/** A chant's current version. */
let versionOf;

const VIEW_DEFAULTS = { scale: 6, lastLine: "ragged", maxLines: 0, prefix: "", svg: "whole", ids: true };

/** The engine's layout arguments for a view's options. */
function viewArgs({ scale, lastLine, maxLines, prefix, svg, ids }) {
  const lines = oneOf("svg", svg, ["whole", "lines"]) === 1;
  return {
    scale: Number(scale),
    last: oneOf("lastLine", lastLine, ["ragged", "justified"]),
    maxLines: maxLines >>> 0,
    prefix: String(prefix),
    lines,
    flags: (lines ? 2 | 8 : 0) | (ids ? 0 : 4),
  };
}

/** The engine's ten weights, NaN for each one to keep at its default. */
function weightArgs(weights) {
  const given = options(weights, Object.fromEntries(WEIGHTS.map((k) => [k, undefined])), "weights");
  return WEIGHTS.map((k) => (typeof given[k] === "number" ? given[k] : NaN));
}

/** Frees the engine's half of a chant or page JavaScript no longer holds. */
const unheld = typeof FinalizationRegistry === "function"
  ? new FinalizationRegistry(({ handle, gen, free }) => {
    if (gen === generation && wasm) wasm[free](handle);
  })
  : null;

export class Chant {
  #handle;
  #generation;
  #diagnostics;
  #summary;
  #psalm;
  /**
   * The chant's state: its version, and what makes it (the source, the options, and a
   * psalm's tone), from which a page of this state can be laid out again. A new object on
   * each change, shared by the pages laid out in that state.
   */
  #state;

  /**
   * Engraves `gabc` once.
   * @param {string} gabc
   * @param {{ initial?: number, annotation?: boolean, lyricSize?: number, font?: string }} [options]
   *   initial: drop-cap height in staves (0 for none, default 1). annotation: show the
   *   annotation or mode above it (default true). lyricSize: in staff spaces (default 2.45).
   *   font: which EB Garamond the page loads, "google" (Google Fonts, default) or
   *   "eb-garamond-12" (the EB Garamond 12 files), so lyrics are spaced for it.
   */
  constructor(gabc, options) {
    if (gabc === PSALM) return;
    this.#engrave({ source: String(gabc), args: chantArgs(options, "Chant"), psalm: null });
  }

  /**
   * Sets psalm text to a tone, as `psalm` does, and engraves it with its spans in `text`:
   * the timeline's and hit tests' sources, and the diagnostics' offsets, are in `text`.
   * `update(text)` sets the new text to the same tone. `psalm` holds the setting as `psalm`
   * returns it.
   * @param {string} text
   * @param {string} tone as for `psalm`
   * @param {{ intone?: "first"|"every"|"never", autoPoint?: boolean,
   *   accents?: "all"|"none"|"outsideFlex", initial?: number,
   *   annotation?: boolean, lyricSize?: number, font?: string }} [options] `psalm`'s options
   *   and the constructor's.
   */
  static fromPsalm(text, tone, psalmOptions) {
    const { intone, autoPoint, accents, ...rest } = options(psalmOptions, { ...PSALM_DEFAULTS, ...CHANT_DEFAULTS }, "Chant.fromPsalm");
    const psalm = {
      tone: String(tone),
      custom: isBlock(tone),
      psalmFlags: psalmFlags({ intone, accents }),
      autoPoint: autoPoint ? 1 : 0,
    };
    const chant = new Chant(PSALM);
    chant.#engrave({ source: String(text), args: chantArgs(rest, "Chant.fromPsalm"), psalm });
    return chant;
  }

  /**
   * A psalm tone as one line of notes with no words (the intonation, the mediant's cadence,
   * a bar, the termination's), as a pointed psalter prints it above the psalm.
   * @param {string} tone as for `psalm`
   * @param {object} [options] as the constructor takes (an initial has no words to drop).
   */
  static fromTone(tone, options) {
    chantArgs(options, "Chant.fromTone");
    const gabc = guarded((w) => {
      const run = need(w, "neuma_tone_gabc", "psalm tones");
      putInput(String(tone));
      run(isBlock(tone));
      return takeOutput();
    });
    if (gabc.startsWith("{")) engineResult(JSON.parse(gabc));
    return new Chant(gabc, options);
  }

  /**
   * Engraves `recipe` in the engine, as this chant's state: when made, and again when the
   * chant is used after `free()` or after the engine restarted (keeping its state, so its
   * pages stay current).
   */
  #engrave(recipe) {
    const { source, args, psalm } = recipe;
    const handle = guarded((w) => {
      const make = psalm ? need(w, "chant_from_psalm", "psalm tones") : w.chant_new;
      putInput(psalm ? psalm.tone + "\0" + source : source);
      return psalm ? make(psalm.custom, psalm.psalmFlags, psalm.autoPoint, ...args) : make(...args);
    });
    const out = takeOutput();
    if (handle < 0) engineResult(JSON.parse(out));
    this.#handle = handle;
    this.#generation = generation;
    unheld?.register(this, { handle, gen: generation, free: "chant_free" }, this);
    if (this.#state) return;
    [this.#diagnostics, this.#psalm] = diagnosticsAndPsalm(out);
    this.#state = { version: ++versions, recipe };
  }

  /** The engine's handle for this chant, engraving it again if the engine no longer has it. */
  #live() {
    if (this.#handle === undefined || this.#generation !== generation) this.#engrave(this.#state.recipe);
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

  /**
   * For a chant made with `fromPsalm`, the setting as `psalm` returns it:
   * `{ gabc, notes, diagnostics }`, following each `update`. Undefined otherwise.
   */
  get psalm() {
    return this.#psalm;
  }

  /**
   * Names the chant's current state, as in Rust and on mobile: a number no other state of
   * any chant has had, which grows with each `update` or `setOptions` that changed anything,
   * also across a restart of the engine. A page laid out at another version is `stale`; key
   * a memo or a framework's render on it.
   */
  get version() {
    return this.#state.version;
  }

  /** The source the chant was made or last updated from. */
  get source() {
    return this.#state.recipe.source;
  }

  /** Takes in a change the engine reported (2), or none (1); returns whether it changed. */
  #changed(status, recipe) {
    if (status !== 2) return false;
    [this.#diagnostics, this.#psalm] = diagnosticsAndPsalm(takeOutput());
    this.#summary = undefined;
    this.#state = { version: ++versions, recipe };
    return true;
  }

  /**
   * Replaces the score with `src` (GABC, or psalm text for a chant made with `fromPsalm`),
   * keeping this Chant's options, as an editor does on each change. Lay it out again to see
   * it; `diagnostics`, `summary` and `psalm` follow the new source, and pages laid out
   * before become `stale` but keep answering for the score they show. Returns whether
   * anything changed: not when `src` is the current source, so calling it again with the
   * same text is free.
   * @param {string} src
   * @returns {boolean}
   */
  update(src) {
    const handle = this.#live();
    const source = String(src);
    const status = guarded((w) => {
      putInput(source);
      return w.chant_update(handle);
    });
    return this.#changed(status, { ...this.#state.recipe, source });
  }

  /**
   * Engraves the score again with new options (those the constructor takes), as when the
   * reader changes the lyric size. They replace the current options, as the constructor
   * takes them: one left out takes its default, not its current value, as in Rust and on
   * mobile, so `setOptions({ lyricSize: 3 })` on a chant made with `{ initial: 0 }` brings
   * back the initial. Options that engrave as the current ones change nothing, and pages
   * laid out before stay current. Returns whether anything changed.
   * @returns {boolean}
   */
  setOptions(options) {
    const args = chantArgs(options, "setOptions");
    const handle = this.#live();
    const status = guarded((w) => w.chant_set_options(handle, ...args));
    return this.#changed(status, { ...this.#state.recipe, args });
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
   * A view of the score: a place that shows it (an editor's preview, a thumbnail), laid out
   * with these options at whatever width it has. `view.layout(width)` gives the same page
   * while nothing changed, and a new one, reusing the last one's lines, when something has.
   * @param {{ scale?: number, lastLine?: "ragged"|"justified", maxLines?: number,
   *   prefix?: string, svg?: "whole"|"lines", ids?: boolean }} [options] as `layout` takes.
   * @returns {View}
   */
  view(viewOptions) {
    return new View(VIEW, this, viewArgs(options(viewOptions, VIEW_DEFAULTS, "view")));
  }

  /**
   * Lays the score out at `width` output units (staff spaces times `scale`), as a new page
   * each call. A place that lays the score out again on each change does better through a
   * `view`, which gives the same page while nothing changed and reuses lines when it has.
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
  layout(width, layoutOptions) {
    const { weights, ...rest } = options(layoutOptions, { ...VIEW_DEFAULTS, weights: undefined }, "layout");
    return this.#make(Number(width), viewArgs(rest), weightArgs(weights), undefined);
  }

  /** Lays out a page, reusing the lines of `previous` (a page in parts) when given. */
  #make(width, view, weights, previous) {
    const handle = this.#live();
    const before = previous ? pageHandle(previous) : undefined;
    const at = [width, view.scale, view.last, view.maxLines];
    const made = guarded((w) => {
      putInput(view.prefix);
      return w.chant_layout(handle, ...at, view.flags, before ?? -1);
    });
    const out = takeOutput();
    const nul = out.indexOf("\0");
    const { width: w, height: h } = JSON.parse(out.slice(0, nul));
    const page = new Page(PAGE, this, this.#state, at, weights, made, w, h);
    if (view.lines) {
      const [head, defs, rest, ...tail] = out.slice(nul + 1).split("\0");
      const lines = [];
      // A line `previous` had comes as \u0001 and its index there, so its SVG isn't copied
      // out of the engine and decoded again.
      for (let i = 0; i + 1 < tail.length; i += 2) {
        const s = tail[i + 1];
        lines.push({ top: Number(tail[i]), svg: s.charCodeAt(0) === 1 ? linesOf.get(previous)[Number(s.slice(1))] : s });
      }
      page.svgParts = { head, defs, rest, lines };
      // The page's own list, which the caller can't change under the next layout.
      linesOf.set(page, lines.map((l) => l.svg));
    } else {
      page.svg = out.slice(nul + 1);
    }
    return page;
  }

  /**
   * Lays out again, for its hit tests, a page laid out at `state` with `at`: through this
   * chant while it is in that state, else from the state's recipe in a chant of its own.
   * Either way it lays out what it did the first time. Returns the engine's handle.
   */
  #relayOut(state, at) {
    // A freed chant isn't engraved again just for a page: the page lays out from its recipe.
    if (state === this.#state && this.#handle !== undefined && this.#generation === generation) {
      const handle = this.#handle;
      const made = guarded((w) => {
        putInput("");
        return w.chant_layout(handle, ...at, 16, -1);
      });
      if (made >= 0) return made;
    }
    const { source, args, psalm } = state.recipe;
    return guarded((w) => {
      putInput(psalm ? psalm.tone + "\0" + source : source);
      const made = w.page_rebuild(psalm ? 1 : 0, psalm?.custom ?? 0, psalm?.psalmFlags ?? 0, psalm?.autoPoint ?? 0, ...args, ...at);
      if (made < 0) throw fail(Error, "tone", "a psalm page's tone could not be read again");
      return made;
    });
  }

  static {
    layOut = (chant, width, view, weights, previous) => chant.#make(width, view, weights, previous);
    relayOut = (chant, state, at) => chant.#relayOut(state, at);
    versionOf = (chant) => chant.#state.version;
  }

  /**
   * Drops the engine's engraving now rather than when the Chant is garbage collected: a
   * hint, for when the chant goes away. The chant still works (used again, it engraves its
   * source again, as the same state), and so do its pages and views.
   */
  free() {
    if (this.#handle === undefined) return;
    unheld?.unregister(this);
    if (this.#generation === generation && wasm) wasm.chant_free(this.#handle);
    this.#handle = undefined;
  }
}

const PSALM = Symbol("psalm");
const PAGE = Symbol("page");
const VIEW = Symbol("view");
/** Each page in parts' lines' SVG, as the engine numbers them. */
const linesOf = new WeakMap();

/**
 * A place that shows a chant, laid out again as the chant or the place's width changes:
 * made by `chant.view(options)`. `view.layout(width)` returns the view's page for that
 * width: the same page while nothing changed (it remembers its last two, so a caller
 * alternating two widths gets each back), else a new one, made reusing the lines of the
 * last (in `svg: "lines"` mode).
 */
export class View {
  #chant;
  #args;
  #current = null;
  #previous = null;

  /** Views come from `Chant.view`. */
  constructor(token, chant, args) {
    if (token !== VIEW) throw fail(TypeError, "no-constructor", "views come from Chant.view");
    this.#chant = chant;
    this.#args = args;
  }

  /** The page this view last gave, or null. */
  get page() {
    return this.#current;
  }

  /**
   * The page for the chant as it is now at `width`. Asked again with the same width and
   * weights, and no change to the chant between, it returns the same page.
   * @param {number} width
   * @param {{ weights?: object }} [options] the page's timeline's weights, as `layout` takes.
   * @returns {Page}
   */
  layout(width, layoutOptions) {
    const at = Number(width);
    const values = weightArgs(options(layoutOptions, { weights: undefined }, "view.layout").weights);
    const same = (p) => p && !p.stale && pageAsked(p) === at && sameWeights(pageWeights(p), values);
    if (same(this.#current)) return this.#current;
    if (same(this.#previous)) {
      [this.#current, this.#previous] = [this.#previous, this.#current];
      return this.#current;
    }
    const page = layOut(this.#chant, at, this.#args, values, this.#args.lines ? this.#current : undefined);
    this.#previous = this.#current;
    this.#current = page;
    return page;
  }

  /**
   * Drops the engine's copies of the view's pages, as `page.free()` does: a hint for when
   * the view goes away. The pages, and the view, still work.
   */
  free() {
    this.#current?.free();
    this.#previous?.free();
  }
}

const sameWeights = (a, b) => a.every((v, i) => Object.is(v, b[i]));

/** A page's engine handle while the engine may hold it, without laying it out again. */
let pageHandle;
/** The width a page was asked for, and its weights. */
let pageAsked;
let pageWeights;

/**
 * A layout of a Chant: `width`, `height`, and `svg` or `svgParts`. Its `timeline()` and hit
 * tests answer for the score this page shows for as long as the page is held, whatever the
 * chant has laid out or become since, even after `chant.free()`. `stale` says the chant has
 * changed since. None of them throws for the page's age.
 *
 * The engine keeps the layout behind a page in a cache of the most recently used layouts
 * (see `setLayoutBudget`). A page whose layout was dropped lays itself out again when next
 * asked, from what it was made from, to the same result. `free()` drops it now.
 */
export class Page {
  #chant;
  /** The chant's state the page was laid out at, and its layout arguments. */
  #state;
  #at;
  #weights;
  #handle;
  #generation;
  #timeline;

  static {
    pageHandle = (page) => (page.#generation === generation && wasm ? page.#handle : undefined);
    pageAsked = (page) => page.#at[0];
    pageWeights = (page) => page.#weights;
  }

  /** Pages come from `Chant.layout` and `View.layout`. */
  constructor(token, chant, state, at, weights, handle, width, height) {
    if (token !== PAGE) throw fail(TypeError, "no-constructor", "pages come from Chant.layout or View.layout");
    this.#chant = chant;
    this.#state = state;
    this.#at = at;
    this.#weights = weights;
    this.width = width;
    this.height = height;
    this.#hold(handle);
  }

  #hold(handle) {
    this.#handle = handle;
    this.#generation = generation;
    unheld?.register(this, { handle, gen: generation, free: "page_free" }, this);
  }

  /**
   * Runs `f` with the engine's handle for this page, laying the page out again first if the
   * engine no longer holds its layout (`f` returns `unknown` then).
   */
  #ask(f, unknown) {
    const handle = pageHandle(this);
    const out = handle === undefined ? unknown : guarded((w) => f(w, handle));
    if (out !== unknown) return out;
    unheld?.unregister(this);
    this.#hold(relayOut(this.#chant, this.#state, this.#at));
    const again = this.#handle;
    return guarded((w) => f(w, again));
  }

  /** The chant's `version` when this page was laid out. */
  get version() {
    return this.#state.version;
  }

  /**
   * Whether the chant has changed (`update`, `setOptions`) since this page was laid out.
   * A stale page still answers for the score it shows; lay out again to show the new one.
   */
  get stale() {
    return this.#state.version !== versionOf(this.#chant);
  }

  /** The source this page shows. */
  get source() {
    return this.#state.recipe.source;
  }

  /**
   * The playback timeline, made on the first call and kept: `{ notes, pauses, lines,
   * duration }`, with times in weight units. Each note is `{ id, cx, cy, w, h, start,
   * duration, … }`, `cx`, `cy` its notehead's center; each pause `{ beforeNote, kind, start,
   * duration, bar }`, `bar` where its bar is drawn (`{ index, line, left, right, cx, top,
   * bottom }`) or null.
   */
  timeline() {
    if (this.#timeline === undefined) {
      this.#ask((w, h) => w.page_timeline(h, ...this.#weights), 0);
      this.#timeline = JSON.parse(takeOutput());
    }
    return this.#timeline;
  }

  /** The id of the note under (`x`, `y`), or the nearest on that line; null off the lines. */
  noteAt(x, y) {
    const id = this.#ask((w, h) => w.page_note_at(h, x, y), -2);
    return id < 0 ? null : id;
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
    this.#ask((w, h) => w.page_source_at(h, x, y), 0);
    return JSON.parse(takeOutput());
  }

  /**
   * What to highlight for a caret in the source this page shows: the notes and the bar
   * whose source holds it, then each box of its syllable (one per line it spans), most
   * specific first. A caret just after a note, as after typing it, counts as on it.
   * @param {number} caret a string index (UTF-16 units), such as `textarea.selectionStart`
   * @param {{ unit?: "utf16"|"utf8" }} [options] `unit: "utf8"` takes a byte offset instead.
   * @returns {Array<object>} elements as `sourceAt` returns them
   */
  elementsAt(caret, elementsOptions) {
    const utf16 = 1 - oneOf("unit", options(elementsOptions, { unit: "utf16" }, "elementsAt").unit, ["utf16", "utf8"]);
    // Past either end means at it: saturate to a u32 (the engine clamps to the source's
    // length) rather than let `>>>` wrap, and read NaN as 0.
    const at = Math.min(Math.max(Math.trunc(Number(caret)) || 0, 0), 0xffffffff);
    this.#ask((w, h) => w.page_elements_at(h, at, utf16), 0);
    return JSON.parse(takeOutput());
  }

  /**
   * Drops the engine's copy of this layout now, rather than when the cache makes room or
   * the page is garbage collected. The page still works: asked again, it lays itself out
   * again. Freeing twice does nothing.
   */
  free() {
    const handle = pageHandle(this);
    unheld?.unregister(this);
    if (handle !== undefined) wasm.page_free(handle);
    this.#handle = undefined;
  }
}

// `using page = chant.layout(…)` frees at the end of the block, where `Symbol.dispose` is.
if (typeof Symbol.dispose === "symbol") {
  for (const Class of [Chant, View, Page]) {
    Class.prototype[Symbol.dispose] = function () {
      this.free();
    };
  }
}

/** The layouts the engine keeps, when not set. */
const DEFAULT_LAYOUT_BUDGET = Object.freeze({ current: 64, stale: 2 });
let layoutBudget = DEFAULT_LAYOUT_BUDGET;

/** The most a pool can be set to keep: the engine takes it as a u32. */
const MAX_LAYOUT_BUDGET = 0xffffffff;

/** One pool's budget: a number at least 1, rounded down and capped at 2^32 − 1. */
function poolBudget(name, value) {
  if (typeof value !== "number" || !(value >= 1)) {
    throw invalid(`setLayoutBudget's ${name} must be a number at least 1 (or undefined or null for the default, ${DEFAULT_LAYOUT_BUDGET[name]}); got ${String(value)}`);
  }
  return Math.min(MAX_LAYOUT_BUDGET, Math.floor(value));
}

/**
 * Sets how many pages' layouts the engine keeps, in two pools across every chant, view and
 * page: `current`, of pages showing their chant as it is now (default 64, enough for a page
 * of many chants and their thumbnails, all hovered and clicked), and `stale`, of pages whose
 * chant has changed since or is gone (default 2). Past either, the least recently used is
 * dropped, and its page lays itself out again if it is asked again, to the same answers.
 * Layouts of a chant's current state share its engraving; a stale one holds the engraving
 * it was laid out from (under 200 KB for a typical score, about 4 MB for the longest). Laying
 * a page out again costs little while its chant is still in the page's state, and a full
 * layout (tens of milliseconds for the longest scores) once the chant has moved on.
 *
 * Set `current` above the number of pages shown at once: with more, a mouse moving across
 * them in turn always reaches the one dropped longest ago, so every hover lays a page out
 * again.
 *
 * Like every options object here, the budget replaces the last one: a pool left out,
 * undefined or null takes its default, and `setLayoutBudget()` restores both. A number sets
 * `current`, `stale` taking its default.
 * @param {number | { current?: number, stale?: number }} budget each pool a number at least
 *   1 (a page laid out again lives to answer), rounded down; `Infinity` or anything past
 *   2^32 − 1 keeps 2^32 − 1, no limit in practice. 0, a negative number, `NaN` or a value
 *   that isn't a number throws (code "invalid-option").
 */
export function setLayoutBudget(budget) {
  const given = typeof budget === "number" ? { current: budget } : budget;
  const { current, stale } = options(given, DEFAULT_LAYOUT_BUDGET, "setLayoutBudget");
  layoutBudget = Object.freeze({ current: poolBudget("current", current), stale: poolBudget("stale", stale) });
  if (wasm) wasm.neuma_set_layout_budget(layoutBudget.current, layoutBudget.stale);
}

/**
 * The engine's memory: `memory` (the WebAssembly memory's size in bytes, which only grows),
 * `layouts` (the pages' layouts it holds), `staleLayouts` (how many of those are stale) and
 * `budget` (`{ current, stale }`, see `setLayoutBudget`).
 */
export function engineStats() {
  const w = ready();
  return { memory: w.memory.buffer.byteLength, layouts: w.neuma_layouts(0) >>> 0, staleLayouts: w.neuma_layouts(1) >>> 0, budget: layoutBudget };
}
