//! [`PsalmChant`]: psalm text set to a tone and engraved, kept in step as the text changes;
//! and [`AnyChant`], a chant of either source.

use std::ops::Deref;

use neuma::{Chant, ChantOptions, Diagnostic};

use crate::{PsalmNote, PsalmOptions, Tone, psalm};

/// Psalm text set to a tone and engraved as a [`Chant`] whose source is the text: its hit
/// tests, `elements_at` and timeline answer in the verse a reader sees, and its diagnostics
/// start with the setting's (such as `point::unsure`). [`update`](Self::update) sets new text
/// to the same tone with the same options, and [`notes`](Self::notes) are always the current
/// text's, each note's place in the tone. It keeps the setting's score only as its chant's.
///
/// It reads as its chant (`Deref<Target = Chant>`): lay it out, hit-test it and read its
/// diagnostics as a [`Chant`]'s. Only changing it goes through `PsalmChant`, so the score
/// and the setting never part.
///
/// ```
/// use neuma::ChantOptions;
/// use neuma_tones::{PsalmChant, PsalmOptions, ToneRole, Tone};
///
/// let text = "O praise the Lord, all ye heathen * praise him, all ye nations.";
/// let tone = Tone::named("8.G").unwrap();
/// let mut chant = PsalmChant::new(text, tone, &PsalmOptions::default(), ChantOptions::default());
/// let first = &chant.layout(600.0).timeline().notes[0];
/// assert_eq!(&chant.source()[first.span.clone()], "O");
/// assert_eq!(chant.notes()[0].role, ToneRole::Intonation);
/// assert!(chant.update("Praise the Lord * all ye nations."));
/// let first = &chant.layout(600.0).timeline().notes[0];
/// assert_eq!(&chant.source()[first.span.clone()], "Praise");
/// ```
#[derive(Debug)]
pub struct PsalmChant {
    chant: Chant,
    tone: Tone,
    options: PsalmOptions,
    /// The setting but for its score, which is the chant's, its text, which is the chant's
    /// source, and its diagnostics, which start the chant's.
    gabc: String,
    notes: Vec<PsalmNote>,
    setting_diagnostics: usize,
}

impl PsalmChant {
    /// Sets `text` to `tone` with `psalm`'s options, as [`psalm`] does, and engraves it with
    /// `options`.
    #[must_use]
    pub fn new(text: &str, tone: &Tone, psalm_options: &PsalmOptions, options: ChantOptions) -> PsalmChant {
        let setting = psalm(text, tone, psalm_options);
        let setting_diagnostics = setting.diagnostics.len();
        let chant = Chant::from_score(setting.score, text, setting.diagnostics, options);
        PsalmChant {
            chant,
            tone: tone.clone(),
            options: psalm_options.clone(),
            gabc: setting.gabc,
            notes: setting.notes,
            setting_diagnostics,
        }
    }

    /// Sets new text to the same tone and engraves it again, only around what changed, as an
    /// editor of the text does on each change. Layouts made before keep showing the old
    /// setting. Returns whether anything changed: `false` when `text` is the current text.
    pub fn update(&mut self, text: &str) -> bool {
        if text == self.chant.source() {
            return false;
        }
        let setting = psalm(text, &self.tone, &self.options);
        self.setting_diagnostics = setting.diagnostics.len();
        self.gabc = setting.gabc;
        self.notes = setting.notes;
        self.chant.update_score(setting.score, text, setting.diagnostics)
    }

    /// Engraves the setting again with new options; see [`Chant::set_options`].
    pub fn set_options(&mut self, options: ChantOptions) -> bool {
        self.chant.set_options(options)
    }

    /// Every note's place in the tone: `notes()[i]` is note `i` of the chant, as
    /// [`PsalmSetting::notes`](crate::PsalmSetting::notes).
    #[must_use]
    pub fn notes(&self) -> &[PsalmNote] {
        &self.notes
    }

    /// The setting as GABC, as [`PsalmSetting::gabc`](crate::PsalmSetting::gabc).
    #[must_use]
    pub fn gabc(&self) -> &str {
        &self.gabc
    }

    /// The problems setting the text found (such as `point::unsure`), with spans in the
    /// text, as [`PsalmSetting::diagnostics`](crate::PsalmSetting::diagnostics). The chant's
    /// [`diagnostics`](Chant::diagnostics) start with them, then add the engraving's.
    #[must_use]
    pub fn setting_diagnostics(&self) -> &[Diagnostic] {
        &self.chant.diagnostics()[..self.setting_diagnostics]
    }

    /// The tone the text is set to.
    #[must_use]
    pub fn tone(&self) -> &Tone {
        &self.tone
    }

    /// The options the text is set with.
    #[must_use]
    pub fn psalm_options(&self) -> &PsalmOptions {
        &self.options
    }

    /// The chant, as `Deref` gives it. Its source is the text, and its score the setting's.
    #[must_use]
    pub fn chant(&self) -> &Chant {
        &self.chant
    }

    /// The chant alone, whose [`update`](Chant::update) then reads GABC.
    #[must_use]
    pub fn into_chant(self) -> Chant {
        self.chant
    }
}

impl Deref for PsalmChant {
    type Target = Chant;

    fn deref(&self) -> &Chant {
        &self.chant
    }
}

/// A chant of any source the bindings and apps take: GABC, or psalm text set to a tone.
/// It reads as its [`Chant`] (`Deref<Target = Chant>`), and [`update`](Self::update) reads
/// the new source as the chant was made from: GABC for a GABC chant, psalm text for a psalm.
///
/// ```
/// use neuma::{Chant, ChantOptions, Diagnostic};
/// use neuma_tones::{AnyChant, PsalmChant, PsalmOptions, Tone};
///
/// let tone = Tone::named("8.G").unwrap();
/// let mut chants = [
///     AnyChant::from(Chant::new("(c4) a(g) (::)")),
///     AnyChant::from(PsalmChant::new("Praise him * all ye nations.", tone, &PsalmOptions::default(), ChantOptions::default())),
/// ];
/// let made = chants.each_ref().map(|c| c.version());
/// assert!(chants[0].update("(c4) a(h) (::)"));
/// assert!(chants[1].update("Praise him * all ye peoples."));
/// assert!(chants.iter().zip(made).all(|(c, v)| c.version() > v && c.layout(600.0).line_count() == 1));
/// assert_eq!(chants[1].psalm().unwrap().notes().len(), chants[1].layout(600.0).timeline().notes.len());
/// ```
#[derive(Debug)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)] // one per chant, and rarely moved
pub enum AnyChant {
    /// A chant read from GABC.
    Gabc(Chant),
    /// Psalm text set to a tone.
    Psalm(PsalmChant),
}

impl AnyChant {
    /// Replaces the source, read as the chant was made from (GABC, or psalm text set to the
    /// same tone); see [`Chant::update`]. Returns whether anything changed.
    pub fn update(&mut self, source: &str) -> bool {
        match self {
            AnyChant::Gabc(c) => c.update(source),
            AnyChant::Psalm(p) => p.update(source),
        }
    }

    /// Engraves again with new options; see [`Chant::set_options`].
    pub fn set_options(&mut self, options: ChantOptions) -> bool {
        match self {
            AnyChant::Gabc(c) => c.set_options(options),
            AnyChant::Psalm(p) => p.set_options(options),
        }
    }

    /// The chant, as `Deref` gives it.
    #[must_use]
    pub fn chant(&self) -> &Chant {
        match self {
            AnyChant::Gabc(c) => c,
            AnyChant::Psalm(p) => p,
        }
    }

    /// The psalm, for a chant set from one.
    #[must_use]
    pub fn psalm(&self) -> Option<&PsalmChant> {
        match self {
            AnyChant::Psalm(p) => Some(p),
            AnyChant::Gabc(_) => None,
        }
    }
}

impl Deref for AnyChant {
    type Target = Chant;

    fn deref(&self) -> &Chant {
        self.chant()
    }
}

impl From<Chant> for AnyChant {
    fn from(chant: Chant) -> AnyChant {
        AnyChant::Gabc(chant)
    }
}

impl From<PsalmChant> for AnyChant {
    fn from(chant: PsalmChant) -> AnyChant {
        AnyChant::Psalm(chant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_follows_the_text() {
        let tone = Tone::named("8.G").unwrap();
        let text = "Bléssed is he * that cómeth.";
        let mut c = PsalmChant::new(text, tone, &PsalmOptions::default(), ChantOptions::default());
        let notes = c.notes().len();
        assert!(!c.update(text));
        assert!(c.update("Bléssed is he * that cómeth, and is."));
        assert!(c.notes().len() > notes);
        assert!(c.gabc().contains("is.(g)"), "{}", c.gabc());
        assert_eq!(c.layout(600.0).timeline().notes.len(), c.notes().len());
        let fresh = psalm(c.source(), tone, &PsalmOptions::default());
        assert_eq!(c.score(), &fresh.score);
        assert_eq!(c.setting_diagnostics(), fresh.diagnostics.as_slice());
        assert!(!c.set_options(ChantOptions::default()));
        // Unpointed text: the setting's diagnostics come first.
        assert!(c.update("Praise the Lord * all ye nations."));
        assert!(c.setting_diagnostics().iter().any(|d| d.code == "point::unsure") || c.setting_diagnostics().is_empty());
        assert!(c.diagnostics().starts_with(c.setting_diagnostics()));
    }
}
