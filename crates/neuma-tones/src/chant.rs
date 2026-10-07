//! [`PsalmChant`]: psalm text set to a tone and engraved, kept in step as the text changes.

use std::ops::Deref;

use neuma::{Chant, ChantOptions};

use crate::{PsalmOptions, PsalmSetting, Tone, psalm};

/// Psalm text set to a tone and engraved as a [`Chant`] whose source is the text: its hit
/// tests, `elements_at` and timeline answer in the verse a reader sees, and its diagnostics
/// start with the setting's (such as `point::unsure`). [`update`](Self::update) sets new text
/// to the same tone with the same options, and [`setting`](Self::setting) is always the
/// current text's, with each note's place in the tone.
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
/// assert_eq!(chant.setting().notes[0].role, ToneRole::Intonation);
/// assert!(chant.update("Praise the Lord * all ye nations."));
/// let first = &chant.layout(600.0).timeline().notes[0];
/// assert_eq!(&chant.source()[first.span.clone()], "Praise");
/// ```
#[derive(Debug)]
pub struct PsalmChant {
    chant: Chant,
    tone: Tone,
    options: PsalmOptions,
    setting: PsalmSetting,
}

impl PsalmChant {
    /// Sets `text` to `tone` with `psalm`'s options, as [`psalm`] does, and engraves it with
    /// `options`.
    #[must_use]
    pub fn new(text: &str, tone: &Tone, psalm_options: &PsalmOptions, options: ChantOptions) -> PsalmChant {
        let setting = psalm(text, tone, psalm_options);
        let chant = Chant::from_score(setting.score.clone(), text, setting.diagnostics.clone(), options);
        PsalmChant {
            chant,
            tone: tone.clone(),
            options: psalm_options.clone(),
            setting,
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
        let changed = self.chant.update_score(setting.score.clone(), text, setting.diagnostics.clone());
        self.setting = setting;
        changed
    }

    /// Engraves the setting again with new options; see [`Chant::set_options`].
    pub fn set_options(&mut self, options: ChantOptions) -> bool {
        self.chant.set_options(options)
    }

    /// The current text's setting: its score, GABC, each note's place in the tone, and the
    /// diagnostics setting it found.
    #[must_use]
    pub fn setting(&self) -> &PsalmSetting {
        &self.setting
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

    /// The chant, as `Deref` gives it.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_follows_the_text() {
        let tone = Tone::named("8.G").unwrap();
        let text = "Bléssed is he * that cómeth.";
        let mut c = PsalmChant::new(text, tone, &PsalmOptions::default(), ChantOptions::default());
        assert_eq!(c.setting().text, text);
        let notes = c.setting().notes.len();
        assert!(!c.update(text));
        assert!(c.update("Bléssed is he * that cómeth, and is."));
        assert!(c.setting().notes.len() > notes);
        assert!(c.setting().gabc.contains("is.(g)"), "{}", c.setting().gabc);
        assert_eq!(c.layout(600.0).timeline().notes.len(), c.setting().notes.len());
        assert_eq!(c.score(), &c.setting().score);
        assert!(!c.set_options(ChantOptions::default()));
    }
}
