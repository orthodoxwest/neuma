//! The C ABI the JS glue calls. Strings cross as UTF-8 in two buffers the module owns: the
//! glue asks for an input buffer of a given size, writes into it, then calls a function that
//! reads it; results are left in the output buffer for the glue to copy out.

use std::cell::{Cell, RefCell};

use neuma::{LastLine, LayoutOptions, SvgOptions, Weights};

use crate::slab::{NONE, Slab};
use crate::{Chant, ChantOptions, Initial, Page, SvgOutput};

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static CHANTS: RefCell<Slab<Chant>> = const { RefCell::new(Slab::new()) };
    static PAGES: RefCell<Slab<Held>> = const { RefCell::new(Slab::new()) };
    /// How many pages' layouts the engine keeps, current and stale; see
    /// [`neuma_set_layout_budget`].
    static BUDGET: Cell<[usize; 2]> = const { Cell::new(DEFAULT_BUDGET) };
}

/// Layouts kept by default: of chants as they are now, enough for a page of many chants and
/// their thumbnails, all hovered and clicked; of chants that have changed since or are gone,
/// a few, for a click between an edit and the next frame.
const DEFAULT_BUDGET: [usize; 2] = [64, 2];

/// A page's layout, and the chant and version it shows.
struct Held {
    page: Page,
    chant: f64,
    version: u64,
}

fn input() -> String {
    INPUT.with(|b| String::from_utf8_lossy(&b.borrow()).into_owned())
}

fn output(s: &str) {
    OUTPUT.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.extend_from_slice(s.as_bytes());
    });
}

fn with_chant<R>(handle: f64, f: impl FnOnce(&mut Chant) -> R) -> Option<R> {
    CHANTS.with(|c| c.borrow_mut().get_mut(handle).map(f))
}

fn chant_options(initial: i32, annotation: u32, lyric_size: f32, font: u32) -> ChantOptions {
    let options = ChantOptions::default()
        .with_initial(Initial::from_staves(initial.into()))
        .with_annotation(annotation != 0)
        .with_lyric_size(lyric_size);
    #[cfg(any(feature = "font-google", feature = "font-garamond12"))]
    let options = options.with_font(if font == 1 {
        crate::LyricFont::Garamond12
    } else {
        crate::LyricFont::Google
    });
    #[cfg(not(any(feature = "font-google", feature = "font-garamond12")))]
    let _ = font;
    options
}

/// The layout a page was made with: `last` 1 for a justified last line, `max_lines` 0 for
/// all.
fn layout_options(scale: f32, last: u32, max_lines: u32) -> LayoutOptions {
    LayoutOptions::default()
        .with_scale(scale)
        .with_last_line(if last == 1 { LastLine::Justified } else { LastLine::Ragged })
        .with_max_lines(max_lines as usize)
}

fn keep(chant: Chant) -> f64 {
    CHANTS.with(|c| c.borrow_mut().put(chant))
}

fn with_page<R>(handle: f64, f: impl FnOnce(&Page) -> R) -> Option<R> {
    PAGES.with(|p| p.borrow_mut().get_mut(handle).map(|held| f(&held.page)))
}

/// Keeps `page`, laid out from `chant` (or [`NONE`]) at `version`, evicting layouts beyond
/// the budget, and returns its handle.
fn keep_page(page: Page, chant: f64, version: u64) -> f64 {
    let handle = PAGES.with(|p| p.borrow_mut().put(Held { page, chant, version }));
    evict();
    handle
}

/// Drops the least recently used layouts beyond the budget: of those showing their chant as
/// it is now, and of those whose chant has changed since or is gone.
fn evict() {
    let [current, stale] = BUDGET.with(Cell::get);
    CHANTS.with(|c| {
        let chants = c.borrow();
        PAGES.with(|p| {
            let mut pages = p.borrow_mut();
            pages.evict_to(stale, |h| is_stale(&chants, h));
            pages.evict_to(current, |h| !is_stale(&chants, h));
        });
    });
}

/// Whether `held` shows its chant as it was, not as it is now, or a chant now gone.
fn is_stale(chants: &Slab<Chant>, held: &Held) -> bool {
    chants.peek(held.chant).is_none_or(|c| c.version() != held.version)
}

/// The diagnostics JSON, then for a chant set from a psalm a NUL and its `{ gabc, notes }`.
fn diagnostics_and_psalm(c: &Chant) {
    match c.psalm_json() {
        Some(p) => output(&format!("{}\0{p}", c.diagnostics_json())),
        None => output(c.diagnostics_json()),
    }
}

/// Makes the input buffer `len` bytes long and returns where to write.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_input(len: u32) -> *mut u8 {
    INPUT.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.resize(len as usize, 0);
        b.as_mut_ptr()
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_output_ptr() -> *const u8 {
    OUTPUT.with(|b| b.borrow().as_ptr())
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_output_len() -> u32 {
    OUTPUT.with(|b| b.borrow().len() as u32)
}

/// Engraves the GABC in the input buffer. Returns a handle and leaves the diagnostics JSON
/// in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_new(initial: i32, annotation: u32, lyric_size: f32, font: u32) -> f64 {
    let chant = Chant::new(&input(), chant_options(initial, annotation, lyric_size, font));
    output(chant.diagnostics_json());
    keep(chant)
}

/// Sets psalm text to a tone (see [`tone_and_text`] and [`neuma_psalm`]) and engraves it.
/// Returns a handle and leaves the diagnostics JSON, a NUL and the setting's `{ gabc, notes }`
/// in the output buffer; or returns -1 and leaves `{"error": …}` there.
#[cfg(feature = "tones")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_from_psalm(
    custom: u32,
    psalm_flags: u32,
    auto_point: u32,
    initial: i32,
    annotation: u32,
    lyric_size: f32,
    font: u32,
) -> f64 {
    match psalm_input(custom, psalm_flags, auto_point) {
        Ok((tone, text, psalm)) => {
            let opts = chant_options(initial, annotation, lyric_size, font);
            let chant = Chant::from_psalm(&text, &tone, &psalm, opts);
            diagnostics_and_psalm(&chant);
            keep(chant)
        }
        Err(e) => {
            let mut out = String::new();
            error(&mut out, &e);
            output(&out);
            NONE
        }
    }
}

/// What [`chant_update`] and [`chant_set_options`] return: 0 for an unknown handle, 1 when
/// nothing changed, 2 when the chant changed and its diagnostics (and psalm) are in the
/// output buffer.
fn changed(handle: f64, f: impl FnOnce(&mut Chant) -> bool) -> u32 {
    let status = with_chant(handle, |c| {
        if f(c) {
            diagnostics_and_psalm(c);
            2
        } else {
            1
        }
    })
    .unwrap_or(0);
    if status == 2 {
        // The chant's pages are stale now.
        evict();
    }
    status
}

/// Engraves the chant again with new options; returns as [`changed`] says.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_set_options(handle: f64, initial: i32, annotation: u32, lyric_size: f32, font: u32) -> u32 {
    changed(handle, |c| c.set_options(chant_options(initial, annotation, lyric_size, font)))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_free(handle: f64) {
    CHANTS.with(|c| c.borrow_mut().free(handle));
    evict();
}

/// Lays out at `width` with the SVG class prefix in the input buffer. `flags`: 2 makes the
/// SVG in parts (see `SvgOutput`), 4 leaves out `data-note` and `data-syllable`, 8 (with 2)
/// gives a line that `previous` (a page, or -1 for none) also had by its index there
/// (`SvgOutput::ChangedLines`), 16 makes no SVG at all (`SvgOutput::None`, to lay a page out
/// again for its hit tests). Leaves `{ width, height }`, a NUL and the SVG in the output
/// buffer, and returns the new page's handle, or -1 for an unknown chant.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_layout(handle: f64, width: f32, scale: f32, last: u32, max_lines: u32, flags: u32, previous: f64) -> f64 {
    let prefix = input();
    let opts = layout_options(scale, last, max_lines);
    let mut svg = SvgOptions::default().with_ids(flags & 4 == 0);
    if !prefix.is_empty() {
        svg = svg.with_prefix(prefix);
    }
    let mode = match flags & 26 {
        16.. => SvgOutput::None,
        10 => SvgOutput::ChangedLines,
        2 => SvgOutput::Lines,
        _ => SvgOutput::Whole,
    };
    let made = with_chant(handle, |c| {
        PAGES.with(|p| {
            let mut pages = p.borrow_mut();
            let previous = pages.get_mut(previous).map(|held| &held.page);
            (c.layout(width, &opts, &svg, mode, previous), c.version())
        })
    });
    match made {
        Some(((page, out), version)) => {
            output(&out);
            keep_page(page, handle, version)
        }
        None => NONE,
    }
}

/// Lays a page out again from what it was made from, after its layout was evicted or freed
/// and its chant has changed or gone: the source in the input buffer (GABC, or for `psalm` 1
/// the tone, a NUL and the text, as [`tone_and_text`] reads them), the chant's options and
/// the layout's, as [`chant_new`], [`chant_from_psalm`] and [`chant_layout`] take them. What
/// it lays out is the same as the first time, as a fresh chant's layout always is. Returns
/// the page's handle, or -1 for a tone that can't be read.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn page_rebuild(
    psalm: u32,
    custom: u32,
    psalm_flags: u32,
    auto_point: u32,
    initial: i32,
    annotation: u32,
    lyric_size: f32,
    font: u32,
    width: f32,
    scale: f32,
    last: u32,
    max_lines: u32,
) -> f64 {
    let options = chant_options(initial, annotation, lyric_size, font);
    let chant = if psalm == 1 {
        #[cfg(feature = "tones")]
        match psalm_input(custom, psalm_flags, auto_point) {
            Ok((tone, text, psalm)) => Chant::from_psalm(&text, &tone, &psalm, options),
            Err(_) => return NONE,
        }
        #[cfg(not(feature = "tones"))]
        {
            let _ = (custom, psalm_flags, auto_point);
            return NONE;
        }
    } else {
        Chant::new(&input(), options)
    };
    let opts = layout_options(scale, last, max_lines);
    let (page, _) = chant.layout(width, &opts, &SvgOptions::default(), SvgOutput::None, None);
    // No chant shows what it shows any more.
    keep_page(page, NONE, 0)
}

/// Sets how many pages' layouts the engine keeps: `current` of pages that show their chant
/// as it is now, and `stale` of the others, whose chant has changed since or is gone; each
/// at least 1, so a page laid out again lives to answer. Past either, the least recently used is dropped; its page lays itself out again
/// when next asked.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_set_layout_budget(current: u32, stale: u32) {
    BUDGET.with(|b| b.set([(current as usize).max(1), (stale as usize).max(1)]));
    evict();
}

/// How many pages' layouts the engine holds; with `stale` 1, how many of them are stale.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_layouts(stale: u32) -> u32 {
    if stale == 0 {
        return PAGES.with(|p| p.borrow().live() as u32);
    }
    CHANTS.with(|c| PAGES.with(|p| p.borrow().count(|h| is_stale(&c.borrow(), h)) as u32))
}

/// Frees a page.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn page_free(handle: f64) {
    PAGES.with(|p| p.borrow_mut().free(handle));
}

/// Leaves a page's timeline JSON, timed with the weights as ten numbers (NaN keeps a
/// default), in the output buffer; returns 0 for an unknown page.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn page_timeline(
    handle: f64,
    note: f32,
    mora: f32,
    episema: f32,
    virgula: f32,
    quarter: f32,
    half: f32,
    full: f32,
    double: f32,
    mediant: f32,
    flex: f32,
) -> u32 {
    // NaN, or any weight the core can't use, keeps its default.
    let weights = Weights::default()
        .with_note(note)
        .with_mora(mora)
        .with_episema(episema)
        .with_virgula(virgula)
        .with_quarter(quarter)
        .with_half(half)
        .with_full(full)
        .with_double(double)
        .with_mediant(mediant)
        .with_flex(flex);
    with_page(handle, |p| output(&crate::timeline_json(p.layout(), &weights))).map_or(0, |_| 1)
}

/// Leaves the score's library entry JSON in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_summary(handle: f64) -> u32 {
    with_chant(handle, |c| output(c.summary_json())).map_or(0, |_| 1)
}

/// Replaces the score with the source in the input buffer (GABC, or psalm text for a chant
/// set from a psalm), keeping the options; returns as [`changed`] says, the output buffer
/// then holding the diagnostics JSON (and for a psalm a NUL and its setting).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_update(handle: f64) -> u32 {
    let src = input();
    changed(handle, |c| c.update(&src))
}

/// The note at (`x`, `y`) of a page, -1 for none, or -2 for an unknown page.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn page_note_at(handle: f64, x: f32, y: f32) -> i32 {
    with_page(handle, |p| p.layout().note_at(x, y).map_or(-1, |n| n as i32)).unwrap_or(-2)
}

/// Leaves the element under (`x`, `y`) of a page as JSON (or `null`) in the output buffer;
/// returns 0 for an unknown page.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn page_source_at(handle: f64, x: f32, y: f32) -> u32 {
    with_page(handle, |p| output(&crate::source_at_json(p.layout(), x, y))).map_or(0, |_| 1)
}

/// Leaves what to highlight for a caret at `offset` (UTF-16 units if `utf16` is 1, else UTF-8
/// bytes) of a page as a JSON array in the output buffer; returns 0 for an unknown page.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn page_elements_at(handle: f64, offset: u32, utf16: u32) -> u32 {
    with_page(handle, |p| {
        output(&crate::elements_at_json(p.layout(), offset as usize, utf16 == 1))
    })
    .map_or(0, |_| 1)
}

/// Summarizes the GABC in the input buffer without engraving it for display, leaving the
/// library entry JSON in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_summarize() {
    let mut out = String::new();
    neuma::json::summary(&mut out, &neuma::summarize(&input()));
    output(&out);
}

/// The tone and text in the input buffer, separated by a NUL: the tone is a built-in name
/// (`8.G`) when `custom` is 0, else a tone block in the tone file syntax.
#[cfg(feature = "tones")]
fn tone_and_text(custom: u32) -> Result<(neuma_tones::Tone, String), String> {
    let all = input();
    let (tone_src, text) = all.split_once('\0').unwrap_or((all.as_str(), ""));
    let tone = if custom == 0 {
        neuma_tones::Tone::named(tone_src).cloned()
    } else {
        neuma_tones::Tone::parse(tone_src)
    }
    .map_err(|e| e.to_string())?;
    Ok((tone, text.to_string()))
}

#[cfg(feature = "tones")]
fn error(out: &mut String, e: &str) {
    out.push_str("{\"error\":");
    neuma::json::string(out, e);
    out.push('}');
}

/// Sets psalm text to a tone (see [`psalm_input`] for the arguments). Leaves the setting
/// JSON, or `{"error": …}`, in the output buffer.
#[cfg(feature = "tones")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_psalm(custom: u32, psalm_flags: u32, auto_point: u32) {
    let mut out = String::new();
    match psalm_input(custom, psalm_flags, auto_point) {
        Ok((tone, text, options)) => {
            let index = neuma::Utf16Index::new(&text);
            let s = neuma_tones::psalm(&text, &tone, &options);
            crate::setting_json(&mut out, &s.gabc, &s.notes, &s.diagnostics, &index);
        }
        Err(e) => error(&mut out, &e),
    }
    output(&out);
}

/// The tone and text in the input buffer (see [`tone_and_text`]) and the psalm options:
/// `psalm_flags` holds when the intonation is sung in bits 0–1 (0 the first verse, 1 every
/// verse, 2 never) and the accents printed in bits 2–3 (0 all, 1 none, 2 none in a flex);
/// any other value is an error. `auto_point` 0 leaves unpointed halves unpointed.
#[cfg(feature = "tones")]
fn psalm_input(custom: u32, psalm_flags: u32, auto_point: u32) -> Result<(neuma_tones::Tone, String, neuma_tones::PsalmOptions), String> {
    let intone = match psalm_flags & 3 {
        0 => neuma_tones::Intone::FirstVerse,
        1 => neuma_tones::Intone::EveryVerse,
        2 => neuma_tones::Intone::Never,
        _ => return Err(format!("psalm flags {psalm_flags:#x}: no such intonation")),
    };
    let accents = match psalm_flags >> 2 {
        0 => neuma_tones::Accents::All,
        1 => neuma_tones::Accents::None,
        2 => neuma_tones::Accents::OutsideFlex,
        _ => return Err(format!("psalm flags {psalm_flags:#x}: no such accents")),
    };
    let (tone, text) = tone_and_text(custom)?;
    let options = neuma_tones::PsalmOptions::default()
        .with_intone(intone)
        .with_accents(accents)
        .with_auto_point(auto_point != 0);
    Ok((tone, text, options))
}

/// Points psalm text for a tone (see [`tone_and_text`]), leaving the pointing JSON, or
/// `{"error": …}`, in the output buffer.
#[cfg(feature = "pointing")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_point(custom: u32) {
    let mut out = String::new();
    match tone_and_text(custom) {
        Ok((tone, text)) => {
            let index = neuma::Utf16Index::new(&text);
            crate::pointing_json(&mut out, &neuma_tones::point(&text, &tone), &index);
        }
        Err(e) => error(&mut out, &e),
    }
    output(&out);
}

/// Points psalm text for a tone, verse by verse, for display (see [`psalm_input`] for the
/// arguments), leaving the display JSON, or `{"error": …}`, in the output buffer.
#[cfg(feature = "tones")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_psalm_display(custom: u32, psalm_flags: u32, auto_point: u32) {
    let mut out = String::new();
    match psalm_input(custom, psalm_flags, auto_point) {
        Ok((tone, text, options)) => {
            let index = neuma::Utf16Index::new(&text);
            let d = neuma_tones::PsalmDisplay::new(&text, &tone, &options);
            crate::display_json(&mut out, &d, &index);
        }
        Err(e) => error(&mut out, &e),
    }
    output(&out);
}

/// The tone in the input buffer (a name when `custom` is 0, else a tone block) as one line
/// of notes with no words (`Tone::gabc`), or `{"error": …}`.
#[cfg(feature = "tones")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_tone_gabc(custom: u32) {
    match tone_and_text(custom) {
        Ok((tone, _)) => output(&tone.gabc()),
        Err(e) => {
            let mut out = String::new();
            error(&mut out, &e);
            output(&out);
        }
    }
}

/// The tone in the input buffer (as [`neuma_tone_gabc`] reads it) named as a psalter prints
/// it beside the tone (`Tone::label`), or `{"error": …}`.
#[cfg(feature = "tones")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_tone_label(custom: u32) {
    let mut out = String::new();
    match tone_and_text(custom) {
        Ok((tone, _)) => {
            out.push_str("{\"label\":");
            neuma::json::string(&mut out, &tone.label());
            out.push('}');
        }
        Err(e) => error(&mut out, &e),
    }
    output(&out);
}

/// Leaves the built-in tone names, one per line, in the output buffer.
#[cfg(feature = "tones")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_tone_names() {
    let names: Vec<&str> = neuma_tones::Tone::builtin().iter().map(|t| t.name.as_str()).collect();
    output(&names.join("\n"));
}
