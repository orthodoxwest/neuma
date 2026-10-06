//! The notes between parentheses: pitches with their shapes and signs, spaces, bars, clefs,
//! line breaks and custodes.

use crate::diag::Sink;
use crate::score::{
    Alteration, AlterationKind, Bar, BarKind, Clef, ClefKind, CustosRule, Episema, Figure, LineBreak, Liquescent, Note, NoteShape,
    Placement, Space, pitch_position,
};

/// Parses `src`, the text inside one pair of parentheses. `offset` is its byte offset in the
/// whole source.
pub(super) fn parse(src: &str, offset: usize, sink: &mut Sink) -> Vec<Figure> {
    let mut p = Parser {
        s: src.as_bytes(),
        src,
        i: 0,
        offset,
        out: Vec::new(),
        sink,
        initio: false,
        nlba: false,
    };
    p.run();
    p.out
}

struct Parser<'a, 's> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
    offset: usize,
    out: Vec<Figure>,
    sink: &'s mut Sink,
    /// A `-` before the next pitch: initio debilis.
    initio: bool,
    /// Inside `<nlba>…</nlba>`: no line break at a space.
    nlba: bool,
}

impl Parser<'_, '_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn peek_at(&self, k: usize) -> Option<u8> {
        self.s.get(self.i + k).copied()
    }

    fn span(&self, start: usize) -> std::ops::Range<usize> {
        self.offset + start..self.offset + self.i
    }

    fn space(&mut self, space: Space) {
        let space = if space == Space::Large && self.nlba {
            Space::LargeNoBreak
        } else {
            space
        };
        // Collapse runs of large spaces.
        if space == Space::Large && matches!(self.out.last(), Some(Figure::Space(Space::Large))) {
            return;
        }
        self.out.push(Figure::Space(space));
    }

    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.i;
            match c {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.i += 1;
                    self.space(Space::Large);
                }
                b'/' => self.slash(),
                b'!' => {
                    self.i += 1;
                    if matches!(self.peek(), Some(b' ')) {
                        self.i += 1;
                        self.out.push(Figure::Space(Space::LargeNoBreak));
                    } else {
                        self.out.push(Figure::Space(Space::Zero));
                    }
                }
                b'@' => {
                    // Fusion (E2): joins notes into one neume; drawn unfused for now.
                    self.i += 1;
                    if self.peek() == Some(b'[') {
                        self.i += 1;
                    }
                    self.sink.info(
                        self.span(start),
                        "gabc::fusion",
                        "neume fusion isn't drawn yet; the notes are drawn unfused",
                    );
                }
                b']' => {
                    // The end of an `@[…]` group.
                    self.i += 1;
                }
                b'`' => {
                    self.i += 1;
                    let high = self.take(b'0');
                    self.bar(BarKind::Virgula, high, start);
                }
                b'^' => {
                    self.i += 1;
                    let high = self.take(b'0');
                    self.bar(BarKind::Minimis, high, start);
                }
                b',' => {
                    self.i += 1;
                    let high = self.take(b'0');
                    // Gregorio reads `,1`–`,8` as Dominican bars too.
                    if !high && let Some(d) = self.peek().filter(|d| (b'1'..=b'8').contains(d)) {
                        self.i += 1;
                        self.bar(BarKind::Dominican(d - b'0'), false, start);
                    } else {
                        self.bar(BarKind::Minima, high, start);
                    }
                }
                b';' => {
                    self.i += 1;
                    if let Some(d) = self.peek().filter(|d| (b'1'..=b'8').contains(d)) {
                        self.i += 1;
                        self.bar(BarKind::Dominican(d - b'0'), false, start);
                    } else {
                        self.bar(BarKind::Minor, false, start);
                    }
                }
                b':' => {
                    self.i += 1;
                    if self.take(b':') {
                        self.bar(BarKind::Finalis, false, start);
                    } else if self.take(b'?') {
                        self.bar(BarKind::DottedMaior, false, start);
                    } else {
                        self.bar(BarKind::Maior, false, start);
                    }
                }
                b'z' | b'Z' => {
                    self.i += 1;
                    if c == b'z' && self.take(b'0') {
                        self.out.push(Figure::Custos {
                            position: None,
                            span: self.span(start),
                        });
                    } else {
                        let custos = if self.take(b'+') {
                            CustosRule::Force
                        } else if self.take(b'-') {
                            CustosRule::Suppress
                        } else {
                            CustosRule::Default
                        };
                        self.out.push(Figure::Break(LineBreak {
                            justify: c == b'z',
                            custos,
                            span: self.span(start),
                        }));
                    }
                }
                b'[' => self.bracket(),
                b'{' | b'}' => {
                    // Gregorio sets notes inside braces in a zero-width box, to overlap what follows.
                    self.i += 1;
                    self.sink.info(
                        self.span(start),
                        "gabc::zero-width",
                        "notes in `{…}` are drawn with their own width",
                    );
                }
                b'<' if self.at_nlba_tag() => {
                    self.nlba = !self.src[self.i..].starts_with("</");
                    self.i += self.src[self.i..].find('>').map_or(1, |n| n + 1);
                }
                b'-' => {
                    self.i += 1;
                    self.initio = true;
                }
                b'|' => {
                    // NABC follows; skip to the end of the notes.
                    self.sink.warn(
                        self.offset + start..self.offset + self.s.len(),
                        "gabc::nabc",
                        "NABC notation isn't supported and is skipped",
                    );
                    self.i = self.s.len();
                }
                b'c' | b'f' if self.is_clef() => self.clef(),
                _ if pitch_position(c as char).is_some() && c.is_ascii_alphabetic() => self.pitch(),
                _ => {
                    let ch = self.src[self.i..].chars().next().unwrap_or('\0');
                    self.i += ch.len_utf8().max(1);
                    self.sink.warn(
                        self.span(start),
                        "gabc::unknown-notation",
                        format!("`{ch}` isn't GABC notation and is skipped"),
                    );
                }
            }
        }
    }

    fn at_nlba_tag(&self) -> bool {
        let rest = &self.src[self.i..];
        rest.starts_with("<nlba>") || rest.starts_with("</nlba>")
    }

    fn take(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn bar(&mut self, kind: BarKind, high: bool, start: usize) {
        // Bar suffixes: `'` (episema) and `_` (brace) aren't supported.
        while let Some(b'\'' | b'_') = self.peek() {
            self.i += 1;
            self.sink
                .warn(self.span(start), "gabc::bar-sign", "signs on bars aren't supported and are skipped");
        }
        if let BarKind::Dominican(n @ 7..) = kind {
            self.sink.warn(
                self.span(start),
                "gabc::dominican-bar",
                format!("a four-line staff has no Dominican bar {n}; it is drawn partly above the staff"),
            );
        }
        self.out.push(Figure::Bar(Bar {
            kind,
            high,
            span: self.span(start),
        }));
    }

    fn slash(&mut self) {
        self.i += 1;
        let space = match self.peek() {
            // `//[2]` is a neumatic cut and then a scaled space, as Gregorio reads it. Before
            // anything but a number, `//` is the larger space and the `[…]` a tag.
            Some(b'/') if self.peek_at(1) == Some(b'[') && self.scale_at(self.i + 1).is_some() => Space::Small,
            Some(b'/') => {
                self.i += 1;
                Space::Medium
            }
            Some(b'0') => {
                self.i += 1;
                Space::Half
            }
            Some(b'!') => {
                self.i += 1;
                Space::Tiny
            }
            Some(b'[') => {
                let start = self.i;
                let end = self.src[self.i..].find(']').map_or(self.s.len(), |n| self.i + n);
                let factor = self.scale_at(self.i);
                self.i = (end + 1).min(self.s.len());
                match factor {
                    Some(f) => Space::Scaled(f),
                    None => {
                        self.sink
                            .warn(self.span(start), "gabc::bad-space", "`/[…]` needs a number; using a small space");
                        Space::Small
                    }
                }
            }
            _ => Space::Small,
        };
        self.out.push(Figure::Space(space));
    }

    /// The factor in a `[f]` starting at byte `at`, if it is a finite number.
    fn scale_at(&self, at: usize) -> Option<f32> {
        let end = self.src[at..].find(']').map_or(self.s.len(), |n| at + n);
        self.src[at + 1..end].trim().parse::<f32>().ok().filter(|f| f.is_finite())
    }

    fn bracket(&mut self) {
        let start = self.i;
        let end = self.src[self.i..].find(']').map_or(self.s.len(), |n| self.i + n);
        let inner = &self.src[self.i + 1..end.max(self.i + 1)];
        self.i = (end + 1).min(self.s.len());
        if inner == "nocustos" {
            self.out.push(Figure::NoCustos);
        } else {
            let name = inner.split([':', '{', '}']).next().unwrap_or(inner);
            if matches!(name, "oh" | "uh" | "ll") {
                // Fine-tuning of where an episema sits or how long a stem is: the default
                // placement stands, so it's worth a note rather than a warning.
                self.sink.info(
                    self.span(start),
                    "gabc::tuning-ignored",
                    format!("`[{name}:…]` fine-tunes placement; the default placement is used"),
                );
            } else {
                self.sink.warn(
                    self.span(start),
                    "gabc::unsupported-tag",
                    format!("`[{name}:…]` isn't supported and is skipped"),
                );
            }
        }
    }

    /// `c1`–`c5`, `f1`–`f5`, `cb1`–`cb5`, with an optional `@` second clef.
    fn is_clef(&self) -> bool {
        match self.peek_at(1) {
            Some(d) if d.is_ascii_digit() => true,
            Some(b'b') => self.peek_at(2).is_some_and(|d| d.is_ascii_digit()),
            _ => false,
        }
    }

    fn clef(&mut self) {
        let start = self.i;
        let kind = if self.peek() == Some(b'c') { ClefKind::Do } else { ClefKind::Fa };
        self.i += 1;
        let flat = self.take(b'b');
        let line = self.peek().map_or(0, |d| d - b'0');
        self.i += 1;
        let line = if (1..=4).contains(&line) {
            line
        } else {
            self.sink.warn(
                self.span(start),
                "gabc::clef-line",
                format!("a four-line staff has no clef line {line}; using line 3"),
            );
            3
        };
        let clef = Clef {
            kind,
            line,
            flat,
            span: self.span(start),
        };
        if self.peek() == Some(b'@') && matches!(self.peek_at(1), Some(b'c' | b'f')) {
            let second = self.i;
            self.i += 2;
            self.take(b'b');
            self.i += 1;
            self.sink.warn(
                self.span(second),
                "gabc::double-clef",
                "double clefs aren't supported; the first clef is used",
            );
        }
        self.out.push(Figure::Clef(clef));
    }

    fn pitch(&mut self) {
        let start = self.i;
        let c = self.s[self.i];
        self.i += 1;
        let Some(position) = pitch_position(c as char) else { return };
        if position > 6 {
            self.sink.warn(
                self.span(start),
                "gabc::pitch-range",
                format!("`{}` is above a four-line staff", c as char),
            );
        }

        // Alterations and custodes are figures of their own, at this pitch.
        match self.peek() {
            Some(b'x' | b'X' | b'y' | b'Y' | b'#') => {
                let a = self.s[self.i];
                self.i += 1;
                let (kind, mut soft) = match a {
                    b'x' => (AlterationKind::Flat, false),
                    b'X' => (AlterationKind::Flat, true),
                    b'y' => (AlterationKind::Natural, false),
                    b'Y' => (AlterationKind::Natural, true),
                    _ => (AlterationKind::Sharp, false),
                };
                if a == b'#' && self.take(b'#') {
                    soft = true;
                }
                let parenthesized = self.take(b'?');
                self.out.push(Figure::Alteration(Alteration {
                    position,
                    kind,
                    soft,
                    parenthesized,
                    span: self.span(start),
                }));
                return;
            }
            Some(b'+') => {
                self.i += 1;
                self.out.push(Figure::Custos {
                    position: Some(position),
                    span: self.span(start),
                });
                return;
            }
            _ => {}
        }

        let shape = if c.is_ascii_uppercase() {
            NoteShape::Inclinatum
        } else {
            NoteShape::Punctum
        };
        let mut note = Note::new(position, shape, start..start);
        note.initio_debilis = std::mem::take(&mut self.initio);
        // Notes already finished by a repetition (`vv`, `ss`); signs after it go to the new note.
        let mut done: Vec<Note> = Vec::new();
        // Notes before this one in its group that a run of `_` extends the episema over.
        let mut extend = 0usize;
        loop {
            match self.peek() {
                Some(d @ b'0'..=b'2') if note.shape == NoteShape::Inclinatum && note.lean.is_none() => {
                    self.i += 1;
                    note.lean = Some(d - b'0');
                }
                Some(b'v') => {
                    self.i += 1;
                    if note.shape == NoteShape::Virga {
                        // `vv`, `vvv`: bivirga, trivirga.
                        let next = Note::new(position, NoteShape::Virga, start..start);
                        done.push(std::mem::replace(&mut note, next));
                    } else {
                        note.shape = NoteShape::Virga;
                    }
                }
                Some(b'V') => {
                    self.i += 1;
                    note.shape = NoteShape::VirgaReversa;
                }
                Some(b's') => {
                    self.i += 1;
                    if note.shape == NoteShape::Stropha {
                        let next = Note::new(position, NoteShape::Stropha, start..start);
                        done.push(std::mem::replace(&mut note, next));
                    } else {
                        note.shape = NoteShape::Stropha;
                    }
                }
                Some(b'w') => {
                    self.i += 1;
                    note.shape = NoteShape::Quilisma;
                }
                Some(b'W') => {
                    self.i += 1;
                    note.shape = NoteShape::Quilisma;
                    note.quadratum = true;
                }
                Some(b'o') => {
                    self.i += 1;
                    note.shape = NoteShape::Oriscus;
                    if let Some(d @ (b'0' | b'1')) = self.peek() {
                        self.i += 1;
                        note.orientation = Some(d == b'1');
                    }
                }
                Some(b'O') => {
                    self.i += 1;
                    note.shape = NoteShape::OriscusScapus;
                    if let Some(d @ (b'0' | b'1')) = self.peek() {
                        self.i += 1;
                        note.orientation = Some(d == b'1');
                    }
                }
                Some(b'q') => {
                    self.i += 1;
                    note.quadratum = true;
                }
                Some(b'r') => {
                    self.i += 1;
                    match self.peek() {
                        Some(d @ b'1'..=b'8') => {
                            self.i += 1;
                            note.above_sign = Some(d - b'0');
                            self.sink
                                .info(self.span(start), "gabc::above-sign", "signs above the staff aren't drawn yet");
                        }
                        Some(b'0') => {
                            self.i += 1;
                            note.cavum = true;
                            self.sink.warn(
                                self.span(start),
                                "gabc::lined-note",
                                "notes surrounded by lines are drawn without the lines",
                            );
                        }
                        _ => note.cavum = true,
                    }
                }
                Some(b'R') => {
                    self.i += 1;
                    self.sink.warn(
                        self.span(start),
                        "gabc::lined-note",
                        "notes surrounded by lines are drawn without the lines",
                    );
                }
                Some(b'=') => {
                    self.i += 1;
                    self.sink
                        .warn(self.span(start), "gabc::linea", "the linea isn't supported; drawn as a punctum");
                }
                Some(b'~') => {
                    self.i += 1;
                    note.liquescent = Liquescent::Deminutus;
                }
                Some(b'<') if !self.at_nlba_tag() => {
                    self.i += 1;
                    note.liquescent = Liquescent::Augmented;
                }
                Some(b'>') => {
                    self.i += 1;
                    note.liquescent = Liquescent::Diminished;
                }
                Some(b'.') => {
                    self.i += 1;
                    note.morae = (note.morae + 1).min(2);
                    match self.peek() {
                        Some(b'0') => {
                            self.i += 1;
                            note.mora_placement = Placement::Below;
                        }
                        Some(b'1') => {
                            self.i += 1;
                            note.mora_placement = Placement::Above;
                        }
                        _ => {}
                    }
                }
                Some(b'_') => {
                    self.i += 1;
                    // Each further `_` right after the first carries the episema back over one
                    // more note of the group: `fgf___` marks all three.
                    // Digits after the run modify the one episema it draws.
                    let mut e = match note.episema {
                        Some(e) if self.src.as_bytes().get(self.i - 2) == Some(&b'_') => {
                            extend += 1;
                            e
                        }
                        _ => Episema::default(),
                    };
                    while let Some(d @ b'0'..=b'5') = self.peek() {
                        self.i += 1;
                        match d {
                            b'0' => e.placement = Placement::Below,
                            b'1' => e.placement = Placement::Above,
                            b'2' => e.no_bridge = true,
                            _ => e.small = Some(d - b'0'),
                        }
                    }
                    note.episema = Some(e);
                }
                Some(b'\'') => {
                    self.i += 1;
                    let placement = match self.peek() {
                        Some(b'0') => {
                            self.i += 1;
                            Placement::Below
                        }
                        Some(b'1') => {
                            self.i += 1;
                            Placement::Above
                        }
                        _ => Placement::Auto,
                    };
                    note.ictus = Some(placement);
                }
                _ => break,
            }
        }
        if let Some(e) = note.episema
            && extend > 0
        {
            let mut left = extend;
            for n in done.iter_mut().rev() {
                if left == 0 {
                    break;
                }
                n.episema.get_or_insert(e);
                left -= 1;
            }
            for f in self.out.iter_mut().rev() {
                // `!` joins notes into one neume, so the run reaches past it.
                let n = match f {
                    Figure::Note(n) => n,
                    Figure::Space(Space::Zero) => continue,
                    _ => break,
                };
                if left == 0 {
                    break;
                }
                n.episema.get_or_insert(e);
                left -= 1;
            }
        }
        let span = self.span(start);
        for mut n in done.into_iter().chain(std::iter::once(note)) {
            n.span = span.clone();
            self.out.push(Figure::Note(n));
        }
    }
}
