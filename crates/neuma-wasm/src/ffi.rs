//! The C ABI the JS glue calls. Strings cross as UTF-8 in two buffers the module owns: the
//! glue asks for an input buffer of a given size, writes into it, then calls a function that
//! reads it; results are left in the output buffer for the glue to copy out.

use std::cell::RefCell;

use neuma::{LayoutOptions, SvgOptions};

use crate::{Chant, ChantOptions, Font, last_line, weights_from};

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
pub extern "C" fn chant_new(initial: u32, annotation: u32, lyric_size: f32, font: u32) -> u32 {
    let opts = ChantOptions {
        initial: initial.min(u8::MAX as u32) as u8,
        annotation: annotation != 0,
        lyric_size,
        font: if font == 1 { Font::Garamond12 } else { Font::Google },
    };
    let chant = Chant::new(&input(), opts);
    output(chant.diagnostics_json());
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

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_free(handle: u32) {
    CHANTS.with(|c| {
        if let Some(slot) = c.borrow_mut().get_mut(handle as usize) {
            *slot = None;
        }
    });
}

/// Lays out at `width`, with the weights as ten numbers (NaN keeps a default) and the SVG
/// class prefix in the input buffer. Leaves the layout JSON in the output buffer; returns 0
/// for an unknown handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn chant_layout(
    handle: u32,
    width: f32,
    scale: f32,
    last: u32,
    note: f32,
    mora: f32,
    episema: f32,
    virgula: f32,
    minima: f32,
    minor: f32,
    maior: f32,
    finalis: f32,
    mediant: f32,
    flex: f32,
) -> u32 {
    let prefix = input();
    let weights = weights_from(&[note, mora, episema, virgula, minima, minor, maior, finalis, mediant, flex]);
    let opts = LayoutOptions {
        scale,
        last_line: last_line(last),
    };
    let svg = SvgOptions {
        prefix: if prefix.is_empty() { SvgOptions::default().prefix } else { prefix },
        ..SvgOptions::default()
    };
    with_chant(handle, |c| {
        c.layout(width, &opts, &weights, &svg);
        output(c.layout_json());
    })
    .map_or(0, |_| 1)
}

/// Leaves the last layout's SVG in the output buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_svg(handle: u32) -> u32 {
    with_chant(handle, |c| output(c.svg())).map_or(0, |_| 1)
}

/// The note at (`x`, `y`) in the last layout, or -1.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn chant_note_at(handle: u32, x: f32, y: f32) -> i32 {
    with_chant(handle, |c| c.note_at(x, y)).flatten().map_or(-1, |n| n as i32)
}
