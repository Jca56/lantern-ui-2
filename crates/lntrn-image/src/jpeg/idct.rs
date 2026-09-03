//! Dequantisation and the 8×8 inverse DCT. The AAN float algorithm as in
//! libjpeg's jidctflt.c, with the quantisation table pre-scaled by the AAN
//! factors so each block costs a handful of multiplies per row.

/// Natural-order index of each zigzag position.
pub(crate) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
    20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58,
    59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

use std::f32::consts::SQRT_2;

const AAN_SCALE: [f32; 8] =
    [1.0, 1.387_039_8, 1.306_563, 1.175_875_6, 1.0, 0.785_694_96, 0.541_196_1, 0.275_899_38];

/// A quantisation table (natural order) folded with the AAN scale factors.
pub(crate) fn scaled_quant(q: &[u16; 64]) -> [f32; 64] {
    let mut out = [0f32; 64];
    for (i, o) in out.iter_mut().enumerate() {
        *o = q[i] as f32 * AAN_SCALE[i / 8] * AAN_SCALE[i % 8] / 8.0;
    }
    out
}

/// Dequantise `coef` (natural order) and write 8×8 samples into `out`
/// rows of `stride` bytes.
pub(crate) fn idct_block(coef: &[i16; 64], q: &[f32; 64], out: &mut [u8], stride: usize) {
    let mut ws = [0f32; 64];
    // Pass 1: columns.
    for c in 0..8 {
        let col = |r: usize| coef[r * 8 + c] as f32 * q[r * 8 + c];
        if (1..8).all(|r| coef[r * 8 + c] == 0) {
            let dc = col(0);
            for r in 0..8 {
                ws[r * 8 + c] = dc;
            }
            continue;
        }
        let v = [col(0), col(1), col(2), col(3), col(4), col(5), col(6), col(7)];
        let o = aan_1d(v);
        for r in 0..8 {
            ws[r * 8 + c] = o[r];
        }
    }
    // Pass 2: rows, then level shift and clamp.
    for r in 0..8 {
        let v: [f32; 8] = ws[r * 8..r * 8 + 8].try_into().expect("8 wide");
        let o = aan_1d(v);
        for (c, s) in o.iter().enumerate() {
            out[r * stride + c] = (s + 128.5).clamp(0.0, 255.0) as u8;
        }
    }
}

/// One AAN butterfly over eight pre-scaled coefficients.
#[inline]
fn aan_1d(v: [f32; 8]) -> [f32; 8] {
    // Even part.
    let tmp10 = v[0] + v[4];
    let tmp11 = v[0] - v[4];
    let tmp13 = v[2] + v[6];
    let tmp12 = (v[2] - v[6]) * SQRT_2 - tmp13;
    let e0 = tmp10 + tmp13;
    let e3 = tmp10 - tmp13;
    let e1 = tmp11 + tmp12;
    let e2 = tmp11 - tmp12;
    // Odd part.
    let z13 = v[5] + v[3];
    let z10 = v[5] - v[3];
    let z11 = v[1] + v[7];
    let z12 = v[1] - v[7];
    let o7 = z11 + z13;
    let t11 = (z11 - z13) * SQRT_2;
    let z5 = (z10 + z12) * 1.847_759;
    let t10 = 1.082_392_2 * z12 - z5;
    let t12 = -2.613_126 * z10 + z5;
    let o6 = t12 - o7;
    let o5 = t11 - o6;
    let o4 = t10 + o5;
    [e0 + o7, e1 + o6, e2 + o5, e3 - o4, e3 + o4, e2 - o5, e1 - o6, e0 - o7]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Textbook separable IDCT, the reference the fast path must match.
    fn slow_idct(coef: &[i16; 64], q: &[u16; 64]) -> [f32; 64] {
        let mut out = [0f32; 64];
        let c = |u: usize| if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
        for y in 0..8 {
            for x in 0..8 {
                let mut s = 0f32;
                for u in 0..8 {
                    for v in 0..8 {
                        let f = coef[v * 8 + u] as f32 * q[v * 8 + u] as f32;
                        s += c(u)
                            * c(v)
                            * f
                            * (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos()
                            * (((2 * y + 1) as f32 * v as f32 * std::f32::consts::PI) / 16.0).cos();
                    }
                }
                out[y * 8 + x] = s / 4.0;
            }
        }
        out
    }

    #[test]
    fn aan_matches_the_slow_transform() {
        let mut seed = 0x1234_5678u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..200 {
            let mut coef = [0i16; 64];
            let mut q = [1u16; 64];
            for i in 0..64 {
                coef[i] = (rnd() % 200) as i16 - 100;
                q[i] = 1 + (rnd() % 20) as u16;
            }
            let sq = scaled_quant(&q);
            let mut out = [0u8; 64];
            idct_block(&coef, &sq, &mut out, 8);
            let slow = slow_idct(&coef, &q);
            for i in 0..64 {
                let want = (slow[i] + 128.5).clamp(0.0, 255.0) as u8;
                assert!((want as i32 - out[i] as i32).abs() <= 1, "sample {i}: {want} vs {}", out[i]);
            }
        }
    }
}
