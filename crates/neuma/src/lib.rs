//! Neuma reads [GABC](https://gregorio-project.github.io/gabc/), the text format of the
//! Gregorio project, and engraves Gregorian chant in square notation that reflows to any
//! width. A layout comes out as SVG, as a renderer-neutral display list of glyphs, rectangles
//! and text for a native canvas, as a timeline with the position, pitch and duration of every
//! note for practice tools, and as a source map for editors. Psalm tones, and English psalm
//! text pointed and set to them, are in [`neuma_tones`](https://docs.rs/neuma-tones).
//!
//! [`Chant`] is the front door: it owns a score and everything made from it, and updates
//! incrementally as the source changes.
//!
//! ```
//! use neuma::Chant;
//!
//! let gabc = "name: Regina caeli;\nmode: 6;\n%%\n\
//!     (c4) RE(f)gí(g)na(h) cae(hj)li,(h) lae(ghg)tá(fe)re,(f.) (;) al(f)le(g)lú(hg)ia.(g.) (::)";
//!
//! // Parsing never fails: problems come back as diagnostics.
//! let chant = Chant::new(gabc); // engraved once; keep it, share it: it is Send + Sync
//! for d in chant.diagnostics() {
//!     eprintln!("{d}");
//! }
//!
//! // Lay out at any width: cheap enough for every resize. A layout is an owned value.
//! let layout = chant.layout(720.0);
//! let svg = layout.svg(); // lyrics set in EB Garamond
//! let timeline = layout.timeline(); // where and when each note is
//! assert!(svg.starts_with("<svg") && timeline.notes.len() == 17);
//! let first = &timeline.notes[0];
//! assert_eq!(layout.note_at(first.cx, first.cy), Some(first.id)); // hit tests, per layout
//! ```
//!
//! A [`Layout`] is an owned value, cheap to clone and `Send + Sync`: keep layouts at several
//! widths at once, and ask each for its SVG, display list, timeline, source map and hit tests
//! (`note_at`, `source_at`, `elements_at`) while the chant changes.
//!
//! A native renderer draws the [`DisplayList`]: glyph outlines by id, filled rectangles, and
//! text runs set in the lyric font with ligatures off, all in output units from the top left.
//!
//! ```
//! use neuma::{Chant, Item, glyph_outline};
//!
//! let chant = Chant::new("(c4) Al(f)le(gf)lú(gh)ia.(g.) (::)");
//! let list = chant.layout(600.0).display();
//! let (mut glyphs, mut rects) = (0, 0);
//! for item in &list.items {
//!     match item {
//!         // An outline in glyph units: scale it by `scale` and draw its origin at (x, y).
//!         // Fetch each id's outline once and keep it.
//!         Item::Glyph { glyph, x, y, scale, .. } => {
//!             let outline = glyph_outline(*glyph).expect("a glyph the engine drew");
//!             let _ = (outline.d, x, y, scale); // fill the SVG path data `d`, nonzero
//!             glyphs += 1;
//!         }
//!         Item::Rect { x, y, w, h, .. } => {
//!             let _ = (x, y, w, h); // staff and ledger lines, stems, bars, episemata
//!             rects += 1;
//!         }
//!         Item::Text { x, baseline, size, runs, .. } => {
//!             let _ = (x, baseline, size); // each run's text in its style, one after another
//!             assert!(!runs.is_empty());
//!         }
//!         _ => {} // kinds added later
//!     }
//! }
//! assert!(glyphs > 0 && rects > 0); // the notes and clef; the staff lines and the bar
//! ```
//!
//! # Concepts
//!
//! ## From a chant to a page
//!
//! Work is split by what it depends on, so that each change redoes only its share:
//!
//! 1. **Reading** ([`parse`], or [`Chant::new`] and [`Chant::update`]) turns GABC into a
//!    [`Score`], the one model of the notes and text. It depends only on the source.
//!    `neuma_tones` builds a `Score` from psalm text and a tone, and [`ScoreBuilder`] from
//!    anything else.
//! 2. **Engraving** builds the neumes, measures the lyrics, centers them on their vowels and
//!    sets the spacing, as segments that don't depend on the width. It depends on the score,
//!    the [`StyleOptions`] (lyric size, initial, alterations, …) and a [`TextMeasure`]:
//!    [`Score::engrave`] takes both, and a `Chant` takes them as one [`ChantOptions`] (with
//!    the lyric font, whose metrics measure the text) in [`Chant::with_options`] and
//!    [`Chant::set_options`].
//! 3. **Layout** breaks the segments into lines at a width, justifies them, and places
//!    clefs, custodes and the initial. It depends on the width and the [`LayoutOptions`]:
//!    `layout(width)` uses the defaults and `layout_with(width, &options)` takes them, on a
//!    [`Chant`] ([`Chant::layout`], [`Chant::layout_with`]) as on an [`Engraving`]
//!    ([`Engraving::layout`], [`Engraving::layout_with`]). It is cheap enough to redo on every
//!    resize.
//! 4. **Output** comes from a [`Layout`]: [`Layout::svg`], [`Layout::display`] (a
//!    [`DisplayList`] of glyphs, rectangles and text for a native canvas),
//!    [`Layout::timeline`] (each note's place, pitch and duration) and
//!    [`Layout::source_map`] (what is drawn where, and from which source bytes).
//!
//! A [`Chant`] runs the first three and keeps what they made: after
//! [`update`](Chant::update) it reads and engraves again only around the edit, and its next
//! layout breaks again only the lines the edit touched, with the same result as starting
//! afresh. The lower stages stay public for advanced use; share an [`Engraving`] in an `Arc`
//! to lay it out at many widths. The browser package and the mobile bindings wrap a `Chant`.
//! What they call a page (`chant.layout(width)` in JavaScript, a `ChantLayout` on mobile) is
//! a `Layout` with its outputs, and a JavaScript view is the place a page is shown, laid out
//! again on each change.
//!
//! ## Units and coordinates
//!
//! - **Staff spaces** measure the notation, and everything that scales with it. One staff
//!   space is the distance between two adjacent staff positions, a line and the space beside
//!   it, so the four staff lines are two staff spaces apart. A punctum is one staff space wide.
//!   This is half of what engravers (SMuFL, Gould's *Behind Bars*, GregorioTeX) call a staff
//!   space, the distance from one line to the next.
//! - **Staff positions** ([`score::StaffPosition`]) count those steps from the staff's middle
//!   space: the lines are at −3, −1, 1 and 3, and positions grow upward (GABC's `a` is −6).
//! - **Ems** measure text: a [`TextMeasure`] gives advances in ems of the lyric font, and
//!   [`StyleOptions::lyric_size`] says how many staff spaces an em is (2.45 by default,
//!   GregorioTeX's 10 pt lyrics on its default staff). Text and notation scale together.
//! - **Glyph units** are the outlines' coordinates ([`glyph_outline`]): 100 to a staff space.
//! - **Output units** are what every output is in: staff spaces times
//!   [`LayoutOptions::scale`] (6 by default), so px in a browser, or pt or dp on a phone. The
//!   width given to `layout` is in output units, and lines are broken at `width / scale` staff
//!   spaces, so a reader's text-size change is a new `scale` and a new layout, not a new
//!   engraving.
//!
//! Every output of a layout shares one coordinate system: the origin is the layout's top left
//! corner, y grows downward, and the unit is the output unit. Boxes are given by their
//! top-left corner and size (`x`, `y`, `w`, `h`); the one point given as a center is named so
//! ([`TimelineNote::cx`] and `cy`). Source spans count UTF-8 bytes; [`Utf16Index`] converts to
//! the UTF-16 units JavaScript, Kotlin and Swift strings count.
//!
//! ## Versions
//!
//! [`Chant::version`] names a chant's state: a number no other state of any chant in the
//! process has had, which grows with each change that changed anything. An `update` or
//! `set_options` that changes nothing returns `false` and keeps the version, so a view keyed
//! on the version lays out again exactly when it must. A [`Layout`] keeps showing the state
//! it was made from for as long as it is held, whatever the chant has become since.
//!
//! ## Diagnostics
//!
//! Reading and engraving never fail. What the engine can't read, doesn't support or has to
//! approximate comes back as a [`Diagnostic`] with a [`Severity`], a source span, a stable
//! code (such as `gabc::unclosed-notes`) and, where one edit makes sense, a [`Fix`];
//! [`Chant::diagnostics`] lists them, and the engine draws what it can. Codes are stable and
//! messages are not. `docs/diagnostics.md` in the repository lists every code.
//!
//! ## Features
//!
//! - `svg` (default): [`Layout::svg`] and the SVG writer.
//! - `fonts` (default): the metrics of both built-in EB Garamond releases; `font-google` or
//!   `font-garamond12` builds in one of them ([`LyricFont`]).
//! - `json`: the `json` module, the outputs as the JSON the browser package and the CLI give.

#![warn(missing_docs)]

/// `with_*` setters for an options struct's fields, so callers outside the crate needn't
/// spell out a struct literal (the structs are `#[non_exhaustive]`).
macro_rules! setters {
    ($ty:ident { $($field:ident: $t:ty => $name:ident),* $(,)? }) => {
        impl $ty {
            $(
                #[doc = concat!("Sets [`", stringify!($field), "`](Self::", stringify!($field), ").")]
                #[must_use]
                pub fn $name(mut self, $field: $t) -> Self {
                    self.$field = $field;
                    self
                }
            )*
        }
    };
}
pub(crate) use setters;

mod chant;
#[cfg(any(feature = "svg", feature = "json"))]
pub(crate) mod decimal;
pub mod diag;
mod display;
mod engrave;
#[cfg(any(feature = "font-google", feature = "font-garamond12"))]
mod fonts;
mod gabc;
/// The glyph table generated from exsurge's outlines. Its names and variants follow the
/// generator and may change in any release; glyphs cross the public API as `u16` ids, with
/// outlines from [`glyph_outline`].
#[doc(hidden)]
pub mod glyphs;
#[cfg(feature = "json")]
pub mod json;
mod layout;
pub mod metrics;
mod notes;
pub mod score;
mod source;
mod summary;
#[cfg(feature = "svg")]
mod svg;
mod text;
mod vowel;

// The modules above that only hold what is re-exported here are private, so each item has
// one place in the documentation: the crate's root.
#[doc(inline)]
pub use chant::{Chant, ChantOptions};
#[doc(inline)]
pub use diag::{Diagnostic, Fix, Severity};
#[doc(inline)]
pub use display::{DisplayList, Item, LineBox, NoteRef, TextRole, TextRun};
#[doc(inline)]
pub use engrave::{AlterationScope, CustosPolicy, Engraving, Initial, Ink, StyleOptions};
#[doc(inline)]
#[cfg(any(feature = "font-google", feature = "font-garamond12"))]
pub use fonts::LyricFont;
#[doc(inline)]
pub use gabc::{Parsed, parse};
#[doc(inline)]
pub use glyphs::{GlyphOutline, glyph_outline};
#[doc(inline)]
pub use layout::{LastLine, Layout, LayoutOptions};
#[doc(inline)]
pub use metrics::{MetricsError, MetricsTable};
#[doc(inline)]
pub use notes::{Pause, PauseBar, PauseKind, Timeline, TimelineNote, Weights};
#[doc(inline)]
pub use score::{BarKind, NoteShape, Score, ScoreBuilder, TextStyle};
#[doc(inline)]
pub use source::{Element, ElementKind, SourceMap, Utf16Index};
#[doc(inline)]
pub use summary::{Mode, OfficePart, Summary, summarize};
#[doc(inline)]
#[cfg(feature = "svg")]
pub use svg::{SvgLine, SvgOptions, SvgParts};
#[doc(inline)]
pub use text::{ApproxMeasure, TextMeasure};
#[doc(inline)]
pub use vowel::VowelRules;

impl Score {
    /// This score as GABC.
    #[must_use]
    pub fn to_gabc(&self) -> String {
        gabc::to_gabc(self)
    }
}
