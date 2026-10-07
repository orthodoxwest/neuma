//! The C ABI the JS glue calls. Strings cross as UTF-8 in two buffers the module owns: the
//! glue asks for an input buffer of a given size, writes into it, then calls a function that
//! reads it; results are left in the output buffer for the glue to copy out.

use std::cell::RefCell;

use neuma::{LastLine, Layout, LayoutOptions, SvgOptions, Weights};

use crate::{Chant, ChantOptions, Initial, LyricFont, SvgOutput};

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static CHANTS: RefCell<Vec<Option<Chant>>> = const { RefCell::new(Vec::new()) };
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

fn with_chant<R>(handle: u32, f: impl FnOnce(&mut Chant) -> R) -> Option<R> {
    CHANTS.with(|c| c.borrow_mut().get_mut(handle as usize).and_then(Option::as_mut).map(f))
}

fn chant_options(initial: i32, annotation: u32, lyric_size: f32, font: u32) -> ChantOptions {
    ChantOptions::default()
        .with_initial(Initial::from_staves(initial.into()))
        .with_annotation(annotation != 0)
        .with_lyric_size(lyric_size)
        .with_font(if font == 1 { LyricFont::Garamond12 } else { LyricFont::Google })
}

/// The layout a page was made with: `last` 1 for a justified last line, `max_lines` 0 for
/// all.
fn layout_options(scale: f32, last: u32, max_lines: u32) -> LayoutOptions {
    LayoutOptions::default()
        .with_scale(scale)
        .with_last_line(if last == 1 { LastLine::Justified } else { LastLine::Ragged })
        .with_max_lines(max_lines as usize)
}

fn keep(chant: Chant) -> u32 {
    CHANTS.with(|c| {
        let mut c = c.borrow_mut();
        match c.iter().position(Option::is_none) {
            Some(i) => {
                c[i] = Some(chant);
                i as u32
            }
            None => {
                c.push(Some(chant));
                (c.len() - 1) as u32
            }
        }
    })
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
pub extern "C" fn chant_new(initial: i32, annotation: u32, lyric_size: f32, font: u32) -> u32 {
    let chant = Chant::new(&input(), chant_options(initial, annotation, lyric_size, font));
    output(chant.diagnostics_json());
    keep(chant)
}

/// Sets psalm text to a tone (see [`tone_and_text`] and [`neuma_psalm`]) and engraves it.
/// Returns a handle and leaves the diagnostics JSON, a NUL and the setting's `{ gabc, notes }`
/// in the output buffer; or returns `u32::MAX` and leaves `{"error": …}` there.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_from_psalm(
    custom: u32,
    intone: u32,
    auto_point: u32,
    initial: i32,
    annotation: u32,
    lyric_size: f32,
    font: u32,
) -> u32 {
    match tone_and_text(custom) {
        Ok((tone, text)) => {
            let opts = chant_options(initial, annotation, lyric_size, font);
            let chant = Chant::from_psalm(&text, tone, psalm_options(intone, auto_point), opts);
            diagnostics_and_psalm(&chant);
            keep(chant)
        }
        Err(e) => {
            let mut out = String::new();
            error(&mut out, &e);
            output(&out);
            u32::MAX
        }
    }
}

/// What [`chant_update`] and [`chant_set_options`] return: 0 for an unknown handle, 1 when
/// nothing changed, 2 when the chant changed and its diagnostics (and psalm) are in the
/// output buffer.
fn changed(handle: u32, f: impl FnOnce(&mut Chant) -> bool) -> u32 {
    with_chant(handle, |c| {
        if f(c) {
            diagnostics_and_psalm(c);
            2
        } else {
            1
        }
    })
    .unwrap_or(0)
}

/// Engraves the chant again with new options; returns as [`changed`] says.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_set_options(handle: u32, initial: i32, annotation: u32, lyric_size: f32, font: u32) -> u32 {
    changed(handle, |c| c.set_options(chant_options(initial, annotation, lyric_size, font)))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_free(handle: u32) {
    CHANTS.with(|c| {
        if let Some(slot) = c.borrow_mut().get_mut(handle as usize) {
            *slot = None;
        }
    });
}

/// Lays out at `width` with the SVG class prefix in the input buffer. `flags`: 2 makes the
/// SVG in parts (see `SvgOutput`), 4 leaves out `data-note` and `data-syllable`, 8 (with 2)
/// gives a line the last layout in parts also had by its index there
/// (`SvgOutput::ChangedLines`). Leaves `{ width, height, version }`, a NUL and the SVG in the
/// output buffer; returns 0 for an unknown handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_layout(handle: u32, width: f32, scale: f32, last: u32, max_lines: u32, flags: u32) -> u32 {
    let prefix = input();
    let opts = layout_options(scale, last, max_lines);
    let mut svg = SvgOptions::default().with_ids(flags & 4 == 0);
    if !prefix.is_empty() {
        svg = svg.with_prefix(prefix);
    }
    let mode = match flags & 10 {
        10 => SvgOutput::ChangedLines,
        2 => SvgOutput::Lines,
        _ => SvgOutput::Whole,
    };
    with_chant(handle, |c| output(&c.layout(width, &opts, &svg, mode))).map_or(0, |_| 1)
}

/// Runs `f` on the page laid out at `version` with `width` and the options (as
/// [`chant_layout`] takes them), leaving what it gives in the output buffer. Returns 1, or 0
/// for an unknown handle or a stale page.
fn on_page(handle: u32, version: u32, width: f32, opts: &LayoutOptions, f: impl FnOnce(&Layout) -> String) -> u32 {
    with_chant(handle, |c| c.page(version, width, opts)).flatten().map_or(0, |layout| {
        output(&f(&layout));
        1
    })
}

/// Whether the page laid out at `version` with `width` and the options can still answer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_has_page(handle: u32, version: u32, width: f32, scale: f32, last: u32, max_lines: u32) -> u32 {
    let opts = layout_options(scale, last, max_lines);
    with_chant(handle, |c| c.page(version, width, &opts)).flatten().map_or(0, |_| 1)
}

/// Leaves the timeline JSON of a page (see [`on_page`]), timed with the weights as ten numbers
/// (NaN keeps a default), in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_timeline(
    handle: u32,
    version: u32,
    width: f32,
    scale: f32,
    last: u32,
    max_lines: u32,
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
    let opts = layout_options(scale, last, max_lines);
    on_page(handle, version, width, &opts, |l| crate::timeline_json(l, &weights))
}

/// Leaves the score's library entry JSON in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_summary(handle: u32) -> u32 {
    with_chant(handle, |c| output(c.summary_json())).map_or(0, |_| 1)
}

/// Replaces the score with the source in the input buffer (GABC, or psalm text for a chant
/// set from a psalm), keeping the options; returns as [`changed`] says, the output buffer
/// then holding the diagnostics JSON (and for a psalm a NUL and its setting).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_update(handle: u32) -> u32 {
    let src = input();
    changed(handle, |c| c.update(&src))
}

/// The note at (`x`, `y`) of a page (see [`on_page`]), -1 for none, or -2 for an unknown
/// handle or a stale page.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_note_at(handle: u32, version: u32, width: f32, scale: f32, last: u32, max_lines: u32, x: f32, y: f32) -> i32 {
    let opts = layout_options(scale, last, max_lines);
    match with_chant(handle, |c| c.page(version, width, &opts)).flatten() {
        Some(layout) => layout.note_at(x, y).map_or(-1, |n| n as i32),
        None => -2,
    }
}

/// Leaves the element under (`x`, `y`) of a page as JSON (or `null`) in the output buffer;
/// returns as [`on_page`] says.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_source_at(handle: u32, version: u32, width: f32, scale: f32, last: u32, max_lines: u32, x: f32, y: f32) -> u32 {
    let opts = layout_options(scale, last, max_lines);
    on_page(handle, version, width, &opts, |l| crate::source_at_json(l, x, y))
}

/// Leaves what to highlight for a caret at `offset` (UTF-16 units if `utf16` is 1, else UTF-8
/// bytes) of a page as a JSON array in the output buffer; returns as [`on_page`] says.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_elements_at(
    handle: u32,
    version: u32,
    width: f32,
    scale: f32,
    last: u32,
    max_lines: u32,
    offset: u32,
    utf16: u32,
) -> u32 {
    let opts = layout_options(scale, last, max_lines);
    on_page(handle, version, width, &opts, |l| {
        crate::elements_at_json(l, offset as usize, utf16 == 1)
    })
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

fn error(out: &mut String, e: &str) {
    out.push_str("{\"error\":");
    neuma::json::string(out, e);
    out.push('}');
}

/// Sets psalm text to a tone (see [`tone_and_text`]); `intone` is 0 for the first verse, 1
/// for every verse, 2 for none, and `auto_point` 0 leaves unpointed halves unpointed. Leaves
/// the setting JSON, or `{"error": …}`, in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_psalm(custom: u32, intone: u32, auto_point: u32) {
    let mut out = String::new();
    match tone_and_text(custom) {
        Ok((tone, text)) => {
            let options = psalm_options(intone, auto_point);
            let index = neuma::Utf16Index::new(&text);
            crate::setting_json(&mut out, &neuma_tones::psalm(&text, &tone, &options), &index);
        }
        Err(e) => error(&mut out, &e),
    }
    output(&out);
}

fn psalm_options(intone: u32, auto_point: u32) -> neuma_tones::PsalmOptions {
    neuma_tones::PsalmOptions::default()
        .with_intone(match intone {
            1 => neuma_tones::Intone::EveryVerse,
            2 => neuma_tones::Intone::Never,
            _ => neuma_tones::Intone::FirstVerse,
        })
        .with_auto_point(auto_point != 0)
}

/// Points psalm text for a tone (see [`tone_and_text`]), leaving the pointing JSON, or
/// `{"error": …}`, in the output buffer.
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

/// Leaves the built-in tone names, one per line, in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn neuma_tone_names() {
    let names: Vec<&str> = neuma_tones::Tone::builtin().iter().map(|t| t.name.as_str()).collect();
    output(&names.join("\n"));
}
