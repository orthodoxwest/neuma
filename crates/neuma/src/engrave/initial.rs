//! The drop-cap initial and the annotations above it (docs/DESIGN.md,
//! "Lyrics and text measurement", under Initials).

use crate::score::{Header, Lyric};

/// The tallest initial, in staves.
pub(crate) const MAX_LINES: u8 = 4;

/// Cap height of the lyric face, in ems (EB Garamond's is 0.65). It sizes the initial so its
/// capital spans the staves.
pub(crate) const CAP_HEIGHT: f32 = 0.65;

/// How far a capital's ink runs past its advance, in ems, on the left and on the right: the
/// tail of EB Garamond's Q sweeps well past it, and would reach the staff beside it.
pub(crate) fn overhang(initial: &str) -> (f32, f32) {
    let side = |c: Option<char>, table: &[(char, f32)]| c.and_then(|c| table.iter().find(|e| e.0 == c)).map_or(0.0, |e| e.1);
    let left = side(initial.chars().next(), &[('J', 0.07)]);
    let right = side(
        initial.chars().find(|c| c.is_alphabetic()),
        &[('Q', 0.12), ('Y', 0.045), ('W', 0.015)],
    );
    (left, right)
}

/// How far an initial's ink reaches below the baseline, in ems: the tails of Q and J, and a
/// lowercase descender.
pub(crate) fn depth(initial: &str) -> f32 {
    match initial.chars().next() {
        Some('Q') => 0.27,
        Some('J') => 0.21,
        Some('g' | 'j' | 'p' | 'q' | 'y' | 'ç' | 'ý' | 'ÿ' | 'Ç' | 'Ą' | 'Ę' | 'Į' | 'Ų') => 0.3,
        _ => 0.02,
    }
}

/// How far an initial's ink rises above its cap height, in ems: an accent, a lowercase
/// ascender, the apex of an A, the serifs of a T. Measured from the letter's outline in both
/// built-in faces ([`RISE_LETTERS`]); another Latin, Greek or Cyrillic letter rises by no more
/// than its overshoot ([`OVERSHOOT`]), and a letter of any other script by [`OTHER_SCRIPT_RISE`].
/// Marks after the letter are taken as an accent ([`MARK_RISE`]), or two stacked
/// ([`STACKED_MARKS_RISE`]).
pub(crate) fn rise(initial: &str) -> f32 {
    let mut chars = initial.chars();
    let base = chars.next().map_or(OVERSHOOT, |c| match RISE_LETTERS.chars().position(|l| l == c) {
        Some(i) => f32::from(RISE[i]) / 100.0,
        None if latin_greek_or_cyrillic(c) => OVERSHOOT,
        None => OTHER_SCRIPT_RISE,
    });
    match chars.filter(|&m| is_mark(m)).count() {
        0 => base,
        1 => base.max(MARK_RISE),
        _ => base.max(STACKED_MARKS_RISE),
    }
}

/// The least advance of an initial in a script the built-in faces don't have, in ems. Such a
/// letter measures as a missing character, half an em, but is drawn from whatever font the
/// renderer falls back to: an ideograph, kana or Hangul a full em wide, and the letters of
/// other scripts up to that (Unifont draws Khmer a full em too).
const OTHER_SCRIPT_ADVANCE: f32 = 1.0;

/// The advance of `initial` as `measured` (in ems), at least [`OTHER_SCRIPT_ADVANCE`] for a
/// letter outside the Latin, Greek and Cyrillic scripts.
pub(crate) fn advance(initial: &str, measured: f32) -> f32 {
    if initial.chars().next().is_some_and(|c| !latin_greek_or_cyrillic(c)) {
        measured.max(OTHER_SCRIPT_ADVANCE)
    } else {
        measured
    }
}

/// Whether `c` is a mark (general category Mn, Mc or Me), which an initial takes with its
/// letter: a combining accent, or a dependent vowel or sign of the Brahmic scripts.
pub(crate) fn is_mark(c: char) -> bool {
    let c = u32::from(c);
    // MARKS holds inclusive ranges as pairs; find the last range starting at or before c.
    let starts = MARKS.len() / 2;
    let (mut lo, mut hi) = (0, starts);
    while lo < hi {
        let mid = usize::midpoint(lo, hi);
        if MARKS[2 * mid] <= c { lo = mid + 1 } else { hi = mid }
    }
    lo > 0 && c <= MARKS[2 * (lo - 1) + 1]
}

/// The characters of general category Mark (Mn, Mc, Me) in Unicode 14.0, as inclusive
/// ranges: start, end, start, end, …
#[rustfmt::skip]
const MARKS: [u32; 598] = [
    0x0300, 0x036F, 0x0483, 0x0489, 0x0591, 0x05BD, 0x05BF, 0x05BF, 0x05C1, 0x05C2, 0x05C4, 0x05C5, 0x05C7, 0x05C7,
    0x0610, 0x061A, 0x064B, 0x065F, 0x0670, 0x0670, 0x06D6, 0x06DC, 0x06DF, 0x06E4, 0x06E7, 0x06E8, 0x06EA, 0x06ED,
    0x0711, 0x0711, 0x0730, 0x074A, 0x07A6, 0x07B0, 0x07EB, 0x07F3, 0x07FD, 0x07FD, 0x0816, 0x0819, 0x081B, 0x0823,
    0x0825, 0x0827, 0x0829, 0x082D, 0x0859, 0x085B, 0x0898, 0x089F, 0x08CA, 0x08E1, 0x08E3, 0x0903, 0x093A, 0x093C,
    0x093E, 0x094F, 0x0951, 0x0957, 0x0962, 0x0963, 0x0981, 0x0983, 0x09BC, 0x09BC, 0x09BE, 0x09C4, 0x09C7, 0x09C8,
    0x09CB, 0x09CD, 0x09D7, 0x09D7, 0x09E2, 0x09E3, 0x09FE, 0x09FE, 0x0A01, 0x0A03, 0x0A3C, 0x0A3C, 0x0A3E, 0x0A42,
    0x0A47, 0x0A48, 0x0A4B, 0x0A4D, 0x0A51, 0x0A51, 0x0A70, 0x0A71, 0x0A75, 0x0A75, 0x0A81, 0x0A83, 0x0ABC, 0x0ABC,
    0x0ABE, 0x0AC5, 0x0AC7, 0x0AC9, 0x0ACB, 0x0ACD, 0x0AE2, 0x0AE3, 0x0AFA, 0x0AFF, 0x0B01, 0x0B03, 0x0B3C, 0x0B3C,
    0x0B3E, 0x0B44, 0x0B47, 0x0B48, 0x0B4B, 0x0B4D, 0x0B55, 0x0B57, 0x0B62, 0x0B63, 0x0B82, 0x0B82, 0x0BBE, 0x0BC2,
    0x0BC6, 0x0BC8, 0x0BCA, 0x0BCD, 0x0BD7, 0x0BD7, 0x0C00, 0x0C04, 0x0C3C, 0x0C3C, 0x0C3E, 0x0C44, 0x0C46, 0x0C48,
    0x0C4A, 0x0C4D, 0x0C55, 0x0C56, 0x0C62, 0x0C63, 0x0C81, 0x0C83, 0x0CBC, 0x0CBC, 0x0CBE, 0x0CC4, 0x0CC6, 0x0CC8,
    0x0CCA, 0x0CCD, 0x0CD5, 0x0CD6, 0x0CE2, 0x0CE3, 0x0D00, 0x0D03, 0x0D3B, 0x0D3C, 0x0D3E, 0x0D44, 0x0D46, 0x0D48,
    0x0D4A, 0x0D4D, 0x0D57, 0x0D57, 0x0D62, 0x0D63, 0x0D81, 0x0D83, 0x0DCA, 0x0DCA, 0x0DCF, 0x0DD4, 0x0DD6, 0x0DD6,
    0x0DD8, 0x0DDF, 0x0DF2, 0x0DF3, 0x0E31, 0x0E31, 0x0E34, 0x0E3A, 0x0E47, 0x0E4E, 0x0EB1, 0x0EB1, 0x0EB4, 0x0EBC,
    0x0EC8, 0x0ECD, 0x0F18, 0x0F19, 0x0F35, 0x0F35, 0x0F37, 0x0F37, 0x0F39, 0x0F39, 0x0F3E, 0x0F3F, 0x0F71, 0x0F84,
    0x0F86, 0x0F87, 0x0F8D, 0x0F97, 0x0F99, 0x0FBC, 0x0FC6, 0x0FC6, 0x102B, 0x103E, 0x1056, 0x1059, 0x105E, 0x1060,
    0x1062, 0x1064, 0x1067, 0x106D, 0x1071, 0x1074, 0x1082, 0x108D, 0x108F, 0x108F, 0x109A, 0x109D, 0x135D, 0x135F,
    0x1712, 0x1715, 0x1732, 0x1734, 0x1752, 0x1753, 0x1772, 0x1773, 0x17B4, 0x17D3, 0x17DD, 0x17DD, 0x180B, 0x180D,
    0x180F, 0x180F, 0x1885, 0x1886, 0x18A9, 0x18A9, 0x1920, 0x192B, 0x1930, 0x193B, 0x1A17, 0x1A1B, 0x1A55, 0x1A5E,
    0x1A60, 0x1A7C, 0x1A7F, 0x1A7F, 0x1AB0, 0x1ACE, 0x1B00, 0x1B04, 0x1B34, 0x1B44, 0x1B6B, 0x1B73, 0x1B80, 0x1B82,
    0x1BA1, 0x1BAD, 0x1BE6, 0x1BF3, 0x1C24, 0x1C37, 0x1CD0, 0x1CD2, 0x1CD4, 0x1CE8, 0x1CED, 0x1CED, 0x1CF4, 0x1CF4,
    0x1CF7, 0x1CF9, 0x1DC0, 0x1DFF, 0x20D0, 0x20F0, 0x2CEF, 0x2CF1, 0x2D7F, 0x2D7F, 0x2DE0, 0x2DFF, 0x302A, 0x302F,
    0x3099, 0x309A, 0xA66F, 0xA672, 0xA674, 0xA67D, 0xA69E, 0xA69F, 0xA6F0, 0xA6F1, 0xA802, 0xA802, 0xA806, 0xA806,
    0xA80B, 0xA80B, 0xA823, 0xA827, 0xA82C, 0xA82C, 0xA880, 0xA881, 0xA8B4, 0xA8C5, 0xA8E0, 0xA8F1, 0xA8FF, 0xA8FF,
    0xA926, 0xA92D, 0xA947, 0xA953, 0xA980, 0xA983, 0xA9B3, 0xA9C0, 0xA9E5, 0xA9E5, 0xAA29, 0xAA36, 0xAA43, 0xAA43,
    0xAA4C, 0xAA4D, 0xAA7B, 0xAA7D, 0xAAB0, 0xAAB0, 0xAAB2, 0xAAB4, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1, 0xAAC1,
    0xAAEB, 0xAAEF, 0xAAF5, 0xAAF6, 0xABE3, 0xABEA, 0xABEC, 0xABED, 0xFB1E, 0xFB1E, 0xFE00, 0xFE0F, 0xFE20, 0xFE2F,
    0x101FD, 0x101FD, 0x102E0, 0x102E0, 0x10376, 0x1037A, 0x10A01, 0x10A03, 0x10A05, 0x10A06, 0x10A0C, 0x10A0F, 0x10A38, 0x10A3A,
    0x10A3F, 0x10A3F, 0x10AE5, 0x10AE6, 0x10D24, 0x10D27, 0x10EAB, 0x10EAC, 0x10F46, 0x10F50, 0x10F82, 0x10F85, 0x11000, 0x11002,
    0x11038, 0x11046, 0x11070, 0x11070, 0x11073, 0x11074, 0x1107F, 0x11082, 0x110B0, 0x110BA, 0x110C2, 0x110C2, 0x11100, 0x11102,
    0x11127, 0x11134, 0x11145, 0x11146, 0x11173, 0x11173, 0x11180, 0x11182, 0x111B3, 0x111C0, 0x111C9, 0x111CC, 0x111CE, 0x111CF,
    0x1122C, 0x11237, 0x1123E, 0x1123E, 0x112DF, 0x112EA, 0x11300, 0x11303, 0x1133B, 0x1133C, 0x1133E, 0x11344, 0x11347, 0x11348,
    0x1134B, 0x1134D, 0x11357, 0x11357, 0x11362, 0x11363, 0x11366, 0x1136C, 0x11370, 0x11374, 0x11435, 0x11446, 0x1145E, 0x1145E,
    0x114B0, 0x114C3, 0x115AF, 0x115B5, 0x115B8, 0x115C0, 0x115DC, 0x115DD, 0x11630, 0x11640, 0x116AB, 0x116B7, 0x1171D, 0x1172B,
    0x1182C, 0x1183A, 0x11930, 0x11935, 0x11937, 0x11938, 0x1193B, 0x1193E, 0x11940, 0x11940, 0x11942, 0x11943, 0x119D1, 0x119D7,
    0x119DA, 0x119E0, 0x119E4, 0x119E4, 0x11A01, 0x11A0A, 0x11A33, 0x11A39, 0x11A3B, 0x11A3E, 0x11A47, 0x11A47, 0x11A51, 0x11A5B,
    0x11A8A, 0x11A99, 0x11C2F, 0x11C36, 0x11C38, 0x11C3F, 0x11C92, 0x11CA7, 0x11CA9, 0x11CB6, 0x11D31, 0x11D36, 0x11D3A, 0x11D3A,
    0x11D3C, 0x11D3D, 0x11D3F, 0x11D45, 0x11D47, 0x11D47, 0x11D8A, 0x11D8E, 0x11D90, 0x11D91, 0x11D93, 0x11D97, 0x11EF3, 0x11EF6,
    0x16AF0, 0x16AF4, 0x16B30, 0x16B36, 0x16F4F, 0x16F4F, 0x16F51, 0x16F87, 0x16F8F, 0x16F92, 0x16FE4, 0x16FE4, 0x16FF0, 0x16FF1,
    0x1BC9D, 0x1BC9E, 0x1CF00, 0x1CF2D, 0x1CF30, 0x1CF46, 0x1D165, 0x1D169, 0x1D16D, 0x1D172, 0x1D17B, 0x1D182, 0x1D185, 0x1D18B,
    0x1D1AA, 0x1D1AD, 0x1D242, 0x1D244, 0x1DA00, 0x1DA36, 0x1DA3B, 0x1DA6C, 0x1DA75, 0x1DA75, 0x1DA84, 0x1DA84, 0x1DA9B, 0x1DA9F,
    0x1DAA1, 0x1DAAF, 0x1E000, 0x1E006, 0x1E008, 0x1E018, 0x1E01B, 0x1E021, 0x1E023, 0x1E024, 0x1E026, 0x1E02A, 0x1E130, 0x1E136,
    0x1E2AE, 0x1E2AE, 0x1E2EC, 0x1E2EF, 0x1E8D0, 0x1E8D6, 0x1E944, 0x1E94A, 0xE0100, 0xE01EF,
];

/// The rise of a letter of the built-in faces with nothing above its cap height: its
/// overshoot, in ems.
const OVERSHOOT: f32 = 0.015;

/// The rise allowed for a mark over the letter, in ems: an acute over a capital reaches about
/// 0.19 em above the cap height in both built-in faces, and none of their precomposed
/// capitals with one mark more than 0.25.
const MARK_RISE: f32 = 0.25;

/// The rise allowed for two or more marks, in ems: stacked over a capital they reach up to
/// 0.33 em above the cap height, as the precomposed Ǘ, Ǻ, Ṍ, Ẫ, Ễ and Ỗ measure (0.27 to
/// 0.33). A circumflex with a tilde, a grave or a hook above stacks; the faces stack no third
/// mark, so more marks are given no more room.
const STACKED_MARKS_RISE: f32 = 0.33;

/// The rise allowed for a letter outside the Latin, Greek and Cyrillic scripts, in ems. The
/// built-in faces don't have it, so it is drawn from whatever font the renderer falls back
/// to, and may stand well above a capital's height (a CJK ideograph fills its em square;
/// Khmer and the Indic scripts set marks above the letter): enough room that the annotation
/// clears it in common fallback fonts.
const OTHER_SCRIPT_RISE: f32 = 0.25;

/// Whether `c` is in a Latin, Greek or Cyrillic block, whose letters the built-in faces draw
/// (those that rise above the cap height are in [`RISE_LETTERS`]).
fn latin_greek_or_cyrillic(c: char) -> bool {
    matches!(
        u32::from(c),
        // Latin: Basic Latin to the IPA extensions and spacing modifiers, phonetic extensions,
        // Latin Extended Additional, C, D and E, and the Latin ligatures.
        0x0000..=0x02FF | 0x1D00..=0x1DBF | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF | 0xAB30..=0xAB6F | 0xFB00..=0xFB06
        // Greek and Coptic, and Greek Extended.
        | 0x0370..=0x03FF | 0x1F00..=0x1FFF
        // Cyrillic, its supplement, and Extended A, B and C.
        | 0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F
    )
}

/// The letters of the built-in EB Garamond faces whose ink rises more than 0.02 em above the
/// cap height (0.65 em), and by how much, in hundredths of an em rounded up ([`RISE`]): the
/// higher of the Google Fonts and EB Garamond 12 regular faces' outlines.
const RISE_LETTERS: &str = concat!(
    "ATZbdfhklÀÁÂÃÄÅÈÉÊËÌÍÎÏÑÒÓÔÕÖÙÚÛÜÝßðþĀĂĄĆĈĊČĎďđĒĔĖĚĜĞĠģĤĥħĨĪ",
    "ĬİĴķĹĺļľŀłŃŇŉŌŎŐŔŘŚŜŠŢŤŦŨŪŬŮŰŴŶŸŹŻŽſƀƃƅƇƌƒƓƕƙƚƛƠƥƩƪƬƭƮƯƵǀǁǂǄ",
    "ǆǉǍǏǑǓǕǖǗǘǙǚǛǜǞǟǠǡǢǦǨǩǬǮǱǳǴǸǺǻǼǾȀȂȄȆȈȊȌȎȐȒȔȖȚȞȟȡȢȣȦȪȫȬȭȮȰȱȲȴ",
    "ȸȺȻȾɅɆɊɓɖɗɠɦɧɫɬɭɮɺʃʄʆʎʔʕʖʠʡʢʣʤʥʧʩʪʫʰʱʹʺʻʼʽˈˡʹΆΈΉΊΌΎΏΐΑΔΖΛΞΣΤ",
    "ΦΪΫάέήίΰβδζθλξόύώϑϓϔϡЀЁЂЃЇЋЌЍЎАЙТФбђћѢѣѶҊҌҍҐҞҟҤҬҴһҾӁӏӐӒӖӚӜӞӢ",
    "ӤӦӪӬӮӰӲӴӸԧᴬᴮᴰᴱᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾᴿᵀᵁᵂᵇᵈᵏᶞᶠḀḂḃḅḇḈḊḋḍḏḑḓḔḕḖḗḜḞḟḠḢḣḥḦḧḩḫ",
    "ḮḯḰḱḳḵḷḸḹḻḽḾṀṄṌṍṎṏṐṑṒṓṔṖṘṜṠṤṥṦṧṨṪṬṮṰṸṹṺṻṼẀẂẄẆẊẌẎẐẒẔẖẛẜẝẟẠẢảẤ",
    "ấẦầẨẩẪẫẬẮắẰằẲẳẴẵẶẺẻẼẾếỀềỂểỄễỆỈỉỎỏỐốỒồỔổỖỗỘỚỜỞởỠỢỦủỨỪỬửỮỰỲỶỷỸ",
    "ỻỽἆἇἈἉἊἋἌἍἎἏἚἛἜἝἦἧἪἫἬἭἮἯἶἷἺἻἼἽἾἿὊὋὌὍὖὗὛὝὟὦὧὪὫὬὭὮὯὰάὲέὴήὶίὸόὺ",
    "ύὼώᾆᾇᾈᾉᾊᾋᾌᾍᾎᾏᾖᾗᾚᾛᾜᾝᾞᾟᾦᾧᾪᾫᾬᾭᾮᾯᾲᾴᾸᾹᾺΆᾼῂῄῈΈῊΉῒΐῗῘῙῚΊῢΰῧῨῩῪΎῲῴῸΌ",
    "ῺΏℎℓÅⱡⱨⱪⱹⱿꜧꜨꝃꝉꞎꟾꟿﬀﬁﬂﬃﬄﬅ",
);
const RISE: [u8; 623] = [
    4, 5, 3, 6, 6, 6, 6, 6, 6, 19, 19, 18, 18, 15, 20, 19, 19, 18, 15, 19, 19, 18, 15, 18, 19, 19, 18, 18, 15, 19, 19, 18, 15, 19, 6, 6, 6,
    13, 17, 4, 19, 18, 16, 18, 18, 6, 6, 13, 17, 16, 18, 18, 17, 16, 4, 18, 22, 6, 18, 13, 17, 16, 18, 6, 19, 21, 6, 6, 6, 6, 19, 20, 4,
    13, 17, 20, 19, 20, 19, 18, 20, 5, 20, 5, 18, 13, 17, 20, 20, 18, 18, 15, 19, 16, 18, 6, 6, 6, 6, 8, 7, 6, 8, 6, 6, 6, 6, 8, 6, 3, 6,
    5, 6, 5, 13, 3, 6, 6, 6, 18, 6, 6, 20, 20, 20, 20, 25, 6, 29, 11, 28, 11, 29, 11, 25, 6, 26, 6, 13, 20, 20, 16, 13, 20, 3, 6, 19, 19,
    33, 11, 19, 19, 21, 17, 21, 18, 21, 17, 21, 17, 21, 17, 21, 17, 5, 20, 21, 6, 3, 6, 16, 25, 6, 25, 6, 16, 26, 5, 13, 6, 6, 7, 7, 7, 3,
    7, 3, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 4, 5, 5, 6, 6, 5, 5, 6, 6, 6, 6, 6, 6, 6, 5, 6, 3, 3, 4, 5, 5, 6, 5, 3, 5, 5, 5, 5, 5,
    5, 5, 6, 4, 2, 3, 2, 3, 3, 5, 3, 15, 15, 4, 4, 4, 4, 6, 6, 6, 8, 5, 5, 10, 4, 4, 4, 6, 5, 15, 3, 19, 15, 5, 19, 17, 5, 19, 19, 21, 4,
    21, 5, 5, 6, 6, 6, 4, 6, 25, 21, 4, 6, 11, 4, 6, 4, 5, 5, 6, 15, 24, 6, 21, 15, 21, 15, 17, 17, 13, 15, 15, 15, 17, 13, 15, 20, 17, 17,
    6, 4, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 5, 3, 3, 6, 6, 5, 6, 5, 4, 16, 22, 6, 6, 19, 16, 22, 6, 6, 6, 6, 28, 10, 28, 10, 17,
    16, 20, 13, 16, 20, 6, 15, 18, 6, 6, 31, 11, 19, 16, 6, 6, 6, 13, 15, 6, 6, 19, 16, 16, 29, 13, 29, 8, 28, 10, 28, 10, 19, 16, 16, 13,
    16, 24, 10, 28, 10, 16, 16, 5, 5, 5, 29, 13, 28, 5, 18, 19, 19, 15, 16, 16, 15, 16, 18, 3, 3, 6, 20, 6, 6, 6, 4, 20, 3, 22, 8, 21, 8,
    24, 7, 30, 10, 18, 27, 8, 26, 8, 28, 6, 32, 8, 17, 20, 3, 18, 21, 8, 21, 8, 24, 7, 29, 10, 18, 20, 3, 20, 3, 21, 8, 21, 8, 24, 7, 29,
    9, 18, 19, 19, 20, 3, 18, 8, 20, 3, 19, 19, 20, 3, 18, 13, 19, 20, 3, 18, 6, 6, 8, 8, 4, 4, 5, 5, 5, 5, 10, 10, 5, 5, 5, 5, 8, 8, 5, 5,
    5, 5, 10, 10, 8, 8, 5, 5, 5, 5, 10, 10, 5, 5, 5, 5, 8, 8, 5, 5, 10, 8, 8, 5, 5, 5, 5, 10, 10, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    8, 8, 4, 4, 5, 5, 5, 5, 10, 10, 8, 8, 5, 5, 5, 5, 10, 10, 8, 8, 5, 5, 5, 5, 10, 10, 4, 4, 17, 13, 5, 5, 4, 4, 4, 5, 5, 5, 5, 6, 6, 10,
    17, 13, 5, 5, 6, 6, 10, 17, 13, 5, 5, 4, 4, 5, 5, 5, 5, 6, 6, 20, 6, 6, 6, 6, 3, 6, 5, 6, 6, 6, 10, 2, 6, 6, 6, 6, 6, 6,
];

/// Whether to set the score's first letter as a drop cap, and how many staves tall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Initial {
    /// No initial: the first syllable is set as the others are.
    None,
    /// The initial spans this many staves (at most 4, and no more than the layout has): its
    /// cap height runs from the top line of the first staff to the bottom line of the last.
    /// `Lines(1)`, the default, stands on the first line's lyric baseline instead, as a
    /// one-line initial does in Solesmes books: its cap height runs from the staff's top line
    /// down to the lyrics.
    Lines(u8),
}

impl Initial {
    /// An initial `staves` staves tall, as the bindings and the CLI take a count: 0 or less
    /// for none, and more than 4 for 4.
    #[must_use]
    pub fn from_staves(staves: i64) -> Initial {
        match staves {
            ..=0 => Initial::None,
            n => Initial::Lines(n.min(i64::from(MAX_LINES)) as u8),
        }
    }
}

impl Default for Initial {
    fn default() -> Initial {
        Initial::Lines(1)
    }
}

/// Splits the drop cap off a lyric: the first letter and any combining marks after it, and
/// the rest of the lyric with its explicit center shifted to match. `None` when the lyric
/// doesn't start with a letter.
pub(crate) fn split_initial(lyric: &Lyric) -> Option<(String, Lyric)> {
    let first = lyric.runs.iter().position(|r| !r.text.is_empty())?;
    let run = &lyric.runs[first];
    if run.consonant {
        return None;
    }
    let mut chars = run.text.chars();
    let c = chars.next()?;
    if !c.is_alphabetic() {
        return None;
    }
    let mut initial = c.to_string();
    let mut taken = 1;
    let rest: String = {
        let mut rest = chars.peekable();
        while let Some(&m) = rest.peek() {
            if !is_mark(m) {
                break;
            }
            initial.push(m);
            rest.next();
            taken += 1;
        }
        rest.collect()
    };
    let mut out = lyric.clone();
    out.runs[first].text = rest;
    out.runs.retain(|r| !r.text.is_empty());
    out.center = lyric
        .center
        .as_ref()
        .filter(|r| r.start >= taken)
        .map(|r| r.start - taken..r.end - taken);
    Some((initial, out))
}

/// The lines shown above the initial: the `annotation` headers (at most two, the first on
/// top), or else the mode written as a lower-case roman numeral, as GregorioTeX prints it,
/// with its modifier and differentia.
pub(crate) fn annotations(header: &Header) -> Vec<String> {
    let given: Vec<String> = header
        .get_all("annotation")
        .map(strip_tex)
        .filter(|a| !a.is_empty())
        .take(2)
        .collect();
    if !given.is_empty() {
        return given;
    }
    let Some(mode) = header.get("mode").map(strip_tex).filter(|m| !m.is_empty()) else {
        return Vec::new();
    };
    let mut line = match mode.parse::<u32>() {
        Ok(n) if (1..=8).contains(&n) => ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii"][n as usize - 1].to_string(),
        _ => mode,
    };
    for name in ["mode-modifier", "mode-differentia"] {
        if let Some(v) = header.get(name).map(strip_tex).filter(|v| !v.is_empty()) {
            line.push(' ');
            line.push_str(&v);
        }
    }
    vec![line]
}

/// Plain text from a header value that may hold TeX: commands and braces are dropped.
pub(crate) fn strip_tex(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // A control word, or a control symbol such as `\&`, which keeps its character.
                if chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                        chars.next();
                    }
                    if chars.peek() == Some(&' ') {
                        chars.next();
                    }
                } else if let Some(s) = chars.next() {
                    out.push(s);
                }
            }
            '{' | '}' => {}
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn splits_the_first_letter() {
        let lyric = Lyric::from_plain("Pu");
        let (i, rest) = split_initial(&lyric).unwrap();
        assert_eq!((i.as_str(), rest.plain().as_str()), ("P", "u"));
        let lyric = Lyric::from_plain("E\u{0301}x");
        let (i, rest) = split_initial(&lyric).unwrap();
        assert_eq!((i.as_str(), rest.plain().as_str()), ("E\u{0301}", "x"));
        let lyric = Lyric {
            center: Some(1..2),
            ..Lyric::from_plain("Ky")
        };
        assert_eq!(split_initial(&lyric).unwrap().1.center, Some(0..1));
        let (i, rest) = split_initial(&Lyric::from_plain("A")).unwrap();
        assert_eq!((i.as_str(), rest.is_empty()), ("A", true));
        assert!(split_initial(&Lyric::from_plain("1. A")).is_none());
    }

    #[test]
    fn rise_is_measured_per_letter() {
        assert_eq!(RISE_LETTERS.chars().count(), RISE.len());
        assert_eq!(rise("H"), 0.015);
        assert_eq!(rise("T"), 0.05);
        assert_eq!(rise("h"), 0.06);
        assert_eq!(rise("É"), 0.19);
        // Capitals beyond ASCII with nothing above them.
        for c in ["Æ", "Đ", "Χ", "Œ", "Ç"] {
            assert!(rise(c) < 0.03, "{c}");
        }
        assert_eq!(rise("E\u{0301}"), 0.25);
    }

    #[test]
    fn rise_allows_for_other_scripts_and_marks() {
        // Not in the built-in faces: drawn from a fallback font, often taller than a capital.
        for c in ["天", "ស", "ա", "א", "ა"] {
            assert_eq!(rise(c), OTHER_SCRIPT_RISE, "{c}");
        }
        // Latin, Greek and Cyrillic letters the table doesn't list rise by their overshoot.
        for c in ["H", "Ŋ", "Ω", "Ш", "Ꙗ"] {
            assert_eq!(rise(c), OVERSHOOT, "{c}");
        }
        // A mark gets an accent's room, and two or more what the tallest stack takes.
        assert_eq!(rise("A\u{0323}"), MARK_RISE);
        assert_eq!(rise("A\u{0308}\u{0304}"), STACKED_MARKS_RISE);
        assert!(rise("A\u{0308}\u{0304}") >= rise("\u{01DE}"));
        assert!(rise("A\u{030A}\u{0301}") >= rise("\u{01FA}"));
        assert_eq!(rise("A\u{0306}\u{0302}\u{0301}"), STACKED_MARKS_RISE);
        assert_eq!(rise("天\u{0301}"), OTHER_SCRIPT_RISE.max(MARK_RISE));
        // No letter of the table rises past the room stacked marks get.
        assert!(RISE.iter().all(|&r| f32::from(r) / 100.0 <= STACKED_MARKS_RISE));
    }

    #[test]
    fn marks_of_every_script() {
        for m in [
            '\u{0300}',
            '\u{036F}',
            '\u{0483}',
            '\u{093F}',
            '\u{17B6}',
            '\u{17D2}',
            '\u{20DD}',
            '\u{FE20}',
            '\u{1D167}',
            '\u{E0100}',
        ] {
            assert!(is_mark(m), "U+{:04X}", m as u32);
        }
        for c in ['\0', 'A', '\u{02FF}', '\u{0370}', 'ក', '天', '\u{10FFFF}'] {
            assert!(!is_mark(c), "U+{:04X}", c as u32);
        }
        assert!(MARKS.chunks(2).all(|r| r[0] <= r[1]) && MARKS.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn other_scripts_keep_their_marks_and_get_an_em() {
        // ក with its vowel sign ោ, then ម: the initial is the letter and its sign.
        let (initial, rest) = split_initial(&Lyric::from_plain("\u{1780}\u{17C4}\u{1798}")).unwrap();
        assert_eq!(initial, "\u{1780}\u{17C4}");
        assert_eq!(rest.plain(), "\u{1798}");
        assert_eq!(advance("天", 0.5), OTHER_SCRIPT_ADVANCE);
        assert_eq!(advance("ស", 0.5), OTHER_SCRIPT_ADVANCE);
        assert_eq!(advance("天", 1.2), 1.2);
        assert_eq!(advance("A", 0.5), 0.5);
        assert_eq!(advance("Ж", 0.7), 0.7);
    }

    #[test]
    fn annotation_lines() {
        let h = parse("annotation: Ant.;\nannotation: VIII G;\nmode: 8;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["Ant.", "VIII G"]);
        let h = parse("mode: 8;\nmode-differentia: G;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["viii G"]);
        let h = parse("mode: 2;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["ii"]);
        let h = parse("annotation: {\\sc Ps.};\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["Ps."]);
        let h = parse("mode: per.;\n%%\n(c4)").score.header;
        assert_eq!(annotations(&h), ["per."]);
        assert!(annotations(&parse("(c4)").score.header).is_empty());
    }
}
