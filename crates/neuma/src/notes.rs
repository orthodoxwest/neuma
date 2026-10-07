//! The timeline: where each note is drawn, its pitch and its relative duration, for practice
//! and playback tools (DESIGN section 12).

use std::ops::Range;

use crate::display::{LineBox, NoteRef};
use crate::engrave::NoteInfo;
use crate::layout::Layout;

use crate::score::{BarKind, ClefKind, NoteShape};

/// Every note of a layout in singing order, with when it starts and how long it lasts, and the
/// pauses between them. Build it with [`Layout::timeline`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Timeline {
    /// By note id; notes on lines a `max_lines` layout leaves out are missing.
    pub notes: Vec<TimelineNote>,
    pub pauses: Vec<Pause>,
    /// Each line's box, staff center and lyric baseline, in output units.
    pub lines: Vec<LineBox>,
    /// The whole timeline's length, in weight units.
    pub duration: f32,
}

/// Why the singing pauses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PauseKind {
    /// A bar.
    Bar(BarKind),
    /// A `*` in the text: the mediant of a psalm verse, or where the soloist's intonation
    /// ends in other chants.
    Mediant,
    /// The flex `†`.
    Flex,
}

/// One note of a [`Timeline`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TimelineNote {
    /// The note's id: its index in the score, stable across layouts.
    pub id: NoteRef,
    /// The syllable's index in the score.
    pub syllable: u32,
    /// The line it is drawn on, from 0.
    pub line: u32,
    /// The notehead's center, in output units.
    pub cx: f32,
    pub cy: f32,
    /// The notehead's width and height, in output units.
    pub w: f32,
    pub h: f32,
    /// The note's source span, in UTF-8 bytes.
    pub span: Range<usize>,
    /// The note's staff position: 0 is the bottom line, 1 the space above it, and so on.
    pub staff_position: i32,
    /// Diatonic steps above the clef's do (a fa clef's fa is degree 3).
    pub degree: i32,
    /// Semitones above the clef's do, alterations applied.
    pub semitones: i32,
    /// When the note starts and how long it lasts, in weight units: notes and pauses laid
    /// end to end with the caller's weights. The duration is the note's weight (see
    /// [`Weights`]); pauses are timed separately.
    pub start: f32,
    pub duration: f32,
    pub syllable_text: String,
    pub word: u32,
    /// The vowel the engine centered the syllable on.
    pub vowel: Option<char>,
    pub shape: NoteShape,
    pub liquescent: bool,
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
    pub half: u32,
}

/// A pause before note `before_note` (`notes.len()` means after the last note).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Pause {
    pub before_note: u32,
    pub kind: PauseKind,
    /// How long the pause lasts, in weight units: the [`Weights`] value for its kind, or 0
    /// for a bar that only closes the mediant or flex just before it.
    pub duration: f32,
    /// When the pause starts, in weight units.
    pub start: f32,
}

/// Relative durations: multipliers per sign, in weight units. Not beats; tools choose the
/// tempo. A value that is negative or not finite keeps the default, and values are capped at
/// [`Weights::MAX`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Weights {
    /// A plain note.
    pub note: f32,
    /// A dotted note.
    pub mora: f32,
    /// A note with a horizontal episema.
    pub episema: f32,
    /// The virgula `` ` `` and the minimis bar `^`.
    pub virgula: f32,
    /// The quarter bar `,` (divisio minima).
    pub quarter: f32,
    /// The half bar `;` (divisio minor) and the Dominican bars.
    pub half: f32,
    /// The full bar `:` (divisio maior).
    pub full: f32,
    /// The double bar `::` (divisio finalis).
    pub double: f32,
    /// The mediant `*` of a psalm verse.
    pub mediant: f32,
    /// The flex `†`.
    pub flex: f32,
}

crate::setters!(Weights {
    note: f32 => with_note,
    mora: f32 => with_mora,
    episema: f32 => with_episema,
    virgula: f32 => with_virgula,
    quarter: f32 => with_quarter,
    half: f32 => with_half,
    full: f32 => with_full,
    double: f32 => with_double,
    mediant: f32 => with_mediant,
    flex: f32 => with_flex,
});

impl Weights {
    /// Solesmes-style reading: every note one pulse, a dotted note two, an episema a little
    /// longer, and pauses growing with the bar.
    pub const SOLESMES: Weights = Weights {
        note: 1.0,
        mora: 2.0,
        episema: 1.5,
        virgula: 0.5,
        quarter: 0.5,
        half: 1.0,
        full: 2.0,
        double: 3.0,
        mediant: 2.0,
        flex: 1.0,
    };

    /// The largest weight a timeline uses; larger ones are capped to it.
    pub const MAX: f32 = 1000.0;

    /// These weights as a timeline uses them: each that is negative or not finite replaced by
    /// the default, and each capped at [`Weights::MAX`].
    #[must_use]
    pub fn sanitized(self) -> Weights {
        let d = Weights::SOLESMES;
        let keep = |v: f32, default: f32| if v.is_finite() && v >= 0.0 { v.min(Weights::MAX) } else { default };
        Weights {
            note: keep(self.note, d.note),
            mora: keep(self.mora, d.mora),
            episema: keep(self.episema, d.episema),
            virgula: keep(self.virgula, d.virgula),
            quarter: keep(self.quarter, d.quarter),
            half: keep(self.half, d.half),
            full: keep(self.full, d.full),
            double: keep(self.double, d.double),
            mediant: keep(self.mediant, d.mediant),
            flex: keep(self.flex, d.flex),
        }
    }

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
            BarKind::Quarter => self.quarter,
            BarKind::Half | BarKind::Dominican(_) => self.half,
            BarKind::Full | BarKind::DottedFull => self.full,
            BarKind::Double => self.double,
        }
    }
}

impl Default for Weights {
    fn default() -> Weights {
        Weights::SOLESMES
    }
}

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Semitones above do for `degree` diatonic steps above do.
fn semitones(degree: i32) -> i32 {
    let octave = degree.div_euclid(7);
    MAJOR[degree.rem_euclid(7) as usize] + 12 * octave
}

/// A note's diatonic degree and semitones above its clef's do, alterations applied.
pub(crate) fn pitch(info: &NoteInfo) -> (i32, i32) {
    let reference = info.clef.position() as i32;
    let degree = info.position as i32 - reference + if info.clef.kind == ClefKind::Fa { 3 } else { 0 };
    (degree, semitones(degree) + i32::from(info.alteration))
}

/// The score's pauses with their weights. A mediant or flex is the pause at its bar: the
/// bar right after it adds no time.
pub(crate) fn timed_pauses(marks: &[(u32, PauseKind)], weights: &Weights) -> Vec<Pause> {
    let mut pauses: Vec<Pause> = marks
        .iter()
        .map(|&(before, kind)| Pause {
            before_note: before,
            kind,
            duration: weights.pause(kind),
            start: 0.0,
        })
        .collect();
    for i in 1..pauses.len() {
        let mark = matches!(pauses[i - 1].kind, PauseKind::Mediant | PauseKind::Flex);
        if mark && matches!(pauses[i].kind, PauseKind::Bar(_)) && pauses[i - 1].before_note == pauses[i].before_note {
            pauses[i].duration = 0.0;
        }
    }
    pauses
}

impl Layout {
    /// The timeline of this layout's notes, timed with the default [`Weights`].
    #[must_use]
    pub fn timeline(&self) -> Timeline {
        self.timeline_with(&Weights::default())
    }

    /// The timeline of this layout's notes, timed with `weights` (sanitized as
    /// [`Weights::sanitized`] says).
    #[must_use]
    pub fn timeline_with(&self, weights: &Weights) -> Timeline {
        let weights = &weights.sanitized();
        let eng = &*self.eng;
        let s = self.scale;
        // (line, x, y, w, h) of each placed note.
        type Placed = Option<(u32, f32, f32, f32, f32)>;
        let mut placed: Vec<Placed> = vec![None; eng.notes.len()];
        for (li, line) in self.lines.iter().enumerate() {
            for (i, seg) in eng.segments[line.first..=line.last].iter().enumerate() {
                for h in &seg.heads {
                    if let Some(slot) = placed.get_mut(h.note as usize) {
                        let (x0, y0) = (line.xs[i], line.staff);
                        *slot = Some((li as u32, (x0 + h.x) * s, (y0 + h.y) * s, h.w * s, h.h * s));
                    }
                }
            }
        }
        // A layout cut short (`max_lines`) still marks recitations as the whole score does,
        // so the notes after its last line join the marking and are then dropped.
        let drawn = self.lines.last().map_or(0, |l| l.last + 1);
        let truncated = drawn < eng.segments.len();
        let last_placed = placed.iter().rposition(Option::is_some);
        let mut notes = Vec::with_capacity(eng.notes.len());
        let mut kept = 0;
        for (id, info) in eng.notes.iter().enumerate() {
            let after = truncated && last_placed.is_none_or(|l| id > l);
            let Some((line, x, y, w, h)) = placed[id].or(after.then_some((0, 0.0, 0.0, 0.0, 0.0))) else {
                continue;
            };
            if !after {
                kept += 1;
            }
            let (degree, semitones) = pitch(info);
            let syllable_text = eng.syllable_text.get(info.syllable as usize).cloned().unwrap_or_default();
            let accent = syllable_text.chars().any(|c| "áéíóúýǽÁÉÍÓÚÝǼ\u{0301}".contains(c));
            let new_syllable = id == 0 || eng.notes[id - 1].syllable != info.syllable;
            notes.push(TimelineNote {
                id: id as u32,
                syllable: info.syllable,
                line,
                cx: x,
                cy: y,
                w,
                h,
                span: info.span.clone(),
                staff_position: i32::from(info.position),
                degree,
                semitones,
                start: 0.0,
                duration: weights.of_note(info),
                syllable_text,
                word: eng.syllable_word.get(info.syllable as usize).copied().unwrap_or(0),
                vowel: info.vowel,
                shape: info.shape,
                liquescent: info.liquescent,
                accent,
                new_syllable,
                recitation: false,
                verse: 0,
                half: 0,
            });
        }
        let paused: Vec<u32> = eng.pauses.iter().map(|&(before, _)| before).collect();
        mark_recitations(&mut notes, &paused);
        notes.truncate(kept);

        // Lay notes and pauses end to end, and count phrases.
        let mut pauses = timed_pauses(&eng.pauses, weights);
        // A layout cut short keeps only the pauses drawn on its lines.
        if truncated {
            let mut segs = eng.pause_segments.iter();
            pauses.retain(|_| segs.next().is_some_and(|&s| s < drawn));
        }
        let mut t = 0.0f32;
        let mut verse = 0u32;
        let mut half = 0u32;
        let mut p = 0;
        for i in 0..=notes.len() {
            let id = notes.get(i).map_or(u32::MAX, |n| n.id);
            while p < pauses.len() && pauses[p].before_note <= id {
                pauses[p].start = t;
                t += pauses[p].duration;
                // A full bar right after the mediant belongs to it and doesn't end the verse.
                let after_mediant = p > 0 && pauses[p - 1].kind == PauseKind::Mediant && pauses[p - 1].before_note == pauses[p].before_note;
                match pauses[p].kind {
                    PauseKind::Bar(BarKind::Full | BarKind::DottedFull) if after_mediant => {}
                    PauseKind::Bar(BarKind::Full | BarKind::DottedFull | BarKind::Double) => {
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
        Timeline {
            notes,
            pauses,
            lines,
            duration: t,
        }
    }
}

/// Marks runs of three or more consecutive single-note syllables on one pitch, not counting
/// across a pause (`paused` holds the note ids pauses come before, in order).
fn mark_recitations(notes: &mut [TimelineNote], paused: &[u32]) {
    let single = |i: usize, notes: &[TimelineNote]| notes[i].new_syllable && notes.get(i + 1).is_none_or(|n| n.new_syllable);
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

impl Timeline {
    /// The note sounding at time `t` (in weight units), or `None` during a pause or outside
    /// the timeline: what a player highlights on each frame. A binary search.
    #[must_use]
    pub fn note_at_time(&self, t: f32) -> Option<&TimelineNote> {
        let i = self.notes.partition_point(|n| n.start <= t).checked_sub(1)?;
        let n = &self.notes[i];
        (t < n.start + n.duration).then_some(n)
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
