//! Neume construction: splits a run of notes into neumes and builds each one from glyphs,
//! stems and markings. Ported from exsurge's `createNeumesFromNotes` and `NeumeBuilder` (MIT,
//! (c) 2008-2016 Fr. Matthew Spencer, OSJ; see NOTICE) and restructured for neuma's model.
//!
//! Geometry is in staff spaces with y growing downward: a note at staff position `p` sits at
//! `y = -p`. Glyphs are placed by their left edge, as exsurge does, and drawn at
//! `left + origin_x`.

use super::{Ink, Mark, Piece};
use crate::glyphs::{Align, GlyphId as G};
use crate::score::{Liquescent, Note, NoteShape, Placement};

/// Weight of stems, connecting lines and staff lines.
pub(crate) const STEM: f32 = 0.128;
/// Thickness of a horizontal episema.
pub(crate) const EPISEMA: f32 = STEM * 1.25;
/// exsurge's `intraNeumeSpacing`: half a staff space.
pub(crate) const INTRA: f32 = 0.5;
/// How far a ledger line reaches past the notes it serves, each way: GregorioTeX's
/// `additionallineswidth`, 0.14584 cm at a 0.288 cm staff space, less the punctum's side
/// bearings, as its ledger lines measure on the page.
pub(crate) const LEDGER_OVERHANG: f32 = 0.95;
/// The narrowest gap left between two ledger lines at one height; any narrower and the two are
/// drawn as one. GregorioTeX's meet exactly at its word space between notes, and a sliver
/// between them would look like a slip.
pub(crate) const LEDGER_GAP: f32 = 0.5;
/// The gap a ledger line leaves before an accidental or an ictus it would otherwise run into.
const LEDGER_CLEAR: f32 = 0.25;
/// Height of the punctum glyph's bounds, for hanging lines.
const PUNCTUM_HEIGHT: f32 = 1.036;
/// How far a line hanging from a note reaches past the lower note it joins.
const HANGING_LINE_REACH: f32 = PUNCTUM_HEIGHT / 2.2;
/// A virga's stem from a note on a line, in staff spaces.
const VIRGA_STEM_ON_LINE: f32 = 2.7;
/// A virga's stem from a note in a space, in staff spaces.
const VIRGA_STEM_IN_SPACE: f32 = 1.8;
/// How far apart two inclinata on the same pitch are, in inclinatum widths.
const INCLINATA_UNISON: f32 = 1.1;
/// The gap between the pes and the inclinata of a pes subpunctis.
const PES_SUBPUNCTIS_GAP: f32 = INTRA * 0.68;
/// The part of its width an episema keeps over an inclinatum that steps down from the one
/// before, or over a liquescent inclinatum.
const SHORT_EPISEMA: f32 = 2.0 / 3.0;
/// How far an ictus is set off a staff line it would hit, in staff spaces.
const ICTUS_OFF_LINE: f32 = 0.3;
/// How far an ictus in a space is set toward its note, in staff spaces.
const ICTUS_IN_SPACE: f32 = -0.2;
/// How much farther an ictus in a space goes to clear an inclinatum's corner.
const ICTUS_PAST_INCLINATUM: f32 = 0.3;
/// How much farther an ictus goes to clear an episema on the same side.
const ICTUS_PAST_EPISEMA: f32 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Punctum,
    Virga,
    Bivirga,
    Trivirga,
    Apostropha,
    Distropha,
    Tristropha,
    Oriscus,
    PunctaInclinata,
    Podatus,
    Clivis,
    Climacus,
    Porrectus,
    PorrectusFlexus,
    PesQuassus,
    PesSubpunctis,
    Salicus,
    SalicusFlexus,
    Scandicus,
    ScandicusFlexus,
    Torculus,
    TorculusResupinus,
    TorculusResupinusFlexus,
    Ancus,
}

/// One neume found in a run of notes: `notes[start..end]`, and the space after it when another
/// neume of the same run follows (`None` at the end of the run).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Split {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub trailing: Option<f32>,
}

fn plain(n: &Note) -> bool {
    n.shape == NoteShape::Punctum
}
fn virga(n: &Note) -> bool {
    matches!(n.shape, NoteShape::Virga | NoteShape::VirgaReversa)
}
fn oriscus(n: &Note) -> bool {
    matches!(n.shape, NoteShape::Oriscus | NoteShape::OriscusScapus)
}
fn inclinatum(n: &Note) -> bool {
    n.shape == NoteShape::Inclinatum
}
fn small(n: &Note) -> bool {
    n.liquescent == Liquescent::Deminutus
}
fn odd(p: i8) -> bool {
    p.rem_euclid(2) == 1
}
fn set_auto(p: &mut Option<Placement>, to: Placement) {
    if *p == Some(Placement::Auto) {
        *p = Some(to);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum St {
    Unknown,
    Punctum,
    Inclinata,
    Oriscus,
    Podatus,
    Clivis,
    Climacus,
    Porrectus,
    PesSubpunctis,
    Salicus,
    SalicusFlexus,
    Scandicus,
    ScandicusFlexus,
    Virga,
    Bivirga,
    Apostropha,
    Distropha,
    Tristropha,
    Torculus,
    TorculusResupinus,
}

impl St {
    fn kind(self) -> Kind {
        match self {
            St::Unknown | St::Punctum => Kind::Punctum,
            St::Inclinata => Kind::PunctaInclinata,
            St::Oriscus => Kind::Oriscus,
            St::Podatus => Kind::Podatus,
            St::Clivis => Kind::Clivis,
            St::Climacus => Kind::Climacus,
            St::Porrectus => Kind::Porrectus,
            St::PesSubpunctis => Kind::PesSubpunctis,
            St::Salicus => Kind::Salicus,
            St::SalicusFlexus => Kind::SalicusFlexus,
            St::Scandicus => Kind::Scandicus,
            St::ScandicusFlexus => Kind::ScandicusFlexus,
            St::Virga => Kind::Virga,
            St::Bivirga => Kind::Bivirga,
            St::Apostropha => Kind::Apostropha,
            St::Distropha => Kind::Distropha,
            St::Tristropha => Kind::Tristropha,
            St::Torculus => Kind::Torculus,
            St::TorculusResupinus => Kind::TorculusResupinus,
        }
    }
}

struct Splitter<'a> {
    notes: &'a mut [Note],
    out: Vec<Split>,
    first: usize,
    cur: isize,
}

impl Splitter<'_> {
    /// exsurge's `createNeume`: ends a neume at the current note, the previous one, or the one
    /// before that, and steps back so the notes left over start the next neume.
    fn create(&mut self, kind: Kind, include_cur: bool, include_prev: bool) -> St {
        let last = if include_cur {
            self.cur
        } else if include_prev {
            self.cur - 1
        } else {
            self.cur - 2
        };
        if last < 0 {
            return St::Unknown;
        }
        let end = last as usize + 1;
        self.out.push(Split {
            kind,
            start: self.first,
            end,
            trailing: None,
        });
        self.first = end;
        if !include_cur {
            self.cur -= 1;
            if !include_prev {
                self.cur -= 1;
            }
            let next = &self.notes[(self.cur + 1) as usize];
            let space = if next.shape == NoteShape::Quilisma { 0.0 } else { INTRA };
            if let Some(s) = self.out.last_mut() {
                s.trailing = Some(space);
            }
        }
        St::Unknown
    }

    /// After a single punctum or oriscus is split off before a higher note, exsurge closes the
    /// gap unless a mora on a line just below would collide.
    fn close_gap_if_rising(&mut self, c: usize) {
        let (cur, prev) = (&self.notes[c], &self.notes[c - 1]);
        if cur.position > prev.position
            && (odd(cur.position) || prev.position != cur.position - 1 || prev.morae == 0)
            && let Some(s) = self.out.last_mut()
        {
            s.trailing = Some(0.0);
        }
    }

    fn step(&mut self, state: St) -> St {
        let c = self.cur as usize;
        let cur = self.notes[c].clone();
        let prev = if c > 0 { Some(self.notes[c - 1].clone()) } else { None };
        let Some(prev) = prev.filter(|_| state != St::Unknown) else {
            return if virga(&cur) {
                St::Virga
            } else if cur.shape == NoteShape::Stropha {
                St::Apostropha
            } else if oriscus(&cur) {
                St::Oriscus
            } else if inclinatum(&cur) {
                St::Inclinata
            } else if cur.cavum {
                self.create(Kind::Punctum, true, true)
            } else {
                St::Punctum
            };
        };
        let up = cur.position > prev.position;
        let down = cur.position < prev.position;
        match state {
            St::Unknown => unreachable!(),
            St::Punctum => {
                if !plain(&cur) || small(&prev) {
                    let s = self.create(Kind::Punctum, false, true);
                    self.close_gap_if_rising(c);
                    s
                } else if up {
                    set_auto(&mut self.notes[c].ictus, Placement::Above);
                    St::Podatus
                } else if down {
                    set_auto(&mut self.notes[c - 1].ictus, Placement::Above);
                    if inclinatum(&cur) { St::Climacus } else { St::Clivis }
                } else if prev.morae == 0 {
                    St::Distropha
                } else {
                    self.create(Kind::Punctum, false, true)
                }
            }
            St::Inclinata => {
                if inclinatum(&cur) {
                    St::Inclinata
                } else {
                    self.create(Kind::PunctaInclinata, false, true)
                }
            }
            St::Oriscus => {
                if plain(&cur) && up {
                    self.notes[c - 1].orientation.get_or_insert(true);
                    return self.create(Kind::PesQuassus, true, true);
                }
                if plain(&cur) && down {
                    self.notes[c - 1].orientation.get_or_insert(false);
                    return self.create(Kind::Clivis, true, true);
                }
                let s = self.create(Kind::Oriscus, false, true);
                self.close_gap_if_rising(c);
                s
            }
            St::Podatus => {
                if up {
                    set_auto(&mut self.notes[c].ictus, Placement::Above);
                    set_auto(&mut self.notes[c - 1].ictus, Placement::Below);
                    if oriscus(&prev) { St::Salicus } else { St::Scandicus }
                } else if down {
                    if inclinatum(&cur) { St::PesSubpunctis } else { St::Torculus }
                } else {
                    self.create(Kind::Podatus, false, true)
                }
            }
            St::Clivis => {
                if plain(&cur) && up {
                    set_auto(&mut self.notes[c].ictus, Placement::Above);
                    St::Porrectus
                } else if down && small(&cur) {
                    self.create(Kind::Ancus, true, true)
                } else {
                    self.create(Kind::Clivis, false, true)
                }
            }
            St::Climacus => {
                if inclinatum(&cur) {
                    St::Climacus
                } else {
                    self.create(Kind::Climacus, false, true)
                }
            }
            St::Porrectus => {
                if plain(&cur) && down {
                    self.create(Kind::PorrectusFlexus, true, true)
                } else {
                    self.create(Kind::Porrectus, false, true)
                }
            }
            St::PesSubpunctis => {
                if inclinatum(&cur) {
                    St::PesSubpunctis
                } else {
                    self.create(Kind::PesSubpunctis, false, true)
                }
            }
            St::Salicus => {
                if down {
                    St::SalicusFlexus
                } else {
                    self.create(Kind::Salicus, false, true)
                }
            }
            St::SalicusFlexus => self.create(Kind::SalicusFlexus, false, true),
            St::Scandicus => {
                if virga(&prev) && inclinatum(&cur) && down {
                    // A pes followed by a climacus, not a scandicus.
                    self.create(Kind::Podatus, false, false)
                } else if plain(&cur) && down {
                    St::ScandicusFlexus
                } else {
                    self.create(Kind::Scandicus, false, true)
                }
            }
            St::ScandicusFlexus => self.create(Kind::ScandicusFlexus, false, true),
            St::Virga => {
                if inclinatum(&cur) && down {
                    St::Climacus
                } else if virga(&cur) && cur.position == prev.position {
                    St::Bivirga
                } else {
                    self.create(Kind::Virga, false, true)
                }
            }
            St::Bivirga => {
                if virga(&cur) && cur.position == prev.position {
                    self.create(Kind::Trivirga, true, true)
                } else {
                    self.create(Kind::Bivirga, false, true)
                }
            }
            St::Apostropha => {
                if cur.position == prev.position {
                    St::Distropha
                } else {
                    self.create(Kind::Apostropha, false, true)
                }
            }
            St::Distropha => {
                if cur.position == prev.position {
                    if prev.morae > 0 {
                        self.create(Kind::Distropha, false, true)
                    } else {
                        St::Tristropha
                    }
                } else {
                    self.create(Kind::Apostropha, false, false)
                }
            }
            // A tristropha only forms when the run ends after three notes; otherwise the third
            // note starts the next neume.
            St::Tristropha => self.create(Kind::Distropha, false, false),
            St::Torculus => {
                if plain(&cur) && up && c >= 2 && self.notes[c - 2].position - prev.position <= 4 {
                    set_auto(&mut self.notes[c].ictus, Placement::Above);
                    St::TorculusResupinus
                } else {
                    self.create(Kind::Torculus, false, true)
                }
            }
            St::TorculusResupinus => {
                if plain(&cur) && down {
                    self.create(Kind::TorculusResupinusFlexus, true, true)
                } else {
                    self.create(Kind::TorculusResupinus, false, true)
                }
            }
        }
    }
}

/// Splits a run of notes (no separators between them) into neumes. Fills in automatic oriscus
/// orientations and ictus placements as it goes.
pub(crate) fn split(notes: &mut [Note]) -> Vec<Split> {
    let len = notes.len() as isize;
    let mut s = Splitter {
        notes,
        out: Vec::new(),
        first: 0,
        cur: 0,
    };
    let mut state = St::Unknown;
    while s.cur < len {
        state = s.step(state);
        if s.cur == len - 1 && state != St::Unknown {
            state = s.create(state.kind(), true, true);
        }
        s.cur += 1;
    }
    s.out
}

/// A drawn notehead: the box exsurge calls the note's bounds, in neume coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Head {
    /// Index of the note in the run.
    pub index: usize,
    pub glyph: Option<G>,
    pub x: f32,
    pub w: f32,
    /// Top and bottom of the glyph's bounds.
    pub top: f32,
    pub bottom: f32,
    pub position: i8,
    /// The note's hit box (left, top, right, bottom) when it isn't its notehead box: a
    /// porrectus end's box, trimmed on the side that faces a neighbour.
    pub hit: Option<[f32; 4]>,
}

impl Head {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    /// The x where a mark centered on the note goes.
    pub fn center(&self) -> f32 {
        match self.glyph {
            Some(G::Porrectus1 | G::Porrectus2 | G::Porrectus3 | G::Porrectus4) => self.x + 0.5,
            None => self.x - 0.5,
            _ => self.x + self.w / 2.0,
        }
    }
    /// Width and height of the note's own box: a punctum's worth at either end of a
    /// porrectus swash, else the glyph's.
    pub fn size(&self) -> (f32, f32) {
        match self.glyph {
            Some(G::Porrectus1 | G::Porrectus2 | G::Porrectus3 | G::Porrectus4) | None => (1.0, 1.0),
            _ => (self.w.max(0.5), self.bottom - self.top),
        }
    }
    /// The note's hit box: center x, center y, width and height.
    /// The notehead box: center x, center y, width and height.
    pub fn hit_box(&self) -> (f32, f32, f32, f32) {
        let (w, h) = self.size();
        (self.center(), -(self.position as f32), w, h)
    }
    /// The box hit testing uses (left, top, right, bottom): the notehead box, or less of it
    /// for a porrectus end.
    pub fn hit_rect(&self) -> [f32; 4] {
        self.hit.unwrap_or_else(|| {
            let (x, y, w, h) = self.hit_box();
            [x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0]
        })
    }
    fn swash(&self) -> bool {
        matches!(
            self.glyph,
            Some(G::Porrectus1 | G::Porrectus2 | G::Porrectus3 | G::Porrectus4) | None
        )
    }
}

/// Trims the hit boxes of a porrectus swash's two ends off the other noteheads of the neume
/// (the note stacked on the swash's end, the punctum before it), on the side facing each one,
/// along the axis where they overlap least, so that hit testing never finds two notes at one
/// point. The notehead's own box, and its center, stay as they are.
fn trim_swash_boxes(heads: &mut [Head]) {
    for i in 0..heads.len() {
        if !heads[i].swash() {
            continue;
        }
        let whole = heads[i].hit_rect();
        let [mut l, mut t, mut r, mut b] = whole;
        let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
        for (j, o) in heads.iter().enumerate() {
            if j == i {
                continue;
            }
            let [ol, ot, or, ob] = o.hit_rect();
            let (dx, dy) = (r.min(or) - l.max(ol), b.min(ob) - t.max(ot));
            if dx <= 0.0 || dy <= 0.0 {
                continue;
            }
            if dy <= dx {
                if (ot + ob) / 2.0 < cy { t = t.max(ob) } else { b = b.min(ot) }
            } else if (ol + or) / 2.0 < cx {
                l = l.max(or);
            } else {
                r = r.min(ol);
            }
        }
        if [l, t, r, b] != whole && r > l && b > t {
            heads[i].hit = Some([l, t, r, b]);
        }
    }
}

#[derive(Clone, Copy)]
struct Last {
    position: i8,
    align: Align,
}

/// exsurge's `NeumeBuilder`.
struct Builder {
    pieces: Vec<Piece>,
    heads: Vec<Head>,
    x: f32,
    last: Option<Last>,
    hanging: bool,
    min_x: f32,
    note_base: u32,
}

impl Builder {
    fn new(note_base: u32) -> Builder {
        Builder {
            pieces: Vec::new(),
            heads: Vec::new(),
            x: 0.0,
            last: None,
            hanging: false,
            min_x: 0.0,
            note_base,
        }
    }

    fn stem(&mut self, x: f32, y: f32, h: f32, note: Option<usize>) {
        let note = note.map(|i| self.note_base + i as u32);
        self.pieces.push(Piece {
            mark: Mark::Rect { x, y, w: STEM, h },
            role: Ink::Stem,
            note,
            through: None,
        });
    }

    /// A connecting line between two staff positions, as exsurge's `NeumeLineVisualizer`.
    /// Returns its top and height.
    fn line_between(p0: i8, p1: i8, hanging: bool) -> (f32, f32) {
        let (upper, mut lower) = if p0 < p1 { (p1, p0) } else { (p0, p1) };
        if hanging && upper - lower > 4 {
            lower = upper - 4;
        }
        let y0 = -(upper as f32);
        let mut y1 = 0.0;
        if hanging {
            // A hanging line from a note on a line down one step goes past the lower note by a
            // whole step, so it reads as a stem.
            if upper - lower == 1 && odd(upper) && lower > -3 {
                lower -= 1;
            }
            y1 += HANGING_LINE_REACH;
        }
        y1 += -(lower as f32);
        (y0, y1 - y0)
    }

    fn line_from(&mut self, note: &Note) -> &mut Self {
        self.last = Some(Last {
            position: note.position,
            align: Align::Left,
        });
        self.hanging = true;
        self
    }

    fn needs_line(&self, position: i8) -> Option<Last> {
        let last = self.last?;
        (self.hanging || last.align == Align::Right || (last.position - position).abs() > 1).then_some(last)
    }

    fn note_at(&mut self, i: usize, note: &Note, glyph: G, with_line: bool) -> &mut Self {
        let right = glyph.align() == Align::Right;
        if with_line && let Some(last) = self.needs_line(note.position) {
            let (y, h) = Self::line_between(last.position, note.position, self.hanging);
            let lx = self.min_x.max(self.x - STEM);
            self.stem(lx, y, h, Some(i));
            if !right {
                self.x = lx;
            }
        }
        let w = glyph.width();
        let x = if right && self.last.is_some() {
            self.x - w
        } else {
            let x = self.x;
            self.x += w;
            x
        };
        self.place(i, note, glyph, x);
        self.last = Some(Last {
            position: note.position,
            align: glyph.align(),
        });
        self.hanging = false;
        self
    }

    fn place(&mut self, i: usize, note: &Note, glyph: G, x: f32) {
        let y = -(note.position as f32);
        let top = y - glyph.data().origin_y / 100.0;
        self.pieces.push(Piece {
            mark: Mark::Glyph {
                glyph,
                x: x + glyph.origin_x(),
                y,
            },
            role: Ink::Note,
            note: Some(self.note_base + i as u32),
            through: None,
        });
        self.heads.push(Head {
            index: i,
            glyph: Some(glyph),
            x,
            w: glyph.width(),
            top,
            bottom: top + glyph.height(),
            position: note.position,
            hit: None,
        });
    }

    /// A virga: a punctum with a stem down its right side (left side for a virga reversa).
    fn virga_at(&mut self, i: usize, note: &Note) -> &mut Self {
        self.note_at(i, note, G::PunctumQuadratum, true);
        let y = -(note.position as f32);
        let h = if odd(note.position) {
            VIRGA_STEM_ON_LINE
        } else {
            VIRGA_STEM_IN_SPACE
        };
        self.x -= STEM;
        let x = if note.shape == NoteShape::VirgaReversa {
            self.x - 1.0 + STEM
        } else {
            self.x
        };
        self.stem(x, y, h, Some(i));
        self.last = Some(Last {
            position: note.position,
            align: Align::Left,
        });
        self.hanging = false;
        self
    }

    fn advance(&mut self, by: f32) -> &mut Self {
        self.last = None;
        self.hanging = false;
        self.x += by;
        self
    }

    fn line_ending_at(&mut self, note: &Note) -> &mut Self {
        let Some(last) = self.last else { return self };
        let (y, h) = Self::line_between(last.position, note.position, true);
        self.x -= STEM;
        let x = self.x;
        self.stem(x, y, h, None);
        self.last = Some(Last {
            position: note.position,
            align: Align::Left,
        });
        self
    }

    fn podatus(&mut self, li: usize, lower: &Note, ui: usize, upper: &Note) -> &mut Self {
        let (mut lg, ug) = if lower.initio_debilis {
            let ug = if upper.liquescent == Liquescent::None {
                G::PunctumQuadratum
            } else {
                G::PunctumQuadratumDesLiquescent
            };
            (G::TerminatingDesLiquescent, ug)
        } else {
            match upper.liquescent {
                Liquescent::Deminutus => (G::BeginningAscLiquescent, G::TerminatingAscLiquescent),
                Liquescent::Augmented => (G::PunctumQuadratum, G::PunctumQuadratumAscLiquescent),
                Liquescent::Diminished => (G::PunctumQuadratum, G::PunctumQuadratumDesLiquescent),
                Liquescent::None if upper.position - lower.position > 1 => (G::PodatusLower, G::PodatusUpper),
                Liquescent::None => (G::PodatusLowerShort, G::PodatusUpperShort),
            }
        };
        if lower.shape == NoteShape::Quilisma {
            lg = G::Quilisma;
        }
        self.note_at(li, lower, lg, true).note_at(ui, upper, ug, true);
        self.last = None;
        self
    }

    fn clivis_upper(&mut self, ui: usize, upper: &Note, lower: Option<&Note>, mut glyph: G) -> &mut Self {
        if oriscus(upper) {
            return self.note_at(ui, upper, G::OriscusDes, false);
        }
        if let Some(lower) = lower {
            self.line_from(lower);
            self.hanging = lower.position < upper.position;
            if small(lower) {
                glyph = G::BeginningDesLiquescent;
            }
        }
        self.note_at(ui, upper, glyph, true)
    }

    fn clivis_lower(&mut self, li: usize, lower: &Note) -> &mut Self {
        let g = match lower.liquescent {
            Liquescent::Deminutus => G::TerminatingDesLiquescent,
            Liquescent::Augmented => G::PunctumQuadratumAscLiquescent,
            Liquescent::Diminished => G::PunctumQuadratumDesLiquescent,
            Liquescent::None => G::PunctumQuadratum,
        };
        self.note_at(li, lower, g, true)
    }

    fn clivis(&mut self, ui: usize, upper: &Note, li: usize, lower: &Note) -> &mut Self {
        self.clivis_upper(ui, upper, Some(lower), G::PunctumQuadratum)
            .clivis_lower(li, lower);
        self.last = None;
        self
    }

    fn inclinata(&mut self, base: usize, notes: &[Note]) -> &mut Self {
        // Advance by the inclinatum's own width so liquescents space like the others.
        let advance = G::PunctumInclinatum.width();
        let mut prev = notes.first().map_or(0, |n| n.position);
        for (k, n) in notes.iter().enumerate() {
            let g = match n.liquescent {
                Liquescent::Deminutus => G::PunctumInclinatumLiquescent,
                _ => G::PunctumInclinatum,
            };
            let diff = (prev - n.position).abs();
            let multiple = if diff == 0 { INCLINATA_UNISON } else { diff as f32 * 2.0 / 3.0 };
            if k > 0 {
                self.x += advance * multiple;
            }
            let x = self.x;
            self.place(base + k, n, g, x);
            prev = n.position;
        }
        if !notes.is_empty() {
            self.x += advance;
        }
        self.last = None;
        self
    }

    fn porrectus_swash(&mut self, si: usize, start: &Note, ei: usize, end: &Note) -> &mut Self {
        if let Some(last) = self.needs_line(start.position) {
            let (y, h) = Self::line_between(last.position, start.position, self.hanging);
            self.x = self.min_x.max(self.x - STEM);
            let x = self.x;
            self.stem(x, y, h, Some(si));
        }
        let glyph = match start.position - end.position {
            1 => G::Porrectus1,
            2 => G::Porrectus2,
            3 => G::Porrectus3,
            4 => G::Porrectus4,
            _ => {
                // No swash this wide: two puncta joined by a stem (a diagnostic says so).
                self.last = None;
                self.hanging = false;
                self.note_at(si, start, G::PunctumQuadratum, false);
                self.line_from(start);
                self.hanging = false;
                return self.note_at(ei, end, G::PunctumQuadratum, true);
            }
        };
        let x = self.x;
        self.place(si, start, glyph, x);
        if let Some(piece) = self.pieces.last_mut() {
            piece.through = Some(self.note_base + ei as u32);
        }
        self.x = x + glyph.width();
        // The second note draws nothing; its head is the swash's right end.
        let y = -(end.position as f32);
        self.heads.push(Head {
            index: ei,
            glyph: None,
            x: self.x,
            w: 0.0,
            top: y - 0.6,
            bottom: y + 0.6,
            position: end.position,
            hit: None,
        });
        self.last = Some(Last {
            position: end.position,
            align: Align::Left,
        });
        self.hanging = false;
        self
    }
}

fn single_glyph(n: &Note) -> G {
    match (n.liquescent, n.shape) {
        (Liquescent::None, _) if n.cavum => G::PunctumCavum,
        (Liquescent::None, NoteShape::Inclinatum) => G::PunctumInclinatum,
        (Liquescent::None, NoteShape::Quilisma) => G::Quilisma,
        (Liquescent::None, _) => G::PunctumQuadratum,
        (_, NoteShape::Inclinatum) => G::PunctumInclinatumLiquescent,
        (_, NoteShape::Oriscus | NoteShape::OriscusScapus) => G::OriscusLiquescent,
        (Liquescent::Augmented, _) => G::PunctumQuadratumAscLiquescent,
        (Liquescent::Diminished, _) => G::PunctumQuadratumDesLiquescent,
        (Liquescent::Deminutus, _) => G::PunctumQuadratumLiquescent,
    }
}

fn apostropha_glyph(n: &Note) -> G {
    if n.shape == NoteShape::Stropha {
        return G::Stropha;
    }
    match n.liquescent {
        Liquescent::Augmented => G::PunctumQuadratumAscLiquescent,
        Liquescent::Diminished => G::PunctumQuadratumDesLiquescent,
        _ if n.cavum => G::PunctumCavum,
        _ => G::PunctumQuadratum,
    }
}

fn final_down(n: &Note) -> G {
    match n.liquescent {
        Liquescent::Deminutus => G::TerminatingDesLiquescent,
        Liquescent::Augmented => G::PunctumQuadratumAscLiquescent,
        Liquescent::Diminished => G::PunctumQuadratumDesLiquescent,
        Liquescent::None => G::PunctumQuadratum,
    }
}

fn first_glyph(n: &Note) -> G {
    if n.initio_debilis {
        G::TerminatingDesLiquescent
    } else if n.shape == NoteShape::Quilisma {
        G::Quilisma
    } else {
        G::PunctumQuadratum
    }
}

/// A built neume, in coordinates whose x origin is the neume's left edge.
#[derive(Clone, Debug, Default)]
pub(crate) struct Built {
    pub pieces: Vec<Piece>,
    pub heads: Vec<Head>,
    /// Extent of the noteheads and stems, not counting markings.
    pub width: f32,
}

/// Builds one neume. `next_position` is the first note after this neume, for an oriscus that
/// points automatically. `note_base` is the score-wide index of `notes[0]`.
pub(crate) fn build(kind: Kind, notes: &[Note], next_position: Option<i8>, note_base: u32) -> Built {
    let mut b = Builder::new(note_base);
    let n = notes;
    match kind {
        Kind::Punctum => {
            b.note_at(0, &n[0], single_glyph(&n[0]), true);
        }
        Kind::Apostropha => {
            b.note_at(0, &n[0], apostropha_glyph(&n[0]), true);
        }
        Kind::Virga => {
            b.virga_at(0, &n[0]);
        }
        Kind::Bivirga | Kind::Trivirga => {
            for (i, note) in n.iter().enumerate() {
                if i > 0 {
                    b.advance(INTRA);
                }
                b.virga_at(i, note);
            }
        }
        Kind::Distropha | Kind::Tristropha => {
            let g: Vec<G> = n.iter().map(apostropha_glyph).collect();
            let gap = if kind == Kind::Distropha {
                INTRA - g.iter().take(2).filter(|g| **g == G::Stropha).count() as f32 * INTRA / 4.0
            } else if g[0] == G::Stropha {
                INTRA / 2.0
            } else {
                INTRA
            };
            for (i, note) in n.iter().enumerate() {
                if i > 0 {
                    b.advance(gap);
                }
                b.note_at(i, note, g[i], true);
            }
        }
        Kind::Oriscus => {
            let note = &n[0];
            let g = if note.liquescent != Liquescent::None {
                G::OriscusLiquescent
            } else {
                match note.orientation {
                    Some(true) => G::OriscusAsc,
                    Some(false) => G::OriscusDes,
                    None if next_position.is_some_and(|p| p > note.position) => G::OriscusAsc,
                    None => G::OriscusDes,
                }
            };
            b.note_at(0, note, g, true);
        }
        Kind::PunctaInclinata => {
            b.inclinata(0, n);
        }
        Kind::Podatus => {
            b.podatus(0, &n[0], 1, &n[1]);
        }
        Kind::Clivis => {
            b.clivis(0, &n[0], 1, &n[1]);
        }
        Kind::Climacus => {
            b.virga_at(0, &n[0]).advance(INTRA).inclinata(1, &n[1..]);
        }
        Kind::Ancus => {
            let (upper, middle, lower) = (&n[0], &n[1], &n[2]);
            b.clivis_upper(0, upper, Some(middle), G::PunctumQuadratum);
            let mg = if small(lower) {
                G::BeginningDesLiquescent
            } else {
                G::PunctumQuadratum
            };
            if upper.position - middle.position > 1 {
                b.clivis_upper(1, middle, Some(upper), mg);
            } else {
                b.clivis_upper(1, middle, None, mg);
            }
            b.clivis_lower(2, lower);
        }
        Kind::Porrectus => {
            let (second, third) = (&n[1], &n[2]);
            let tg = match third.liquescent {
                Liquescent::Deminutus => G::TerminatingAscLiquescent,
                Liquescent::Diminished => G::PunctumQuadratumDesLiquescent,
                _ if third.position - second.position > 1 => G::PodatusUpper,
                _ => G::PodatusUpperShort,
            };
            b.line_from(second).porrectus_swash(0, &n[0], 1, second).note_at(2, third, tg, true);
        }
        Kind::PorrectusFlexus => {
            let fourth = &n[3];
            let (tg, fg) = if small(fourth) {
                (G::PunctumQuadratumDesLiquescent, G::TerminatingDesLiquescent)
            } else {
                (G::PunctumQuadratum, final_down(fourth))
            };
            b.line_from(&n[1])
                .porrectus_swash(0, &n[0], 1, &n[1])
                .note_at(2, &n[2], tg, true)
                .note_at(3, fourth, fg, true);
        }
        Kind::PesQuassus => {
            let (lower, upper) = (&n[0], &n[1]);
            let lg = if oriscus(lower) { G::OriscusAsc } else { G::PunctumQuadratum };
            b.note_at(0, lower, lg, true);
            if upper.position - lower.position == 1 {
                b.virga_at(1, upper);
            } else if upper.liquescent == Liquescent::Diminished {
                b.note_at(1, upper, G::PunctumQuadratumDesLiquescent, true).line_ending_at(lower);
            } else {
                b.note_at(1, upper, G::PunctumQuadratum, true).line_ending_at(lower);
            }
        }
        Kind::PesSubpunctis => {
            b.podatus(0, &n[0], 1, &n[1]).advance(PES_SUBPUNCTIS_GAP).inclinata(2, &n[2..]);
        }
        Kind::Salicus | Kind::SalicusFlexus => {
            b.note_at(0, &n[0], G::PunctumQuadratum, true);
            if n[1].shape != NoteShape::OriscusScapus {
                b.advance(INTRA);
            }
            b.note_at(1, &n[1], G::OriscusAsc, true);
            if kind == Kind::Salicus {
                let third = &n[2];
                match third.liquescent {
                    Liquescent::Deminutus => {
                        b.note_at(2, third, G::TerminatingAscLiquescent, true);
                    }
                    Liquescent::Augmented => {
                        b.note_at(2, third, G::PunctumQuadratumAscLiquescent, true);
                    }
                    Liquescent::Diminished => {
                        b.note_at(2, third, G::PunctumQuadratumDesLiquescent, true);
                    }
                    Liquescent::None => {
                        b.virga_at(2, third);
                    }
                }
            } else {
                let fourth = &n[3];
                let tg = if small(fourth) {
                    G::PunctumQuadratumDesLiquescent
                } else {
                    G::PunctumQuadratum
                };
                b.note_at(2, &n[2], tg, true).note_at(3, fourth, final_down(fourth), true);
            }
        }
        Kind::Scandicus => {
            if virga(&n[2]) {
                b.podatus(0, &n[0], 1, &n[1]).virga_at(2, &n[2]);
            } else {
                let fg = if n[0].shape == NoteShape::Quilisma {
                    G::Quilisma
                } else {
                    G::PunctumQuadratum
                };
                b.note_at(0, &n[0], fg, true).podatus(1, &n[1], 2, &n[2]);
            }
        }
        Kind::ScandicusFlexus => {
            if virga(&n[2]) {
                b.podatus(0, &n[0], 1, &n[1]).advance(INTRA).clivis(2, &n[2], 3, &n[3]);
            } else {
                let fg = match n[3].liquescent {
                    Liquescent::Augmented => G::PunctumQuadratumAscLiquescent,
                    Liquescent::Diminished => G::PunctumQuadratumDesLiquescent,
                    _ => G::PunctumQuadratum,
                };
                b.note_at(0, &n[0], G::PunctumQuadratum, true)
                    .podatus(1, &n[1], 2, &n[2])
                    .advance(INTRA)
                    .note_at(3, &n[3], fg, true);
            }
        }
        Kind::Torculus => {
            b.note_at(0, &n[0], first_glyph(&n[0]), true)
                .note_at(1, &n[1], G::PunctumQuadratum, true)
                .note_at(2, &n[2], final_down(&n[2]), true);
        }
        Kind::TorculusResupinus => {
            let fourth = &n[3];
            let fg = if small(fourth) {
                G::TerminatingAscLiquescent
            } else if n[2].liquescent == Liquescent::Diminished {
                G::PunctumQuadratumDesLiquescent
            } else {
                G::PodatusUpper
            };
            b.note_at(0, &n[0], first_glyph(&n[0]), true)
                .porrectus_swash(1, &n[1], 2, &n[2])
                .note_at(3, fourth, fg, true);
        }
        Kind::TorculusResupinusFlexus => {
            let fifth = &n[4];
            let (fg, vg) = if small(fifth) {
                (G::PunctumQuadratumDesLiquescent, G::TerminatingDesLiquescent)
            } else {
                (G::PunctumQuadratum, final_down(fifth))
            };
            b.note_at(0, &n[0], first_glyph(&n[0]), true)
                .porrectus_swash(1, &n[1], 2, &n[2])
                .note_at(3, &n[3], fg, true)
                .note_at(4, fifth, vg, true);
        }
    }
    let width = extent(&b.pieces).map_or(0.0, |(_, r)| r.max(0.0));
    trim_swash_boxes(&mut b.heads);
    Built {
        pieces: b.pieces,
        heads: b.heads,
        width,
    }
}

/// Horizontal ink extent of a set of pieces.
pub(crate) fn extent<'a>(pieces: impl IntoIterator<Item = &'a Piece>) -> Option<(f32, f32)> {
    let mut out: Option<(f32, f32)> = None;
    for p in pieces {
        let (l, r) = match p.mark {
            Mark::Glyph { glyph, x, .. } => {
                let (a, _, c, _) = glyph.ink();
                (x + a, x + c)
            }
            Mark::Rect { x, w, .. } => (x, x + w),
        };
        out = Some(match out {
            Some((ol, or)) => (ol.min(l), or.max(r)),
            None => (l, r),
        });
    }
    out
}

/// Default episema and mora placements for a neume, following exsurge's `positionMarkings`.
/// Returns the placement of each note's episema (`Auto` resolved) by note index.
pub(crate) fn episema_placements(kind: Kind, n: &[Note]) -> Vec<Placement> {
    use Placement::{Above, Below};
    let mut out: Vec<Placement> = n.iter().map(|x| x.episema.map_or(Placement::Auto, |e| e.placement)).collect();
    let has = |i: usize| n.get(i).is_some_and(|x| x.episema.is_some());
    let set = |out: &mut Vec<Placement>, i: usize, p: Placement| {
        if let Some(v) = out.get_mut(i)
            && *v == Placement::Auto
        {
            *v = p;
        }
    };
    let clivis = |out: &mut Vec<Placement>, a: usize, b: usize| -> bool {
        let top = has(a);
        set(out, a, Above);
        set(out, b, if top { Above } else { Below });
        top
    };
    let podatus = |out: &mut Vec<Placement>, lo: usize, hi: usize| {
        set(out, lo, Below);
        set(out, hi, Above);
    };
    match kind {
        Kind::Apostropha => {}
        Kind::Podatus | Kind::PesQuassus => podatus(&mut out, 0, 1),
        Kind::PesSubpunctis => {
            podatus(&mut out, 0, 1);
            for i in 2..n.len() {
                set(&mut out, i, Above);
            }
        }
        Kind::Clivis => {
            clivis(&mut out, 0, 1);
        }
        Kind::Ancus => {
            clivis(&mut out, 0, 2);
            clivis(&mut out, 1, 2);
        }
        Kind::Torculus => {
            let top = clivis(&mut out, 1, 2);
            set(&mut out, 0, if top { Above } else { Below });
        }
        Kind::Porrectus => {
            clivis(&mut out, 0, 1);
            podatus(&mut out, 1, 2);
        }
        Kind::PorrectusFlexus => {
            let mut top = has(0);
            set(&mut out, 0, Above);
            top = clivis(&mut out, 2, 3) || top;
            set(&mut out, 1, if top { Above } else { Below });
        }
        Kind::TorculusResupinus => {
            clivis(&mut out, 1, 2);
            podatus(&mut out, 2, 3);
            clivis(&mut out, 1, 0);
        }
        Kind::TorculusResupinusFlexus => {
            let mut top = has(1);
            set(&mut out, 1, Above);
            top = clivis(&mut out, 3, 4) || top;
            set(&mut out, 2, if top { Above } else { Below });
            clivis(&mut out, 1, 0);
        }
        Kind::Salicus => {
            for i in 0..n.len() {
                set(&mut out, i, Below);
            }
        }
        Kind::SalicusFlexus => {
            let top = clivis(&mut out, 2, 3);
            set(&mut out, 1, if top { Above } else { Below });
            set(&mut out, 0, if top { Above } else { Below });
        }
        Kind::Scandicus => {
            if virga(&n[2]) {
                podatus(&mut out, 0, 1);
                set(&mut out, 2, Above);
            } else {
                set(&mut out, 0, Below);
                podatus(&mut out, 1, 2);
            }
        }
        Kind::ScandicusFlexus => {
            if virga(&n[2]) {
                podatus(&mut out, 0, 1);
                clivis(&mut out, 2, 3);
            } else {
                set(&mut out, 0, Below);
                podatus(&mut out, 1, 2);
                set(&mut out, 3, Above);
            }
        }
        _ => {
            for i in 0..n.len() {
                set(&mut out, i, Above);
            }
        }
    }
    for p in &mut out {
        if *p == Placement::Auto {
            *p = Above;
        }
    }
    out
}

/// Mora placements exsurge adjusts by context: a line note followed down a step in a clivis,
/// a pes whose lower note is on a line, and the last of a descending inclinata pair.
pub(crate) fn mora_placements(kind: Kind, n: &[Note]) -> Vec<Placement> {
    let mut out: Vec<Placement> = n.iter().map(|x| x.mora_placement).collect();
    let mut below = |out: &mut Vec<Placement>, i: usize| {
        if out[i] == Placement::Auto {
            out[i] = Placement::Below;
        }
    };
    let clivis = |out: &mut Vec<Placement>, a: usize, b: usize, below: &mut dyn FnMut(&mut Vec<Placement>, usize)| {
        if n[b].morae > 0 && n[a].position - n[b].position == 1 && odd(n[b].position) {
            below(out, b);
        }
    };
    let podatus = |out: &mut Vec<Placement>, lo: usize, below: &mut dyn FnMut(&mut Vec<Placement>, usize)| {
        if odd(n[lo].position) && n[lo].morae > 0 {
            below(out, lo);
        }
    };
    match kind {
        Kind::Clivis => clivis(&mut out, 0, 1, &mut below),
        Kind::Ancus => {
            clivis(&mut out, 0, 2, &mut below);
            clivis(&mut out, 1, 2, &mut below);
        }
        Kind::Torculus => clivis(&mut out, 1, 2, &mut below),
        Kind::Podatus | Kind::Porrectus => {
            let lo = if kind == Kind::Podatus { 0 } else { 1 };
            podatus(&mut out, lo, &mut below);
        }
        Kind::PorrectusFlexus => clivis(&mut out, 2, 3, &mut below),
        Kind::TorculusResupinusFlexus => clivis(&mut out, 3, 4, &mut below),
        Kind::Climacus | Kind::PunctaInclinata | Kind::PesSubpunctis => {
            let k = n.len();
            if k >= 2 && n[k - 1].position < n[k - 2].position {
                let (top, bottom) = (&n[k - 2], &n[k - 1]);
                if odd(bottom.position) && top.position - bottom.position == 1 && bottom.morae > 0 {
                    below(&mut out, k - 1);
                }
            }
        }
        _ => {}
    }
    out
}

/// Adds episemata, morae, ictus and ledger lines to a built neume. `trailing_zero` says the
/// next notation follows with no space, so a mora on the last note looks at its first note.
pub(crate) fn add_markings(built: &mut Built, kind: Kind, notes: &[Note], note_base: u32) {
    let episemata = episema_placements(kind, notes);
    let morae = mora_placements(kind, notes);
    let heads = built.heads.clone();
    let ledger = |p: i8| -> Option<i8> {
        if p >= 5 {
            Some(5)
        } else if p <= -5 {
            Some(-5)
        } else {
            None
        }
    };
    for (hi, h) in heads.iter().enumerate() {
        let note = &notes[h.index];
        let id = Some(note_base + h.index as u32);
        // Horizontal episema.
        if note.episema.is_some() {
            let below = episemata[h.index] == Placement::Below;
            let lp = ledger(h.position);
            let y = episema_y(h, below, lp);
            let (mut x, mut w) = match h.glyph {
                Some(G::Porrectus1 | G::Porrectus2 | G::Porrectus3 | G::Porrectus4) => (h.x, 1.0),
                None => (h.x - 1.0, 1.0),
                _ => (h.x, h.w),
            };
            let shorten = h.glyph == Some(G::PunctumInclinatum)
                && hi > 0
                && heads[hi - 1].glyph == Some(G::PunctumInclinatum)
                && heads[hi - 1].position - h.position == 1;
            if shorten {
                w *= SHORT_EPISEMA;
                x += 0.5 * w;
            } else if h.glyph == Some(G::PunctumInclinatumLiquescent) {
                w *= SHORT_EPISEMA;
                x += 0.25 * w;
            }
            built.pieces.push(Piece {
                mark: Mark::Rect {
                    x,
                    y: y - EPISEMA / 2.0,
                    w,
                    h: EPISEMA,
                },
                role: Ink::Episema,
                note: id,
                through: None,
            });
        }
        // Ictus (vertical episema).
        if let Some(p) = note.ictus {
            let above = p == Placement::Above;
            let sp = h.position + if above { 1 } else { -1 };
            let epi_same = note.episema.is_some() && (episemata[h.index] == Placement::Above) == above;
            let line_hit = odd(sp) && (sp.abs() < 4 || ledger(h.position) == Some(sp));
            let mut extra = 0.0;
            if h.glyph == Some(G::PunctumInclinatum) && !line_hit && !epi_same {
                extra = ICTUS_PAST_INCLINATUM;
            }
            if epi_same {
                extra = ICTUS_PAST_EPISEMA;
            }
            let mut off = extra + if line_hit { ICTUS_OFF_LINE } else { ICTUS_IN_SPACE };
            if above {
                off = -off;
            }
            let glyph = if above { G::VerticalEpisemaAbove } else { G::VerticalEpisemaBelow };
            let x = h.center();
            built.pieces.push(Piece {
                mark: Mark::Glyph {
                    glyph,
                    x,
                    y: -(sp as f32) + off,
                },
                role: Ink::Ictus,
                note: id,
                through: None,
            });
        }
        // Morae.
        if note.morae > 0 {
            let mora = G::Mora;
            let mut hoff = 0.5 + mora.origin_x();
            let mut next_pos = None;
            if let Some(next) = heads.get(hi + 1) {
                let nn = &notes[next.index];
                if nn.morae > 0 && hi + 2 == heads.len() {
                    hoff += next.right() - h.right();
                } else if next.right() > h.right() {
                    hoff = (next.right() - h.right() - mora.width()) / 2.0;
                    next_pos = Some(next.position);
                }
            }
            let even = !odd(h.position);
            let voff = match morae[h.index] {
                Placement::Above => {
                    if even {
                        -1.75
                    } else {
                        -0.75
                    }
                }
                Placement::Below => {
                    if even {
                        1.75
                    } else {
                        0.75
                    }
                }
                Placement::Auto => {
                    if !even {
                        -0.75
                    } else if next_pos == Some(h.position - 1) {
                        -0.25
                    } else {
                        0.0
                    }
                }
            };
            for k in 0..note.morae {
                let x = h.right() + hoff + mora.origin_x() + k as f32 * 0.6;
                built.pieces.push(Piece {
                    mark: Mark::Glyph {
                        glyph: mora,
                        x,
                        y: -(h.position as f32) + voff,
                    },
                    role: Ink::Mora,
                    note: id,
                    through: None,
                });
            }
        }
    }
    // Ledger lines, one per head that needs them, merged where heads share a line.
    let mut ledgers: Vec<(i8, f32, f32, u32)> = Vec::new();
    for h in &heads {
        let lines: Vec<i8> = if h.position >= 5 {
            (5..=h.position).step_by(2).collect()
        } else if h.position <= -5 {
            (h.position..=-5).filter(|p| odd(*p)).collect()
        } else {
            Vec::new()
        };
        let (l, r) = if h.glyph.is_none() { (h.x - 1.0, h.x) } else { (h.x, h.right()) };
        for lp in lines {
            // Lines that would meet, or all but meet, are drawn as one.
            if let Some(e) = ledgers
                .iter_mut()
                .find(|e| e.0 == lp && e.2 + 2.0 * LEDGER_OVERHANG + LEDGER_GAP >= l)
            {
                e.2 = e.2.max(r);
            } else {
                ledgers.push((lp, l, r, note_base + h.index as u32));
            }
        }
    }
    for (lp, l, r, id) in ledgers {
        built.pieces.push(Piece {
            mark: Mark::Rect {
                x: l - LEDGER_OVERHANG,
                y: -(lp as f32) - STEM / 2.0,
                w: r - l + 2.0 * LEDGER_OVERHANG,
                h: STEM,
            },
            role: Ink::Ledger,
            note: Some(id),
            through: None,
        });
    }
}

/// Shortens the ledger lines among `pieces` that would run into an accidental or an ictus
/// beside their notes, to stop [`LEDGER_CLEAR`] short of it, though never closer to the notes
/// than the overhang a ledger line needs to read as one.
pub(crate) fn clear_ledgers(pieces: &mut [Piece]) {
    const MIN_OVERHANG: f32 = 0.25;
    if !pieces.iter().any(|p| p.role == Ink::Ledger) {
        return;
    }
    let others: Vec<[f32; 4]> = pieces
        .iter()
        .filter(|p| matches!(p.role, Ink::Accidental | Ink::Ictus))
        .map(Piece::ink_box)
        .collect();
    for p in pieces.iter_mut().filter(|p| p.role == Ink::Ledger) {
        let Mark::Rect { x, y, w, h } = &mut p.mark else { continue };
        let (notes_l, notes_r) = (*x + LEDGER_OVERHANG, *x + *w - LEDGER_OVERHANG);
        let (mut l, mut r) = (*x, *x + *w);
        for o in others.iter().filter(|o| o[1] < *y + *h && o[3] > *y) {
            if o[2] <= notes_l && o[2] + LEDGER_CLEAR > l {
                l = (o[2] + LEDGER_CLEAR).min(notes_l - MIN_OVERHANG);
            }
            if o[0] >= notes_r && o[0] - LEDGER_CLEAR < r {
                r = (o[0] - LEDGER_CLEAR).max(notes_r + MIN_OVERHANG);
            }
        }
        *x = l;
        *w = r - l;
    }
}

/// exsurge's episema height: centered between the note and the next line when there's room,
/// otherwise a third of the way between lines, never on a line.
fn episema_y(h: &Head, below: bool, ledger: Option<i8>) -> f32 {
    let gap = 0.25;
    let on_ledger = |step: f32| ledger.is_some_and(|lp| -(lp as f32) == step);
    if below {
        let mut y = h.bottom + gap;
        if h.glyph.is_none() {
            y += 0.5;
        }
        let mut step = y.ceil();
        if step.abs() % 2.0 == 0.0 {
            step = (step + 0.75 + (y - gap)) / 2.0;
        } else {
            step = ((1.5 * y - 0.5).ceil() * 2.0 + 1.0) / 3.0;
            if step.abs() % 2.0 == 1.0 {
                step += if step.abs() < 4.0 || on_ledger(step) {
                    2.0 / 3.0
                } else {
                    1.0 / 3.0
                };
            }
        }
        step
    } else {
        let y = h.top - gap;
        let mut step = y.floor();
        if step.abs() % 2.0 == 0.0 {
            step = (step - 0.75 + (y + gap)) / 2.0;
        } else {
            step = ((1.5 * y - 0.5).floor() * 2.0 + 1.0) / 3.0;
            if step.abs() % 2.0 == 1.0 {
                step -= if step.abs() < 4.0 || on_ledger(step) {
                    2.0 / 3.0
                } else {
                    1.0 / 3.0
                };
            }
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gabc::parse;
    use crate::score::Figure;

    fn kinds(notes: &str) -> Vec<Kind> {
        let p = parse(&format!("(c4) a({notes})"));
        let mut ns: Vec<Note> = p.score.syllables[1]
            .notation
            .iter()
            .filter_map(|f| if let Figure::Note(n) = f { Some(n.clone()) } else { None })
            .collect();
        split(&mut ns).into_iter().map(|s| s.kind).collect()
    }

    #[test]
    fn classifies_common_neumes() {
        use Kind::*;
        assert_eq!(kinds("g"), [Punctum]);
        assert_eq!(kinds("gh"), [Podatus]);
        assert_eq!(kinds("hg"), [Clivis]);
        assert_eq!(kinds("ghg"), [Torculus]);
        assert_eq!(kinds("hgh"), [Porrectus]);
        assert_eq!(kinds("hghg"), [PorrectusFlexus]);
        assert_eq!(kinds("ghi"), [Scandicus]);
        assert_eq!(kinds("ghih"), [ScandicusFlexus]);
        assert_eq!(kinds("hvGF"), [Climacus]);
        assert_eq!(kinds("ghGF"), [PesSubpunctis]);
        assert_eq!(kinds("gvhv"), [Virga, Virga]);
        assert_eq!(kinds("gvgv"), [Bivirga]);
        assert_eq!(kinds("gsgsgs"), [Tristropha]);
        assert_eq!(kinds("gsgsgsgs"), [Distropha, Distropha]);
        // exsurge draws a salicus as a punctum and a pes quassus.
        assert_eq!(kinds("fgoh"), [Punctum, PesQuassus]);
        assert_eq!(kinds("goh"), [PesQuassus]);
        assert_eq!(kinds("ghgh"), [TorculusResupinus]);
        assert_eq!(kinds("ghghg"), [TorculusResupinusFlexus]);
        assert_eq!(kinds("hgf~"), [Ancus]);
        assert_eq!(kinds("gwh"), [Podatus]);
        assert_eq!(kinds("fgwh"), [Punctum, Podatus]);
    }

    #[test]
    fn podatus_matches_exsurge_geometry() {
        // exsurge draws (hj) as PodatusLower at 0.5, a stem at 0.872 and PodatusUpper at 1.0.
        let p = parse("(c4) a(hj)");
        let ns: Vec<Note> = p.score.syllables[1]
            .notation
            .iter()
            .filter_map(|f| if let Figure::Note(n) = f { Some(n.clone()) } else { None })
            .collect();
        let b = build(Kind::Podatus, &ns, None, 0);
        let glyphs: Vec<(G, f32)> = b
            .pieces
            .iter()
            .filter_map(|p| {
                if let Mark::Glyph { glyph, x, .. } = p.mark {
                    Some((glyph, x))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(glyphs, [(G::PodatusLower, 0.5), (G::PodatusUpper, 1.0)]);
        let rects: Vec<f32> = b
            .pieces
            .iter()
            .filter_map(|p| if let Mark::Rect { x, .. } = p.mark { Some(x) } else { None })
            .collect();
        assert_eq!(rects.len(), 1);
        assert!((rects[0] - 0.872).abs() < 1e-6);
    }
}
