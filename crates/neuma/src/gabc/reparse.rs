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

use super::{BodyMark, BodyState, ParseMarks, Parsed, check_fixes, end_body, find_separator, finish, parse_header, read_body};
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

    fn state(self, s: &BodyState) -> BodyState {
        let mut s = s.clone();
        self.move_state(&mut s);
        s
    }

    fn move_state(self, s: &mut BodyState) {
        for t in &mut s.lyric.open {
            t.moved(|p| self.at(p));
        }
        for v in s.lyric.verbatim_first.iter_mut().flatten() {
            *v = self.at(*v);
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

pub(crate) fn common_prefix(a: &[u8], b: &[u8]) -> usize {
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

pub(crate) fn common_suffix(a: &[u8], b: &[u8], most: usize) -> usize {
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
/// edit reaches from the header into the body, or the source has no `%%`.
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
    if body_start == 0 || keep.marks.len() != old.syllables.len() {
        return None;
    }
    let suffix = common_suffix(a, b, a.len().min(b.len()) - prefix);
    let (cut_old, cut_new) = (a.len() - suffix, b.len() - suffix);
    let by = b.len() as isize - a.len() as isize;
    let shift = Shift { cut: cut_old, by };
    if prefix < body_start {
        // An edit of the header alone: the body reads as it did, moved.
        return (cut_old <= body_start && after == body_start.wrapping_add_signed(by) && src[..after].ends_with('\n'))
            .then(|| header_edited(src, old, keep, shift));
    }
    if after != body_start || !src[..body_start].ends_with('\n') {
        return None;
    }
    // The syllables read before the edit stay; reading starts after the last of them, or at
    // the body's start.
    let head = keep.marks.partition_point(|m| m.read <= prefix);
    // The old syllables and marks stay where they are, those the edit replaced spliced out.
    let mut marks = std::mem::take(&mut keep.marks);
    let mut syllables = std::mem::take(&mut old.syllables);
    let m = syllables.len();
    let (mut st, before, diagnostics, from) = match head.checked_sub(1).map(|h| &marks[h]) {
        Some(r) => (r.state.clone(), Some(r.read), r.diagnostics, r.end - body_start),
        None => (BodyState::default(), None, keep.header, 0),
    };
    let mut sink = Sink {
        items: keep.found[..diagnostics].to_vec(),
    };
    let mut fresh: Vec<Syllable> = Vec::new();
    let mut fresh_marks: Vec<BodyMark> = Vec::new();
    // Past the edit, where a syllable ends as an old one did, in the same state.
    let mut met = None;
    let old_marks = &marks;
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
    let read = read_body(
        src,
        body_start,
        from,
        &mut st,
        &mut fresh,
        &mut sink,
        Some(&mut fresh_marks),
        before,
        &mut meet,
    );
    // The old syllables from `old_from` on hold, moved; the diagnostics before `found` are
    // those the body's reading found.
    let (old_from, found) = match (read, met) {
        (None, Some(k)) => {
            // The old syllables after `k`, their marks and diagnostics, moved.
            let base_diagnostics = marks[k].diagnostics;
            let d_diag = sink.items.len() as isize - base_diagnostics as isize;
            sink.items
                .extend(keep.found[base_diagnostics..].iter().cloned().map(|d| shift.diagnostic(d)));
            let mut read = fresh_marks.last().map_or(before.unwrap_or(0), |m| m.read);
            for o in &mut marks[k + 1..] {
                read = read.max(shift.at(o.read));
                o.end = shift.at(o.end);
                o.read = read;
                o.diagnostics = o.diagnostics.wrapping_add_signed(d_diag);
                shift.move_state(&mut o.state);
            }
            let found = sink.items.len();
            sink.items.extend(keep.end.iter().cloned().map(|d| shift.diagnostic(d)));
            for s in &mut syllables[k + 1..] {
                shift.syllable(s);
            }
            (k + 1, found)
        }
        (Some(left), _) => {
            let found = sink.items.len();
            end_body(src, body_start, st, left, &mut sink);
            (m, found)
        }
        (None, None) => unreachable!("reading stops only where it met the old syllables"),
    };
    // The old syllables `head..old_from` were read again as the fresh ones; some at either
    // end may have come out the same, and are left alone too.
    let replaced = &syllables[head..old_from];
    let same_head = fresh.iter().zip(replaced).take_while(|(a, b)| a == b).count();
    let moved_tail = m - old_from;
    // As the engraving moves an unchanged end: every offset by the same amount.
    let all = Shift { cut: 0, by };
    let same_tail = fresh[same_head..]
        .iter()
        .rev()
        .zip(replaced[same_head.min(replaced.len())..].iter().rev())
        .take_while(|(a, b)| same_moved(b, a, all))
        .count();
    let n = m - (old_from - head) + fresh.len();
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
    syllables.splice(head..old_from, fresh);
    marks.splice(head..old_from, fresh_marks);
    *keep = ParseMarks {
        body_start,
        header: keep.header,
        marks,
        found: sink.items[..found].to_vec(),
        end: sink.items[found..].to_vec(),
    };
    let mut parsed = finish(src, old.header.clone(), syllables, sink);
    check_fixes(src, &mut parsed);
    Some((parsed, diff))
}

/// [`reparse`] for an edit of the header alone, which `shift` moves the body by: the header
/// is read again, and the old syllables, their marks and diagnostics are moved.
fn header_edited(src: &str, old: &mut Score, keep: &mut ParseMarks, shift: Shift) -> (Parsed, Diff) {
    let mut sink = Sink::default();
    let (header, _) = parse_header(src, &mut sink);
    let header_diagnostics = sink.items.len();
    let d_diag = header_diagnostics as isize - keep.header as isize;
    sink.items
        .extend(keep.found[keep.header..].iter().cloned().map(|d| shift.diagnostic(d)));
    let found = sink.items.len();
    sink.items.extend(keep.end.iter().cloned().map(|d| shift.diagnostic(d)));
    let mut marks = std::mem::take(&mut keep.marks);
    for o in &mut marks {
        o.end = shift.at(o.end);
        o.read = shift.at(o.read);
        o.diagnostics = o.diagnostics.wrapping_add_signed(d_diag);
        shift.move_state(&mut o.state);
    }
    let mut syllables = std::mem::take(&mut old.syllables);
    for s in &mut syllables {
        shift.syllable(s);
    }
    let m = syllables.len();
    *keep = ParseMarks {
        body_start: shift.at(keep.body_start),
        header: header_diagnostics,
        marks,
        found: sink.items[..found].to_vec(),
        end: sink.items[found..].to_vec(),
    };
    let mut parsed = finish(src, header, syllables, sink);
    check_fixes(src, &mut parsed);
    let diff = Diff {
        head: 0,
        tail: m,
        old_len: m,
        by: shift.by,
    };
    (parsed, diff)
}
