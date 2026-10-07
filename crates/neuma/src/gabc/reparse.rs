//! Parsing an edited source again from where the edit is.
//!
//! Reading the body goes through its syllables in order, carrying a little state from each to
//! the next ([`BodyState`]: the lyric styles open, verbatim tags without a closer). A parse
//! kept with [`parse_keeping`](super::parse_keeping) records that state after every syllable,
//! with how far reading it looked ([`BodyMark`]). For an edited source, the syllables read
//! wholly before the edit are kept as they were; reading starts again after the last of them,
//! and stops as soon as, past the edit, a syllable ends where an old one ended (moved by what
//! the edit added or removed) with the same state: from there on the old syllables hold, and
//! are moved over with their source offsets shifted. The result is the same as [`parse`]'s.
//!
//! [`parse`]: super::parse

use std::ops::Range;

use super::{BodyMark, BodyState, LyricState, ParseMarks, Parsed, check_fixes, end_body, find_separator, finish, read_body};
use crate::diag::{Diagnostic, Sink};
use crate::score::{Figure, Score, Syllable};

/// Which syllables an edit left as they were: the first `head`, and the last `tail`, moved
/// `by` bytes; the old score had `old_len`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Diff {
    pub head: usize,
    pub tail: usize,
    pub old_len: usize,
    pub by: isize,
}

/// Moves source offsets of the old source to the new one: from `cut` on, by `by`.
#[derive(Clone, Copy)]
struct Shift {
    cut: usize,
    by: isize,
}

impl Shift {
    fn at(self, p: usize) -> usize {
        if p >= self.cut { p.wrapping_add_signed(self.by) } else { p }
    }

    fn span(self, r: &Range<usize>) -> Range<usize> {
        self.at(r.start)..self.at(r.end)
    }

    fn diagnostic(self, mut d: Diagnostic) -> Diagnostic {
        d.span = self.span(&d.span);
        if let Some(f) = &mut d.fix {
            f.span = self.span(&f.span);
        }
        d
    }

    fn lyric(self, s: &LyricState) -> LyricState {
        let mut s = s.clone();
        for t in &mut s.open {
            t.moved(|p| self.at(p));
        }
        for v in s.verbatim_first.iter_mut().flatten() {
            *v = self.at(*v);
        }
        s
    }

    fn state(self, s: &BodyState) -> BodyState {
        BodyState {
            lyric: self.lyric(&s.lyric),
            unclosed: s.unclosed,
        }
    }

    fn syllable(self, s: &mut Syllable) {
        s.span = self.span(&s.span);
        for f in &mut s.notation {
            match f {
                Figure::Clef(c) => c.span = self.span(&c.span),
                Figure::Note(n) => n.span = self.span(&n.span),
                Figure::Alteration(a) => a.span = self.span(&a.span),
                Figure::Bar(b) => b.span = self.span(&b.span),
                Figure::Break(b) => b.span = self.span(&b.span),
                Figure::Custos { span, .. } => *span = self.span(span),
                Figure::Space(_) | Figure::NoCustos => {}
            }
        }
    }
}

/// Whether `new` reads as `old` moved by `shift`.
fn same_moved(old: &Syllable, new: &Syllable, shift: Shift) -> bool {
    let mut old = old.clone();
    shift.syllable(&mut old);
    old == *new
}

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let n = a.len().min(b.len());
    let mut k = 0;
    // Eight bytes at a time, then byte by byte.
    while k + 8 <= n && a[k..k + 8] == b[k..k + 8] {
        k += 8;
    }
    while k < n && a[k] == b[k] {
        k += 1;
    }
    k
}

fn common_suffix(a: &[u8], b: &[u8], most: usize) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut k = 0;
    while k + 8 <= most && a[n - k - 8..n - k] == b[m - k - 8..m - k] {
        k += 8;
    }
    while k < most && a[n - k - 1] == b[m - k - 1] {
        k += 1;
    }
    k
}

/// Parses `src`, an edit of `old_src`, whose parse was `old` and kept `keep`, reading again
/// only around the edit. The syllables it keeps are moved out of `old`, and `keep` becomes
/// the new parse's. Returns the parse, as [`parse`](super::parse) gives it, and which
/// syllables the edit left alone; `None`, leaving `old` and `keep` as they were, when the
/// edit isn't in the body (or is in its first syllable), or the source has no `%%`.
pub(crate) fn reparse(old_src: &str, old: &mut Score, keep: &mut ParseMarks, src: &str) -> Option<(Parsed, Diff)> {
    let body_start = keep.body_start;
    let (a, b) = (old_src.as_bytes(), src.as_bytes());
    let prefix = common_prefix(a, b);
    // The header, through the `%%` line's end, is unchanged and ends where it did.
    let (_, after) = find_separator(src)?;
    let after = if src[after..].starts_with('\u{feff}') {
        after + '\u{feff}'.len_utf8()
    } else {
        after
    };
    if body_start == 0 || after != body_start || prefix < body_start || !src[..body_start].ends_with('\n') {
        return None;
    }
    if keep.marks.len() != old.syllables.len() {
        return None;
    }
    let suffix = common_suffix(a, b, a.len().min(b.len()) - prefix);
    let (cut_old, cut_new) = (a.len() - suffix, b.len() - suffix);
    let by = b.len() as isize - a.len() as isize;
    let shift = Shift { cut: cut_old, by };
    // The syllables read before the edit stay; reading starts after the last of them.
    let head = keep.marks.partition_point(|m| m.read <= prefix);
    if head == 0 {
        return None;
    }
    let old_marks = std::mem::take(&mut keep.marks);
    let old_syllables = std::mem::take(&mut old.syllables);
    let m = old_syllables.len();
    let mut old_syllables = old_syllables.into_iter();
    let mut syllables: Vec<Syllable> = old_syllables.by_ref().take(head).collect();
    let mut rest: Vec<Syllable> = old_syllables.collect();
    let restart = &old_marks[head - 1];
    let mut st = restart.state.clone();
    let mut sink = Sink {
        items: keep.found[..restart.diagnostics].to_vec(),
    };
    let mut marks: Vec<BodyMark> = old_marks[..head].to_vec();
    // Past the edit, where a syllable ends as an old one did, in the same state.
    let mut met = None;
    let mut meet = |end: usize, now: &BodyState| {
        if end < cut_new {
            return false;
        }
        let old_end = end.wrapping_add_signed(-by);
        let Ok(k) = old_marks.binary_search_by_key(&old_end, |m| m.end) else {
            return false;
        };
        if k < head || shift.state(&old_marks[k].state) != *now {
            return false;
        }
        met = Some(k);
        true
    };
    let from = restart.end - body_start;
    let read = read_body(
        src,
        body_start,
        from,
        &mut st,
        &mut syllables,
        &mut sink,
        Some(&mut marks),
        &mut meet,
    );
    let fresh = syllables.len() - head;
    // The old syllables from `old_from` on hold, moved; the diagnostics before `found` are
    // those the body's reading found.
    let (old_from, found) = match (read, met) {
        (None, Some(k)) => {
            // The old syllables after `k`, their marks and diagnostics, moved.
            let base = &old_marks[k];
            let d_diag = sink.items.len() as isize - base.diagnostics as isize;
            sink.items
                .extend(keep.found[base.diagnostics..].iter().cloned().map(|d| shift.diagnostic(d)));
            let mut read = marks.last().map_or(0, |m| m.read);
            for o in &old_marks[k + 1..] {
                read = read.max(shift.at(o.read));
                marks.push(BodyMark {
                    end: shift.at(o.end),
                    read,
                    diagnostics: o.diagnostics.wrapping_add_signed(d_diag),
                    state: shift.state(&o.state),
                });
            }
            let found = sink.items.len();
            sink.items.extend(keep.end.iter().cloned().map(|d| shift.diagnostic(d)));
            let moved = rest.split_off(k + 1 - head);
            syllables.extend(moved.into_iter().map(|mut s| {
                shift.syllable(&mut s);
                s
            }));
            (k + 1, found)
        }
        (Some(left), _) => {
            let found = sink.items.len();
            end_body(src, body_start, st, left, &mut sink);
            (m, found)
        }
        (None, None) => unreachable!("reading stops only where it met the old syllables"),
    };
    // The old syllables `head..old_from` were read again as the `fresh` new ones after `head`;
    // some at either end may have come out the same, and are left alone too.
    let n = syllables.len();
    let reread = &syllables[head..head + fresh];
    let replaced = &rest[..(old_from - head).min(rest.len())];
    let same_head = reread.iter().zip(replaced).take_while(|(a, b)| a == b).count();
    let moved_tail = n - (head + fresh);
    // As the engraving moves an unchanged end: every offset by the same amount.
    let all = Shift { cut: 0, by };
    let same_tail = reread[same_head..]
        .iter()
        .rev()
        .zip(replaced[same_head.min(replaced.len())..].iter().rev())
        .take_while(|(a, b)| same_moved(b, a, all))
        .count();
    let mut diff = Diff {
        head: head + same_head,
        tail: moved_tail + same_tail,
        old_len: m,
        by,
    };
    // Each syllable counts once, at the head or the tail.
    if diff.head + diff.tail > n.min(m) {
        diff.tail = n.min(m) - diff.head;
    }
    *keep = ParseMarks {
        body_start,
        marks,
        found: sink.items[..found].to_vec(),
        end: sink.items[found..].to_vec(),
    };
    let mut parsed = finish(src, old.header.clone(), syllables, sink);
    check_fixes(src, &mut parsed);
    Some((parsed, diff))
}
