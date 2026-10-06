//! Fast fixed-point number formatting for the SVG and JSON writers.
//!
//! `format!("{:.2}", v)` goes through the exact float-to-decimal machinery, which dominated
//! the cost of writing a long score's SVG (and is slower still in WebAssembly). Coordinates
//! need only two or three decimals, so this rounds with integer arithmetic and falls back to
//! the standard formatter only for values it can't round exactly: halfway cases, huge
//! values and non-finite ones. The output is byte-for-byte what `format!` writes.

use std::fmt::Write as _;

const POW10: [f64; 4] = [1.0, 10.0, 100.0, 1000.0];

/// Appends `v` with exactly `decimals` (0 to 3) digits after the point, as
/// `format!("{:.decimals$}", v)` would, including a `-` on negative values that round to zero.
pub fn push_fixed(out: &mut String, v: f32, decimals: usize) {
    let decimals = decimals.min(3);
    let x = v as f64 * POW10[decimals];
    let frac = x.abs().fract();
    // An f32 has a 24-bit mantissa and 1000 needs 10 bits, so the f64 product is exact and
    // rounding it is exact rounding, except at a half, where `format!` rounds to even.
    if !x.is_finite() || x.abs() >= 1.0e12 || (frac - 0.5).abs() < 1.0e-6 {
        let _ = write!(out, "{:.*}", decimals, v);
        return;
    }
    let r = x.abs().round() as u64;
    if v.is_sign_negative() {
        out.push('-');
    }
    let scale = POW10[decimals] as u64;
    push_u64(out, r / scale);
    if decimals > 0 {
        out.push('.');
        let mut digits = [b'0'; 3];
        let mut f = r % scale;
        for d in digits[..decimals].iter_mut().rev() {
            *d = b'0' + (f % 10) as u8;
            f /= 10;
        }
        for &d in &digits[..decimals] {
            out.push(d as char);
        }
    }
}

/// Appends an unsigned integer in decimal.
pub fn push_u64(out: &mut String, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for &d in &buf[i..] {
        out.push(d as char);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(v: f32, d: usize) -> String {
        let mut s = String::new();
        push_fixed(&mut s, v, d);
        s
    }

    #[test]
    fn matches_the_standard_formatter() {
        let mut edge = vec![
            0.0f32,
            -0.0,
            0.005,
            -0.005,
            0.015,
            0.125,
            -0.125,
            0.0005,
            1.0005,
            2.675,
            1e-9,
            -1e-9,
            1e9,
            -1e9,
            1e13,
            3e38,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.995,
            9.995,
            99.9995,
        ];
        // Every hundredth and thousandth near the values layouts use, and a sweep of bit
        // patterns across the whole range.
        for i in -200_000..200_000 {
            edge.push(i as f32 / 1000.0);
            edge.push(i as f32 / 1000.0 + 0.0005);
        }
        let mut bits = 0x0000_0001u32;
        while bits < 0x7f80_0000 {
            edge.push(f32::from_bits(bits));
            edge.push(-f32::from_bits(bits));
            bits = bits.wrapping_add(0x0001_3579);
        }
        for v in edge {
            for d in 0..=3 {
                assert_eq!(fixed(v, d), format!("{:.*}", d, v), "{v:e} with {d} decimals");
            }
        }
    }

    #[test]
    fn integers() {
        for v in [0u64, 7, 10, 1234567890, u64::MAX] {
            let mut s = String::new();
            push_u64(&mut s, v);
            assert_eq!(s, v.to_string());
        }
    }
}
