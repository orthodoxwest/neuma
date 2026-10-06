//! The note map: where each note is drawn, its pitch and its relative duration, for practice
//! and playback tools (DESIGN section 12).

use std::ops::Range;

use crate::display::{LineBox, NoteRef};
use crate::engrave::NoteInfo;
use crate::layout::Layout;
use crate::score::{BarKind, ClefKind, NoteShape};

#[derive(Clone, Debug, PartialEq)]
pub struct NoteMap {
    pub notes: Vec<MappedNote>,
    pub pauses: Vec<Pause>,
    /// Each line's box, staff center and lyric baseline, in output units.
    pub lines: Vec<LineBox>,
    /// The whole timeline's length, in weight units.
    pub duration: f32,
}

/// Why the singing pauses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PauseKind {
    Bar(BarKind),
    /// A `*` in the text: the mediant of a psalm verse, or where the soloist's intonation
    /// ends in other chants.
    Mediant,
    /// The flex `†`.
    Flex,
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
    /// When the note starts and how long it lasts, in weight units: notes and pauses laid
    /// end to end with the caller's weights.
    pub start: f32,
    pub duration: f32,
    pub syllable_text: String,
    pub word: u32,
    /// The vowel the engine centered the syllable on.
    pub vowel: Option<char>,
    pub shape: NoteShape,
    pub liquescent: bool,
    pub quilisma: bool,
    /// The note's syllable has an acute accent in the source.
    pub accent: bool,
    /// The first note of its syllable.
    pub new_syllable: bool,
    /// Inferred: part of a run of three or more single-note syllables on one pitch, which a
    /// player can time at speech pace.
    pub recitation: bool,
    /// Phrase counters: `verse` advances after each full or double bar, and `half` is 1
    /// after the verse's mediant `*`, else 0.
    pub verse: u32,
    pub half: u8,
}

/// A pause before note `before_note` (`notes.len()` means after the last note).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pause {
    pub before_note: u32,
    pub kind: PauseKind,
    pub weight: f32,
    /// When the pause starts, in weight units.
    pub start: f32,
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
    pub mediant: f32,
    pub flex: f32,
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
        mediant: 2.0,
        flex: 1.0,
    };

    pub(crate) fn of_note(&self, info: &NoteInfo) -> f32 {
        if info.morae > 0 {
            self.mora
        } else if info.episema {
            self.episema
        } else {
            self.note
        }
    }

    fn pause(&self, kind: PauseKind) -> f32 {
        let kind = match kind {
            PauseKind::Bar(b) => b,
            PauseKind::Mediant => return self.mediant,
            PauseKind::Flex => return self.flex,
        };
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

/// A note's diatonic degree and semitones above its clef's do, alterations applied.
pub(crate) fn pitch(info: &NoteInfo) -> (i32, i16) {
    let reference = info.clef.position() as i32;
    let degree = info.position as i32 - reference + if info.clef.kind == ClefKind::Fa { 3 } else { 0 };
    (degree, semitones(degree) + info.alteration as i16)
}

/// The score's pauses with their weights. A mediant or flex is the pause at its bar: the
/// bar right after it adds no time.
pub(crate) fn timed_pauses(marks: &[(u32, PauseKind)], weights: &Weights) -> Vec<Pause> {
    let mut pauses: Vec<Pause> = marks
        .iter()
        .map(|&(before, kind)| Pause {
            before_note: before,
            kind,
            weight: weights.pause(kind),
            start: 0.0,
        })
        .collect();
    for i in 1..pauses.len() {
        let mark = matches!(pauses[i - 1].kind, PauseKind::Mediant | PauseKind::Flex);
        if mark && matches!(pauses[i].kind, PauseKind::Bar(_)) && pauses[i - 1].before_note == pauses[i].before_note {
            pauses[i].weight = 0.0;
        }
    }
    pauses
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
            let (degree, semitones) = pitch(info);
            let weight = weights.of_note(info);
            let syllable_text = eng.syllable_text.get(info.syllable as usize).cloned().unwrap_or_default();
            let accent = syllable_text.chars().any(|c| "áéíóúýǽÁÉÍÓÚÝǼ\u{0301}".contains(c));
            let new_syllable = id == 0 || eng.notes[id - 1].syllable != info.syllable;
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
                semitones,
                weight,
                start: 0.0,
                duration: weight,
                syllable_text,
                word: eng.syllable_word.get(info.syllable as usize).copied().unwrap_or(0),
                vowel: info.vowel,
                shape: info.shape,
                liquescent: info.liquescent,
                quilisma: info.shape == NoteShape::Quilisma,
                accent,
                new_syllable,
                recitation: false,
                verse: 0,
                half: 0,
            });
        }
        let paused: Vec<u32> = eng.pauses.iter().map(|&(before, _)| before).collect();
        mark_recitations(&mut notes, &paused);

        // Lay notes and pauses end to end, and count phrases.
        let mut pauses = timed_pauses(&eng.pauses, weights);
        let mut t = 0.0f32;
        let mut verse = 0u32;
        let mut half = 0u8;
        let mut p = 0;
        for i in 0..=notes.len() {
            let id = notes.get(i).map_or(u32::MAX, |n| n.id);
            while p < pauses.len() && pauses[p].before_note <= id {
                pauses[p].start = t;
                t += pauses[p].weight;
                // A full bar right after the mediant belongs to it and doesn't end the verse.
                let after_mediant = p > 0 && pauses[p - 1].kind == PauseKind::Mediant && pauses[p - 1].before_note == pauses[p].before_note;
                match pauses[p].kind {
                    PauseKind::Bar(BarKind::Maior | BarKind::DottedMaior) if after_mediant => {}
                    PauseKind::Bar(BarKind::Maior | BarKind::DottedMaior | BarKind::Finalis) => {
                        verse += 1;
                        half = 0;
                    }
                    PauseKind::Mediant => half = 1,
                    _ => {}
                }
                p += 1;
            }
            if let Some(n) = notes.get_mut(i) {
                n.start = t;
                n.verse = verse;
                n.half = half;
                t += n.duration;
            }
        }
        let lines = self
            .lines
            .iter()
            .map(|l| LineBox {
                top: l.top * s,
                bottom: l.bottom * s,
                staff: l.staff * s,
                baseline: l.baseline * s,
            })
            .collect();
        NoteMap {
            notes,
            pauses,
            lines,
            duration: t,
        }
    }
}

/// Marks runs of three or more consecutive single-note syllables on one pitch, not counting
/// across a pause (`paused` holds the note ids pauses come before, in order).
fn mark_recitations(notes: &mut [MappedNote], paused: &[u32]) {
    let single = |i: usize, notes: &[MappedNote]| notes[i].new_syllable && notes.get(i + 1).is_none_or(|n| n.new_syllable);
    let mut i = 0;
    while i < notes.len() {
        let mut j = i;
        while j < notes.len()
            && single(j, notes)
            && notes[j].semitones == notes[i].semitones
            && (j == i || paused.binary_search(&notes[j].id).is_err())
        {
            j += 1;
        }
        if j - i >= 3 {
            for n in &mut notes[i..j] {
                n.recitation = true;
            }
        }
        i = j.max(i + 1);
    }
}

impl NoteMap {
    /// The note under (`x`, `y`) in output units: one whose box contains the point, else the
    /// nearest note on the line the point falls in. `None` outside every line.
    pub fn note_at(&self, x: f32, y: f32) -> Option<NoteRef> {
        if let Some(n) = self
            .notes
            .iter()
            .find(|n| (x - n.x).abs() <= n.w / 2.0 && (y - n.y).abs() <= n.h / 2.0)
        {
            return Some(n.id);
        }
        let line = self.lines.iter().position(|l| y >= l.top && y <= l.bottom)? as u32;
        self.notes
            .iter()
            .filter(|n| n.line == line)
            .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .map(|n| n.id)
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
