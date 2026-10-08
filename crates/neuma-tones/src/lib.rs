//! Psalm tones for neuma: pointed English psalm text plus a psalm tone, set as chant
//! ([`psalm`], [`PsalmChant`]), or shown as a pointed psalter prints it, the tone once as a
//! line of notes ([`Tone::gabc`]) over the pointed verses ([`PsalmDisplay`]).
//!
//! ```
//! use neuma_tones::{PsalmChant, PsalmOptions, Tone, psalm};
//!
//! let tone = Tone::named("8.G")?;
//! let text = "The Lord is King, and hath put on glorious ap·pá-rel; * \
//!     the Lord hath put on his apparel, and gird·ed him-sélf with strength.";
//! let setting = psalm(text, tone, &PsalmOptions::default());
//! assert!(setting.gabc.contains("pá(k)"));
//! // Engraved with its spans in the text, so the timeline and hit tests answer there.
//! let chant = PsalmChant::new(text, tone, &PsalmOptions::default(), neuma::ChantOptions::default());
//! let first = &chant.layout(600.0).timeline().notes[0];
//! assert_eq!(&text[first.span.clone()], "The");
//! # Ok::<(), neuma_tones::ToneError>(())
//! ```
//!
//! # Concepts
//!
//! ## Pointed text
//!
//! The text is a verse per line, marked as a hand-pointed psalter prints it:
//!
//! | Mark | Meaning |
//! |---|---|
//! | `*` | The mediant: the end of the first half-verse. |
//! | `†` | The flex: a short drop before the mediant, in long first halves. |
//! | `·` | The cadence starts at the next syllable. Inside a word it also splits it ("e·ver"). |
//! | acute (`á`) | An accented syllable, which takes an accent of the tone's cadence. |
//! | `–` (en dash) | A note of the cadence with no syllable of its own: the syllable before it is held ("thou · árt – mý God", "Dávid, – – *"). Before a half's first syllable it leaves a note out instead ("* – · – – práise the Lord"). |
//! | `-` inside a word | A sung syllable split ("judg-ed"). `\-` (or `-` and U+2060, or U+2011) is a hyphen that is only spelling ("blood\-guiltiness"). A hyphen with the `·` after it is spelling too ("pre-·eminence"), the `·` before it a dotted split ("hon·-our"). |
//! | `[…]` | A rubric, such as a posture cue: kept, never sung. |
//! | `12` at the start of a line | The verse number, after any rubrics that open the line ("[Stand.] 5 For I …"). |
//!
//! Any Unicode space separates words, U+00A0 among them, U+2060 (word joiner) is dropped,
//! and a line starting with `#` is a comment.
//!
//! A half-verse without marks is pointed automatically ([`point`], with the `pointing`
//! feature, on by default), and marks written by hand are kept. [`point`] returns the text
//! with the marks added; [`psalm`] and [`PsalmChant`] point and set it in one step.
//!
//! ## Tones
//!
//! A [`Tone`] is a psalm tone with one ending: an intonation, a reciting note (the tenor) and
//! a cadence for each half-verse and the flex ([`Cadence`]), each a formula of slots
//! ([`Slot`]): one accented syllable, one unaccented syllable, or any number of unaccented
//! syllables on one note. [`Tone::named`] gives the built-in Solesmes tones (`8.G`,
//! `1.D2`, `per`, …) and [`Tone::parse`] reads one of your own.
//!
//! ## Sung or printed
//!
//! - **Sung:** [`psalm`] sets the text to the tone as a [`PsalmSetting`]: a
//!   [`neuma::Score`], its GABC, and each note's place in the tone ([`PsalmNote`]).
//!   [`PsalmChant`] engraves it as a [`neuma::Chant`] whose source is the psalm text, so the
//!   layout's timeline, hit tests and diagnostics answer in the text a reader sees, and its
//!   [`update`](PsalmChant::update) sets new text to the same tone incrementally. Its
//!   versions are its chant's ([`neuma::Chant::version`]).
//! - **Printed:** [`PsalmDisplay`] points the text verse by verse as styled runs, to print
//!   under the tone, which [`Tone::gabc`] gives as one line of notes.
//!
//! [`AnyChant`] holds either a GABC chant or a psalm chant, for an app that shows both.
//!
//! ## Spans and diagnostics
//!
//! Spans count UTF-8 bytes of the psalm text, as neuma's count bytes of GABC. Setting a psalm
//! never fails: problems come back as [`neuma::Diagnostic`]s with stable codes, `pointed::`
//! for the markup, `apply::` for fitting the text to the tone, and `point::unsure` for a
//! half-verse the pointer is less than [`UNSURE`] sure of. `docs/diagnostics.md` in the
//! repository lists them.

#![warn(missing_docs)]

mod apply;
mod chant;
mod display;
#[cfg(feature = "pointing")]
mod point;
mod pointed;
mod syllable;
mod tone;

pub use apply::{Accents, Intone, PsalmNote, PsalmOptions, PsalmSetting, ToneRole, UNSURE, psalm};
pub use chant::{AnyChant, PsalmChant};
pub use display::{PsalmDisplay, PsalmRun, PsalmRunKind, PsalmSyllable, PsalmVerse};
#[cfg(feature = "pointing")]
pub use point::{HalfPointing, Pointing, point};
pub use pointed::VersePart;
pub use tone::{Cadence, Slot, Tone, ToneError};
