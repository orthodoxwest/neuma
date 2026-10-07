//! Engraving again after an edit, from the syllables the edit could have changed.
//!
//! Engraving goes through a score's syllables in order, carrying some state from each to the
//! next (the clef, alterations in force, a pending line break). [`EngraveCache`] keeps that
//! state as it stood before every syllable (`Resume`), with the score and the engraving.
//! For the next score it finds the syllables the edit left alone at either end, rolls the
//! engraving back to the first syllable whose result could have changed, and engraves from
//! there until, inside the unchanged end, the state is again what it was before the same
//! syllable last time. From there on the old engraving holds: it is moved over with its
//! indices and source offsets shifted, rather than engraved again.

use super::{Engraving, Pass, Resume, Segment, StyleOptions};
use crate::diag::Diagnostic;
use crate::gabc::Diff;
use crate::score::{Clef, Figure, Lyric, Score, Syllable};
use crate::text::TextMeasure;
use crate::vowel::VowelRules;
use std::ops::Range;
use std::sync::Arc;

/// What [`EngraveCache::engrave`] keeps between engravings, so engraving a score after a small
/// edit costs about the syllables the edit touched rather than the whole score. The result is
/// the same as [`Score::engrave`]'s.
///
/// A cache assumes one [`TextMeasure`] throughout: give it the same one each time.
/// [`crate::Chant`] keeps one.
#[derive(Debug, Default)]
pub(crate) struct EngraveCache {
    last: Option<Kept>,
}

#[derive(Debug)]
struct Kept {
    score: Score,
    /// Shared with the layouts made from it; taken back without a copy once none is alive.
    engraving: Arc<Engraving>,
    /// The state before each syllable, and after the last.
    marks: Vec<Resume>,
    style: StyleOptions,
    /// The measure's hyphen, word space, ascent and descent: a check that it is the same one.
    metrics: [u32; 4],
    rules: VowelRules,
    first_lyric: Option<(usize, Lyric)>,
}

impl EngraveCache {
    /// The last score engraved, and its engraving.
    pub(crate) fn score(&self) -> Option<&Score> {
        self.last.as_ref().map(|k| &k.score)
    }

    /// The last score, for [`crate::gabc::reparse`] to take the syllables an edit left alone
    /// from: engrave next with the [`Diff`] it gives.
    pub(crate) fn score_mut(&mut self) -> Option<&mut Score> {
        self.last.as_mut().map(|k| &mut k.score)
    }

    /// Takes the last score back, forgetting its engraving.
    pub(crate) fn take_score(&mut self) -> Option<Score> {
        self.last.take().map(|k| k.score)
    }

    pub(crate) fn engraving(&self) -> Option<&Arc<Engraving>> {
        self.last.as_ref().map(|k| &k.engraving)
    }

    /// Engraves `score` as [`Score::engrave`] does, reusing the last engraving where the score
    /// is unchanged, and keeps both for next time. `diff`, when given, says which syllables
    /// are unchanged (and the last score's may have been taken, see [`Self::score_mut`]);
    /// otherwise the scores are compared.
    pub(crate) fn engrave(&mut self, score: Score, measure: &dyn TextMeasure, style: &StyleOptions, diff: Option<Diff>) -> &Arc<Engraving> {
        let mut pass = score.pass(measure, style, true);
        let metrics = [pass.hyphen, pass.word_space, pass.ascent, pass.descent].map(f32::to_bits);
        let first_lyric = pass.first_lyric.clone();
        let rules = pass.e.rules.clone();
        // The header reaches the syllables only through the vowel rules, and the initial only
        // through its own syllable; the rest of either is put together afresh at the end.
        let old = self.last.take().filter(|k| {
            k.style == *style
                && k.metrics == metrics
                && k.rules == pass.e.rules
                && k.first_lyric.as_ref().map(|f| f.0) == first_lyric.as_ref().map(|f| f.0)
                && k.engraving.initial.as_ref().map(|i| i.syllable) == pass.initial.as_ref().map(|i| i.syllable)
                && k.marks
                    .first()
                    .is_some_and(|m| k.engraving.diagnostics.get(..m.diagnostics) == Some(&pass.e.sink.items[..]))
        });
        match old {
            Some(old) => resume(&mut pass, &score, old, diff),
            None => {
                for (si, syl) in score.syllables.iter().enumerate() {
                    pass.syllable(&score, si, syl);
                }
            }
        }
        let (engraving, marks) = pass.finish(&score);
        let kept = self.last.insert(Kept {
            score,
            engraving: Arc::new(engraving),
            marks: marks.unwrap_or_default(),
            style: style.clone(),
            metrics,
            rules,
            first_lyric,
        });
        &kept.engraving
    }
}

/// Moves a source offset of the old score to the new one: offsets in the unchanged end move
/// by what the edit added or removed, those before it stay.
#[derive(Clone, Copy)]
struct Shift {
    /// Where the unchanged end starts in the old source.
    cut: usize,
    by: isize,
}

impl Shift {
    fn span(self, r: &Range<usize>) -> Range<usize> {
        if r.start >= self.cut {
            r.start.wrapping_add_signed(self.by)..r.end.wrapping_add_signed(self.by)
        } else {
            r.clone()
        }
    }

    fn clef(self, c: &Clef) -> Clef {
        Clef {
            span: self.span(&c.span),
            ..c.clone()
        }
    }

    fn diagnostic(self, mut d: Diagnostic) -> Diagnostic {
        d.span = self.span(&d.span);
        if let Some(f) = &mut d.fix {
            f.span = self.span(&f.span);
        }
        d
    }
}

/// Whether `new` is `old` moved `by` bytes in the source.
fn same_moved(old: &Syllable, new: &Syllable, by: isize) -> bool {
    let mv = |r: &Range<usize>| r.start.wrapping_add_signed(by)..r.end.wrapping_add_signed(by);
    old.word_start == new.word_start
        && old.no_break_before == new.no_break_before
        && old.no_break_within == new.no_break_within
        && old.euouae == new.euouae
        && mv(&old.span) == new.span
        && old.text == new.text
        && old.notation.len() == new.notation.len()
        && old.notation.iter().zip(&new.notation).all(|(a, b)| match (a, b) {
            (Figure::Clef(a), Figure::Clef(b)) => a.kind == b.kind && a.line == b.line && a.flat == b.flat && mv(&a.span) == b.span,
            (Figure::Note(a), Figure::Note(b)) => {
                let mut a = a.clone();
                a.span = mv(&a.span);
                a == *b
            }
            (Figure::Alteration(a), Figure::Alteration(b)) => {
                let mut a = a.clone();
                a.span = mv(&a.span);
                a == *b
            }
            (Figure::Bar(a), Figure::Bar(b)) => a.kind == b.kind && a.high == b.high && mv(&a.span) == b.span,
            (Figure::Break(a), Figure::Break(b)) => a.justify == b.justify && a.custos == b.custos && mv(&a.span) == b.span,
            (Figure::Custos { position: p, span: s }, Figure::Custos { position: q, span: t }) => p == q && mv(s) == *t,
            (Figure::Space(a), Figure::Space(b)) => a == b,
            (Figure::NoCustos, Figure::NoCustos) => true,
            _ => false,
        })
}

fn has_note(s: &Syllable) -> bool {
    s.notation.iter().any(|f| matches!(f, Figure::Note(_)))
}

/// Engraves `score` in `pass` from where it first differs from the old one, taking the rest
/// from the old engraving once the two meet again.
fn resume(pass: &mut Pass, score: &Score, old: Kept, diff: Option<Diff>) {
    let Kept {
        score: old_score,
        engraving: old_eng,
        marks: mut old_marks,
        ..
    } = old;
    let new = &score.syllables;
    let n = new.len();
    // The syllables the edit left alone at the start, and at the end, where they have moved.
    let (head, tail, m, by) = match diff {
        Some(d) => (d.head, d.tail, d.old_len, d.by),
        None => {
            let prev = &old_score.syllables;
            let m = prev.len();
            let head = new.iter().zip(prev).take_while(|(a, b)| a == b).count();
            let by = match (new.last(), prev.last()) {
                (Some(a), Some(b)) => a.span.start as isize - b.span.start as isize,
                _ => 0,
            };
            let tail = new
                .iter()
                .rev()
                .zip(prev.iter().rev())
                .take(n.min(m) - head)
                .take_while(|(a, b)| same_moved(b, a, by))
                .count();
            (head, tail, m, by)
        }
    };
    drop(old_score);
    // A syllable reads the next one's word start and `<nlba>`, and its last notes, and any
    // custos without a pitch after them, the next note's pitch: start from the syllable before
    // the first changed one, or from the last one with notes before it.
    let start = match new[..head].iter().rposition(has_note) {
        Some(k) => k.min(head - 1),
        None => 0,
    };

    // Roll back to the state before syllable `start`. A layout still alive shares the
    // engraving: then its start is copied, and its end read from it; else it is taken apart.
    let at = old_marks[start].clone();
    let unique = Arc::try_unwrap(old_eng);
    let (old_eng, shared) = match unique {
        Ok(e) => (Some(e), None),
        Err(shared) => (None, Some(shared)),
    };
    let (
        mut segments,
        mut old_segments,
        notes,
        mut old_notes,
        pauses,
        mut old_pauses,
        pause_segments,
        mut old_pause_segments,
        bar_spans,
        mut old_bars,
        diagnostics,
        mut old_diagnostics,
        alt_text,
        old_alt,
        syllable_text,
        mut old_text,
        syllable_spans,
        mut old_spans,
        syllable_word,
        mut old_words,
    );
    match (old_eng, &shared) {
        (Some(e), _) => {
            (segments, old_segments) = Tail::split(e.segments, at.segments);
            (notes, old_notes) = Tail::split(e.notes, at.notes);
            (pauses, old_pauses) = Tail::split(e.pauses, at.pauses);
            (pause_segments, old_pause_segments) = Tail::split(e.pause_segments, at.pauses);
            (bar_spans, old_bars) = Tail::split(e.bar_spans, at.bars);
            (diagnostics, old_diagnostics) = Tail::split(e.diagnostics, at.diagnostics);
            let mut text = e.alt_text;
            old_alt = std::borrow::Cow::Owned(text.split_off(at.alt_text));
            alt_text = text;
            (syllable_text, old_text) = Tail::split(e.syllable_text, start);
            (syllable_spans, old_spans) = Tail::split(e.syllable_spans, start);
            (syllable_word, old_words) = Tail::split(e.syllable_word, start);
        }
        (None, Some(e)) => {
            (segments, old_segments) = Tail::copy(&e.segments, at.segments);
            (notes, old_notes) = Tail::copy(&e.notes, at.notes);
            (pauses, old_pauses) = Tail::copy(&e.pauses, at.pauses);
            (pause_segments, old_pause_segments) = Tail::copy(&e.pause_segments, at.pauses);
            (bar_spans, old_bars) = Tail::copy(&e.bar_spans, at.bars);
            (diagnostics, old_diagnostics) = Tail::copy(&e.diagnostics, at.diagnostics);
            alt_text = e.alt_text[..at.alt_text].to_string();
            old_alt = std::borrow::Cow::Borrowed(&e.alt_text[at.alt_text..]);
            (syllable_text, old_text) = Tail::copy(&e.syllable_text, start);
            (syllable_spans, old_spans) = Tail::copy(&e.syllable_spans, start);
            (syllable_word, old_words) = Tail::copy(&e.syllable_word, start);
        }
        (None, None) => unreachable!("the engraving is either taken or shared"),
    }
    // The last segment before `start` as it stood then; `start` has notes, so changes it
    // the same way again.
    if let (Some(s), Some(a)) = (segments.last_mut(), at.last_after) {
        s.after = a;
    }
    let old_tail_marks = old_marks.split_off(start);

    let p = &mut *pass;
    p.e.segments = segments;
    p.e.notes = notes;
    p.e.pauses = pauses;
    p.e.pause_segments = pause_segments;
    p.e.sink.items = diagnostics;
    p.bar_spans = bar_spans;
    p.alt_text = alt_text;
    p.syllable_text = syllable_text;
    p.syllable_spans = syllable_spans;
    p.syllable_word = syllable_word;
    p.marks = Some(old_marks);
    restore(p, &at, Shift { cut: usize::MAX, by: 0 }, 0);

    // Offsets from the unchanged end on move; an empty span at 0, which stands for none,
    // can't be told from one there, so an end starting at 0 is engraved again.
    let cut = if tail > 0 {
        new[n - tail].span.start.wrapping_add_signed(-by)
    } else {
        0
    };
    let tail = if cut > 0 { tail } else { 0 };
    let shift = Shift { cut, by };
    for i in start..=n {
        // Inside the unchanged end, the old engraving holds from where the state matches.
        if tail > 0 && i >= n - tail {
            let io = i + m - n;
            let was = &old_tail_marks[io - start];
            let now = p.mark();
            if same_state(&now, was, shift) {
                let fin = &old_tail_marks[m - start];
                let base = &old_tail_marks[0];
                let d_seg = now.segments as isize - was.segments as isize;
                let d_note = now.notes as isize - was.notes as isize;
                let d_bar = now.bars as isize - was.bars as isize;
                let d_syl = i as isize - io as isize;
                let d_word = now.word as isize - was.word as isize;
                // What the old syllables after this one made of the segment before it (one the
                // old engraving made after `start`, since that syllable has notes).
                if let Some(last) = p.e.segments.last_mut()
                    && let Some(old) = was.segments.checked_sub(base.segments + 1).and_then(|k| old_segments.get(k))
                {
                    last.after = old.after;
                }
                let take = |v: usize, from: usize| v - from;
                p.e.segments.extend(
                    old_segments
                        .take(take(was.segments, base.segments)..take(fin.segments, base.segments))
                        .map(|s| moved_segment(s, d_syl, d_note, d_bar, shift)),
                );
                p.e.notes.extend(
                    old_notes
                        .take(take(was.notes, base.notes)..take(fin.notes, base.notes))
                        .map(|mut info| {
                            info.syllable = add(info.syllable, d_syl);
                            info.span = shift.span(&info.span);
                            info.clef = shift.clef(&info.clef);
                            info
                        }),
                );
                p.e.pauses.extend(
                    old_pauses
                        .take(take(was.pauses, base.pauses)..take(fin.pauses, base.pauses))
                        .map(|(note, kind)| (add(note, d_note), kind)),
                );
                p.e.pause_segments.extend(
                    old_pause_segments
                        .take(take(was.pauses, base.pauses)..take(fin.pauses, base.pauses))
                        .map(|s| if s == usize::MAX { s } else { s.wrapping_add_signed(d_seg) }),
                );
                p.bar_spans.extend(
                    old_bars
                        .take(take(was.bars, base.bars)..take(fin.bars, base.bars))
                        .map(|r| shift.span(&r)),
                );
                p.e.sink.items.extend(
                    old_diagnostics
                        .take(take(was.diagnostics, base.diagnostics)..take(fin.diagnostics, base.diagnostics))
                        .map(|d| shift.diagnostic(d)),
                );
                p.alt_text
                    .push_str(&old_alt[take(was.alt_text, base.alt_text)..take(fin.alt_text, base.alt_text)]);
                let rest = io - start..m - start;
                p.syllable_text.extend(old_text.take(rest.clone()));
                p.syllable_spans.extend(old_spans.take(rest.clone()).map(|r| shift.span(&r)));
                p.syllable_word.extend(old_words.take(rest).map(|w| add(w, d_word)));
                let marks = p.marks.as_mut().unwrap();
                for r in &old_tail_marks[io - start..m - start] {
                    marks.push(moved_mark(r, &now, was, shift));
                }
                restore(p, fin, shift, d_word);
                return;
            }
        }
        if let Some(syl) = new.get(i) {
            p.syllable(score, i, syl);
        }
    }
}

/// The end of one of the old engraving's lists, from where engraving starts again: taken
/// from it, or read from it while a layout shares it.
enum Tail<'a, T> {
    Taken(Vec<T>),
    Shared(&'a [T]),
}

impl<'a, T: Clone> Tail<'a, T> {
    /// `v`'s first `at`, and the rest.
    fn split(mut v: Vec<T>, at: usize) -> (Vec<T>, Tail<'a, T>) {
        let rest = v.split_off(at);
        (v, Tail::Taken(rest))
    }

    /// A copy of `v`'s first `at`, and the rest, shared.
    fn copy(v: &'a [T], at: usize) -> (Vec<T>, Tail<'a, T>) {
        let (head, rest) = v.split_at(at);
        let mut start = Vec::with_capacity(v.len() + 16);
        start.extend_from_slice(head);
        (start, Tail::Shared(rest))
    }

    fn get(&self, k: usize) -> Option<&T> {
        match self {
            Tail::Taken(v) => v.get(k),
            Tail::Shared(v) => v.get(k),
        }
    }

    /// The items in `r`, taken once.
    fn take(&mut self, r: Range<usize>) -> Box<dyn Iterator<Item = T> + '_> {
        match self {
            Tail::Taken(v) => Box::new(v.drain(r)),
            Tail::Shared(v) => Box::new(v[r].iter().cloned()),
        }
    }
}

fn add(v: u32, d: isize) -> u32 {
    (v as isize + d) as u32
}

/// Whether engraving carries the same into a syllable now as it did into the old one.
fn same_state(now: &Resume, was: &Resume, shift: Shift) -> bool {
    now.alt_empty == was.alt_empty
        && now.clef == shift.clef(&was.clef)
        && now.initial_clef == was.initial_clef.as_ref().map(|c| shift.clef(c))
        && now.alteration == was.alteration
        && now.pending_break == was.pending_break
        && now.nocustos == was.nocustos
        && now.warned_face == was.warned_face
        && now.last_after == was.last_after
}

/// Sets the state carried between syllables from an old mark.
fn restore(p: &mut Pass, r: &Resume, shift: Shift, d_word: isize) {
    p.word = add(r.word, d_word);
    p.e.clef = shift.clef(&r.clef);
    p.e.initial_clef = r.initial_clef.as_ref().map(|c| shift.clef(c));
    p.e.alteration = r.alteration.clone();
    p.pending_break = r.pending_break;
    p.nocustos = r.nocustos;
    p.warned_face = r.warned_face;
}

/// An old mark, after the point where the two engravings meet, for the new one.
fn moved_mark(r: &Resume, now: &Resume, was: &Resume, shift: Shift) -> Resume {
    let mv = |v: usize, a: usize, b: usize| v + a - b;
    Resume {
        segments: mv(r.segments, now.segments, was.segments),
        notes: mv(r.notes, now.notes, was.notes),
        pauses: mv(r.pauses, now.pauses, was.pauses),
        bars: mv(r.bars, now.bars, was.bars),
        diagnostics: mv(r.diagnostics, now.diagnostics, was.diagnostics),
        alt_text: mv(r.alt_text, now.alt_text, was.alt_text),
        word: add(r.word, now.word as isize - was.word as isize),
        alt_empty: r.alt_empty,
        clef: shift.clef(&r.clef),
        initial_clef: r.initial_clef.as_ref().map(|c| shift.clef(c)),
        alteration: r.alteration.clone(),
        pending_break: r.pending_break,
        nocustos: r.nocustos,
        warned_face: r.warned_face,
        last_after: r.last_after,
    }
}

/// An old segment for the new engraving: its syllable, notes and bars renumbered and its
/// clef's source moved. Its body, numbered from the segment's own, is shared as it is.
fn moved_segment(mut s: Segment, d_syl: isize, d_note: isize, d_bar: isize, shift: Shift) -> Segment {
    s.syllable = add(s.syllable, d_syl);
    // A segment without notes or bars keeps a base of 0, as a fresh one has.
    if d_note != 0 && s.has_notes() {
        s.note_base = add(s.note_base, d_note);
    }
    if d_bar != 0 && !s.bars.is_empty() {
        s.bar_base = add(s.bar_base, d_bar);
    }
    s.clef = shift.clef(&s.clef);
    s
}
