//! Psalm tones for neuma: pointed English psalm text plus a psalm tone, set as chant.
//!
//! ```
//! use neuma_tones::{Options, Tone, apply_text};
//!
//! let tone = Tone::named("8.G").unwrap();
//! let setting = apply_text(tone, "The Lord is King, and hath put on glorious ap·pá-rel; * \
//!     the Lord hath put on his apparel, and gird·ed him-sélf with strength.", &Options::default());
//! assert!(setting.gabc.contains("pá(k)"));
//! ```

pub mod apply;
pub mod pointed;
pub mod syllable;
pub mod tone;

pub use apply::{Intone, NoteRole, Options, Role, Setting, apply, apply_text};
pub use pointed::{Joint, Part, PartKind, Pointed, Syllable, Verse};
pub use tone::{Cadence, Slot, Tone, ToneError};
