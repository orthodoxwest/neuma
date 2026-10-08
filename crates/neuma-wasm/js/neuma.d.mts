// Types for neuma.mjs (and neuma-external.mjs, which differs only in where the engine
// comes from). See the package README for more on each call.

// `Symbol.dispose`, for `using`, where the TypeScript lib in use doesn't declare it yet.
declare global {
  interface SymbolConstructor {
    readonly dispose: unique symbol;
  }
}

/** What a glue error's `code` says it is. */
export type ErrorCode =
  /** A TypeError: an option the function doesn't take, or a value not among those it takes. */
  | "invalid-option"
  /** A tone that can't be read: no built-in tone by that name, or a tone block in error. */
  | "tone"
  /** This build of the module leaves the feature out (psalm tones, automatic pointing). */
  | "unsupported"
  /** `init()` hasn't run. */
  | "not-initialized"
  /** The engine stopped on an internal error; `init()` starts a fresh one. */
  | "engine-stopped"
  /** A TypeError: views and pages come from a Chant, not their constructors. */
  | "no-constructor"
  /**
   * `init()` in `neuma-external.mjs` couldn't fetch `neuma.wasm`: an HTTP error, or a runtime
   * whose fetch takes no file: URL (Node), which needs `init(bytes)`.
   */
  | "fetch";

/** An error the glue throws: an `Error` or a `TypeError` with a `code`. */
export interface NeumaError extends Error {
  code: ErrorCode;
}

/**
 * Options are objects of the keys each call lists; a key it doesn't list throws (code
 * "invalid-option"). An option left out, `undefined` or `null` takes its default.
 */
type Opt<T> = T | null | undefined;

/** A range of the source: `start`/`end` in UTF-8 bytes, `utf16Start`/`utf16End` in UTF-16 units. */
export interface Span {
  start: number;
  end: number;
  utf16Start: number;
  utf16End: number;
}

/** A note's (or a psalm half's) source, in bytes and in UTF-16 units. */
export interface SourceSpan {
  sourceStart: number;
  sourceEnd: number;
  sourceUtf16Start: number;
  sourceUtf16End: number;
}

export type Severity = "error" | "warning" | "info";

/** The one edit that fixes a diagnostic: replace `utf16Start`..`utf16End` with `replacement`. */
export interface Fix extends Span {
  replacement: string;
  title: string;
}

/** A problem found reading the source. `code` is stable across versions. */
export interface Diagnostic extends Span {
  severity: Severity;
  code: string;
  message: string;
  fix: Fix | null;
}

/** Which EB Garamond the page draws lyrics with, so they are spaced for it. */
export type LyricFont = "google" | "eb-garamond-12";

/** How a chant is engraved. */
export interface ChantOptions {
  /** Drop-cap height in staves: 0 for none; default 1. */
  initial?: Opt<number>;
  /** Show the annotation (or mode) above the initial; default true. */
  annotation?: Opt<boolean>;
  /** Lyric size in staff spaces; default 2.45. */
  lyricSize?: Opt<number>;
  /** Default "google". */
  font?: Opt<LyricFont>;
}

/** How a psalm is set to its tone. */
export interface PsalmOptions {
  /** Where the intonation is sung: the first verse (default), every verse, or never. */
  intone?: Opt<"first" | "every" | "never">;
  /** Point half-verses with no marks automatically; default true. */
  autoPoint?: Opt<boolean>;
  /** Which acutes the printed text keeps; default "all". */
  accents?: Opt<"all" | "none" | "outsideFlex">;
}

/** Relative durations for the timeline. */
export interface Weights {
  note: number;
  mora: number;
  episema: number;
  virgula: number;
  quarter: number;
  half: number;
  full: number;
  double: number;
  mediant: number;
  flex: number;
}

/** Weights to override; a missing, null or non-numeric one keeps its default. */
export type WeightOptions = { [K in keyof Weights]?: Opt<number> };

/** A view's options: everything about a layout but its width and weights. */
export interface ViewOptions {
  /** Output units per staff space; default 6 (any value that isn't positive is 6 too). */
  scale?: Opt<number>;
  /** Default "ragged". */
  lastLine?: Opt<"ragged" | "justified">;
  /** Keep only the first lines, as broken for the whole score; default 0, all. */
  maxLines?: Opt<number>;
  /** Class and id prefix for the SVG; default "neuma". */
  prefix?: Opt<string>;
  /** "lines" gives `svgParts` instead of `svg`; default "whole". */
  svg?: Opt<"whole" | "lines">;
  /** false leaves out `data-note` and `data-syllable`; default true. */
  ids?: Opt<boolean>;
}

export interface LayoutOptions extends ViewOptions {
  weights?: Opt<WeightOptions>;
}

/** A page's SVG in parts: wrap each line's `svg` in `<g transform="translate(0 top)">`. */
export interface SvgParts {
  /** The `<svg>` start tag and style. */
  head: string;
  /** The glyph `<path>`s, for a `<defs>`. */
  defs: string;
  /** The initial and its annotations. */
  rest: string;
  lines: Array<{ top: number; svg: string }>;
}

export type NoteShape =
  | "punctum"
  | "inclinatum"
  | "virga"
  | "virga-reversa"
  | "quilisma"
  | "oriscus"
  | "oriscus-scapus"
  | "stropha";

export interface TimelineNote extends SourceSpan {
  /** Stable across layouts of one Chant; SVG ink lists it in `data-note`. */
  id: number;
  /** The notehead's center. */
  cx: number;
  cy: number;
  w: number;
  h: number;
  /** In weight units. */
  start: number;
  duration: number;
  line: number;
  syllable: number;
  word: number;
  /** Advances after each full or double bar. */
  verse: number;
  /** 1 after the mediant `*`. */
  half: number;
  staffPosition: number;
  degree: number;
  /** From the clef's do, with flats applied. */
  semitones: number;
  syllableText: string;
  vowel: string | null;
  shape: NoteShape;
  liquescent: boolean;
  /** The syllable has an acute in the source. */
  accent: boolean;
  /** The first note of its syllable. */
  newSyllable: boolean;
  /** Inferred from three or more single-note syllables on one pitch. */
  recitation: boolean;
}

export type PauseKind =
  | "virgula"
  | "minimis"
  | "quarter"
  | "half"
  | "full"
  | "dotted-full"
  | "double"
  | "dominican"
  | "mediant"
  | "flex";

export interface Pause {
  beforeNote: number;
  kind: PauseKind;
  start: number;
  duration: number;
}

export interface LineBox {
  top: number;
  bottom: number;
  /** The staff's middle line. */
  staff: number;
  /** The lyrics'. */
  baseline: number;
}

export interface Timeline {
  notes: TimelineNote[];
  pauses: Pause[];
  lines: LineBox[];
  duration: number;
}

/** What a hit test or a caret finds: a box from its top left, and its source. */
export interface SourceElement extends Span {
  kind: "note" | "bar" | "syllable";
  /** The note id, or the bar's or syllable's index in the score. */
  index: number;
  line: number;
  x: number;
  y: number;
  w: number;
  h: number;
  /** A note's notehead center. */
  cx: number;
}

export type OfficePartKind =
  | "antiphon"
  | "introit"
  | "gradual"
  | "alleluia"
  | "tract"
  | "sequence"
  | "offertory"
  | "communion"
  | "hymn"
  | "responsory"
  | "short-responsory"
  | "versicle"
  | "chapter"
  | "collect"
  | "psalm"
  | "canticle"
  | "kyrie"
  | "gloria"
  | "credo"
  | "sanctus"
  | "agnus"
  | "other";

/** A score's library entry (see the README). */
export interface Summary {
  name: string | null;
  officePart: string | null;
  occasion: string | null;
  book: string | null;
  language: string | null;
  transcriber: string | null;
  gabcCopyright: string | null;
  scoreCopyright: string | null;
  commentary: string | null;
  kind: OfficePartKind | null;
  mode: { number: number | null; name: string; modifier: string | null; differentia: string | null } | null;
  annotations: string[];
  otherHeaders: Array<{ name: string; value: string }>;
  incipit: string;
  text: string;
  lowest: number | null;
  highest: number | null;
  finalPitch: number | null;
  notes: number;
  syllables: number;
  words: number;
  duration: number;
}

export type VersePart = "flex" | "mediant" | "termination";
export type ToneRole = "intonation" | "tenor" | "preparatory" | "accent" | "ending";

/** Note `i` of an engraved psalm, its source the sung syllable's in the text. */
export interface PsalmNote extends SourceSpan {
  verse: number;
  number: number | null;
  part: VersePart;
  role: ToneRole;
}

/** A psalm's diagnostics are those of setting the text: offsets in the text. */
export interface PsalmSetting {
  gabc: string;
  notes: PsalmNote[];
  diagnostics: Diagnostic[];
}

export interface Pointing {
  text: string;
  halves: Array<
    SourceSpan & {
      verse: number;
      part: "mediant" | "termination";
      /** The model's probability, 0 to 1; below about 0.8 worth checking. */
      confidence: number;
      kept: boolean;
    }
  >;
  diagnostics: Diagnostic[];
}

export type PsalmRun =
  | { text: string; kind: "text" | "point" | "held" | "mediant" | "flex" | "rubric" }
  | (SourceSpan & {
      text: string;
      kind: "syllable";
      part: VersePart;
      role: ToneRole;
      accent: boolean;
      /** In a flex, where the voice drops: italic. */
      flexDrop: boolean;
      wordStart: boolean;
    });

export interface PsalmDisplay {
  /** The tone as a psalter prints it: "Tone 8 G". */
  toneLabel: string;
  verses: Array<SourceSpan & { number: number | null; runs: PsalmRun[] }>;
  diagnostics: Diagnostic[];
}

/** Engine memory and the layouts it holds. */
export interface EngineStats {
  /** The WebAssembly memory's size in bytes, which only grows. */
  memory: number;
  /** The pages' layouts the engine holds. */
  layouts: number;
  /** How many of those are stale: their chant has changed since, or is gone. */
  staleLayouts: number;
  budget: LayoutBudget;
}

export interface LayoutBudget {
  /** Layouts of pages showing their chant as it is now; default 64. */
  current: number;
  /** Layouts of the others; default 2. */
  stale: number;
}

export const DEFAULT_WEIGHTS: Readonly<Weights>;

/** Initializes from the module's own copy of the engine, or from the `.wasm` bytes given. */
export function init(bytes?: BufferSource): Promise<void>;
/** Initializes synchronously from the raw `.wasm` bytes. */
export function initSync(bytes: BufferSource): void;

/** A score's library entry, without engraving it. */
export function summarize(gabc: string): Summary;
/** Psalm text set to a tone (a built-in name such as "8.G", or a tone block), as GABC. */
export function psalm(text: string, tone: string, options?: Opt<PsalmOptions>): PsalmSetting;
/** Psalm text pointed for a tone. */
export function point(text: string, tone: string): Pointing;
/** Psalm text pointed for a tone, verse by verse, as a pointed psalter prints it. */
export function psalmDisplay(text: string, tone: string, options?: Opt<PsalmOptions>): PsalmDisplay;
/** A tone's name as a psalter prints it: "Tone 8 G". */
export function toneLabel(tone: string): string;
/** The built-in tones' names. */
export function toneNames(): string[];
/** The note sounding at time `t` of a timeline, or null. */
export function noteAtTime(timeline: { notes: TimelineNote[] }, t: number): TimelineNote | null;
/**
 * Sets how many layouts the engine keeps, replacing the last budget: a pool left out,
 * undefined or null takes its default (64 current, 2 stale), and a number sets `current`.
 * Each pool is a number at least 1, rounded down; `Infinity` or anything past 2^32 − 1
 * keeps 2^32 − 1. Anything else throws (code "invalid-option"). Set `current` above the
 * number of pages shown at once, or hovering across them lays each out again.
 */
export function setLayoutBudget(budget?: number | Opt<Partial<LayoutBudget>>): void;
export function engineStats(): EngineStats;

/** One score, engraved once and laid out at any width. */
export class Chant {
  constructor(gabc: string, options?: Opt<ChantOptions>);
  /** Psalm text set to a tone and engraved, with spans in the text. */
  static fromPsalm(text: string, tone: string, options?: Opt<PsalmOptions & ChantOptions>): Chant;
  /** A tone as one line of notes with no words. */
  static fromTone(tone: string, options?: Opt<ChantOptions>): Chant;
  readonly diagnostics: Diagnostic[];
  /** For a chant made with `fromPsalm`, its setting; undefined otherwise. */
  readonly psalm: PsalmSetting | undefined;
  /** Names the chant's state; grows with each change. */
  readonly version: number;
  readonly source: string;
  readonly summary: Summary;
  /** Replaces the score, keeping the options. Returns whether anything changed. */
  update(src: string): boolean;
  /**
   * Engraves again with these options, replacing the current ones (as the constructor takes
   * them: one left out takes its default, not its current value), as in Rust and on mobile.
   * Returns whether anything changed.
   */
  setOptions(options?: Opt<ChantOptions>): boolean;
  view(options: ViewOptions & { svg: "lines" }): View<LinesPage>;
  view(options?: Opt<ViewOptions & { svg?: Opt<"whole"> }>): View<WholePage>;
  /** With `svg` known only at run time: check the page's `svgParts` or `svg`. */
  view(options?: Opt<ViewOptions>): View<WholePage | LinesPage>;
  /** A new page each call; a place that lays out again on each change wants a view. */
  layout(width: number, options: LayoutOptions & { svg: "lines" }): LinesPage;
  layout(width: number, options?: Opt<LayoutOptions & { svg?: Opt<"whole"> }>): WholePage;
  /** With `svg` known only at run time: check the page's `svgParts` or `svg`. */
  layout(width: number, options?: Opt<LayoutOptions>): WholePage | LinesPage;
  /** Drops the engine's engraving now; the chant still works. */
  free(): void;
  [Symbol.dispose](): void;
}

/** A place that shows a chant: the same page while nothing changed. */
export class View<P extends Page = Page> {
  private constructor();
  /** The page this view last gave, or null. */
  readonly page: P | null;
  layout(width: number, options?: Opt<{ weights?: Opt<WeightOptions> }>): P;
  /** Drops the engine's copies of the view's pages; they still work. */
  free(): void;
  [Symbol.dispose](): void;
}

/** A layout of a chant, answering for the score it shows for as long as it is held. */
export class Page {
  private constructor();
  readonly width: number;
  readonly height: number;
  /** With `svg: "whole"`. */
  svg?: string;
  /** With `svg: "lines"`. */
  svgParts?: SvgParts;
  /** The chant's version this page was laid out at. */
  readonly version: number;
  /** Whether the chant has changed since. */
  readonly stale: boolean;
  /** The source this page shows. */
  readonly source: string;
  timeline(): Timeline;
  /** The note under (x, y), or the nearest on that line; null off the lines. */
  noteAt(x: number, y: number): number | null;
  sourceAt(x: number, y: number): SourceElement | null;
  /** What to highlight for a caret (UTF-16 units, or bytes with `unit: "utf8"`). */
  elementsAt(caret: number, options?: Opt<{ unit?: Opt<"utf16" | "utf8"> }>): SourceElement[];
  /** Drops the engine's copy of this layout; the page lays itself out again when asked. */
  free(): void;
  [Symbol.dispose](): void;
}

export type WholePage = Page & { svg: string; svgParts?: undefined };
export type LinesPage = Page & { svgParts: SvgParts; svg?: undefined };
