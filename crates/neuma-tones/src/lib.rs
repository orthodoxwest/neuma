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
