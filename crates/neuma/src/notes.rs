//! The note map: where each note is drawn, its pitch and its relative duration, for practice
//! and playback tools (DESIGN section 12).

use std::ops::Range;

use crate::display::NoteRef;
use crate::layout::Layout;
use crate::score::{BarKind, ClefKind, NoteShape};

#[derive(Clone, Debug, PartialEq)]
pub struct NoteMap {
    pub notes: Vec<MappedNote>,
    pub pauses: Vec<Pause>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappedNote {
    pub id: NoteRef,
    pub syllable: u32,
    pub line: u32,
    /// Notehead center, output units.
    pub x: f32,
    pub y: f32,
    /// Notehead box, output units.
    pub w: f32,
    pub h: f32,
    /// Source span in GABC bytes.
    pub span: Range<usize>,
    pub staff_position: i8,
    /// Diatonic steps above the clef's do (a fa clef's fa is degree 3).
    pub degree: i32,
    /// Semitones above the clef's do, alterations applied.
    pub semitones: i16,
    pub weight: f32,
    pub syllable_text: String,
    pub word: u32,
    /// The vowel the engine centered the syllable on.
    pub vowel: Option<char>,
    pub shape: NoteShape,
    pub liquescent: bool,
    pub quilisma: bool,
}

/// A pause before note `before_note` (`notes.len()` means after the last note).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pause {
    pub before_note: u32,
    pub kind: BarKind,
    pub weight: f32,
}

/// Relative durations: multipliers per sign. Not beats; tools choose the tempo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weights {
    pub note: f32,
    pub mora: f32,
    pub episema: f32,
    pub virgula: f32,
    pub minima: f32,
    pub minor: f32,
    pub maior: f32,
    pub finalis: f32,
}

impl Weights {
    /// Solesmes-style reading: every note one pulse, a dotted note two, an episema a little
    /// longer, and pauses growing with the bar.
    pub const SOLESMES: Weights = Weights {
        note: 1.0,
        mora: 2.0,
        episema: 1.5,
        virgula: 0.5,
        minima: 0.5,
        minor: 1.0,
        maior: 2.0,
        finalis: 3.0,
    };

    fn bar(&self, kind: BarKind) -> f32 {
        match kind {
            BarKind::Virgula | BarKind::Minimis => self.virgula,
            BarKind::Minima => self.minima,
            BarKind::Minor | BarKind::Dominican(_) => self.minor,
            BarKind::Maior | BarKind::DottedMaior => self.maior,
            BarKind::Finalis => self.finalis,
        }
    }
}

impl Default for Weights {
    fn default() -> Weights {
        Weights::SOLESMES
    }
}

const MAJOR: [i16; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Semitones above do for `degree` diatonic steps above do.
fn semitones(degree: i32) -> i16 {
    let octave = degree.div_euclid(7);
    MAJOR[degree.rem_euclid(7) as usize] + 12 * octave as i16
}

impl Layout<'_> {
    pub fn notes(&self, weights: &Weights) -> NoteMap {
        let eng = self.eng;
        let s = self.scale;
        // (line, x, y, w, h) of each placed note.
        type Placed = Option<(u32, f32, f32, f32, f32)>;
        let mut placed: Vec<Placed> = vec![None; eng.notes.len()];
        for (li, line) in self.lines.iter().enumerate() {
            for (i, seg) in eng.segments[line.first..=line.last].iter().enumerate() {
                for h in &seg.heads {
                    if let Some(slot) = placed.get_mut(h.note as usize) {
                        *slot = Some((li as u32, (line.xs[i] + h.x) * s, (line.staff + h.y) * s, h.w * s, h.h * s));
                    }
                }
            }
        }
        let mut notes = Vec::with_capacity(eng.notes.len());
        for (id, info) in eng.notes.iter().enumerate() {
            let Some((line, x, y, w, h)) = placed[id] else { continue };
            let reference = info.clef.position() as i32;
            let degree = info.position as i32 - reference + if info.clef.kind == ClefKind::Fa { 3 } else { 0 };
            let weight = if info.morae > 0 {
                weights.mora
            } else if info.episema {
                weights.episema
            } else {
                weights.note
            };
            notes.push(MappedNote {
                id: id as u32,
                syllable: info.syllable,
                line,
                x,
                y,
                w,
                h,
                span: info.span.clone(),
                staff_position: info.position,
                degree,
                semitones: semitones(degree) + info.alteration as i16,
                weight,
                syllable_text: eng.syllable_text.get(info.syllable as usize).cloned().unwrap_or_default(),
                word: eng.syllable_word.get(info.syllable as usize).copied().unwrap_or(0),
                vowel: info.vowel,
                shape: info.shape,
                liquescent: info.liquescent,
                quilisma: info.shape == NoteShape::Quilisma,
            });
        }
        let pauses = eng
            .pauses
            .iter()
            .map(|&(before, kind)| Pause {
                before_note: before,
                kind,
                weight: weights.bar(kind),
            })
            .collect();
        NoteMap { notes, pauses }
    }
}

#[cfg(test)]
mod tests {
    use super::semitones;

    #[test]
    fn scale_degrees() {
        assert_eq!(semitones(0), 0);
        assert_eq!(semitones(3), 5);
        assert_eq!(semitones(7), 12);
        assert_eq!(semitones(-1), -1);
        assert_eq!(semitones(-3), -5);
    }
}
