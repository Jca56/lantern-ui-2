//! From component planes to RGBA: chroma upsampling (libjpeg's "fancy"
//! triangle filters for the 2×1, 1×2 and 2×2 cases, replication otherwise)
//! and the fixed-point YCbCr→RGB conversion libjpeg uses, so our output
//! lands within a hair of the reference decoder.

/// Which conversion the frame's components need.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Colour {
    Gray,
    YCbCr,
    Rgb,
}

/// Bring a `dw`×`dh` component plane (rows `stride` apart) up to
/// `out_w`×`out_h` by integer factors `hr`×`vr`.
pub(crate) fn upsample(
    plane: &[u8],
    stride: usize,
    (dw, dh): (usize, usize),
    (hr, vr): (usize, usize),
    (out_w, out_h): (usize, usize),
) -> Vec<u8> {
    let row = |y: usize| &plane[y.min(dh - 1) * stride..][..dw];
    let mut out = vec![0u8; out_w * out_h];
    let fancy_h = dw > 2;
    for oy in 0..out_h {
        let dst = &mut out[oy * out_w..(oy + 1) * out_w];
        let iy = (oy / vr).min(dh - 1);
        match (hr, vr) {
            (1, 1) => dst.copy_from_slice(&row(iy)[..out_w]),
            (2, 1) if fancy_h => {
                let s: Vec<u32> = row(iy).iter().map(|&v| v as u32).collect();
                h2_fancy(&s, 1, 2, 2, dst);
            }
            (1, 2) => {
                let (nb, bias) = neighbour(oy, iy, dh);
                let s = sums(row(iy), row(nb));
                for (d, v) in dst.iter_mut().zip(&s) {
                    *d = ((v + bias) >> 2) as u8;
                }
            }
            (2, 2) if fancy_h => {
                let (nb, _) = neighbour(oy, iy, dh);
                h2_fancy(&sums(row(iy), row(nb)), 8, 7, 4, dst);
            }
            _ => {
                let src = row(iy);
                for (x, d) in dst.iter_mut().enumerate() {
                    *d = src[(x / hr).min(dw - 1)];
                }
            }
        }
    }
    out
}

/// Vertical neighbour row for the upper (even) or lower (odd) output row of
/// a pair, clamped at the picture edges, plus the rounding bias it takes.
fn neighbour(oy: usize, iy: usize, dh: usize) -> (usize, u32) {
    if oy.is_multiple_of(2) { (iy.saturating_sub(1), 1) } else { ((iy + 1).min(dh - 1), 2) }
}

/// `3*a + b` per column: the vertical half of the 2×2 / 1×2 filter.
fn sums(a: &[u8], b: &[u8]) -> Vec<u32> {
    a.iter().zip(b).map(|(&a, &b)| a as u32 * 3 + b as u32).collect()
}

/// Horizontal 3/4–1/4 filter over per-column values. `bias_l`/`bias_r` are
/// the rounding constants and `shift` the final scale: (1, 2, 2) over plain
/// samples (2×1), (8, 7, 4) over `3a+b` sums (2×2). Edge columns come out
/// as plain replication, exactly as libjpeg's special cases do.
fn h2_fancy(s: &[u32], bias_l: u32, bias_r: u32, shift: u32, dst: &mut [u8]) {
    let n = s.len();
    let mut put = |i: usize, v: u32| {
        if let Some(d) = dst.get_mut(i) {
            *d = v as u8;
        }
    };
    for x in 0..n {
        let c = s[x] * 3;
        let l = if x == 0 { s[0] } else { s[x - 1] };
        let r = if x + 1 == n { s[x] } else { s[x + 1] };
        put(2 * x, (c + l + bias_l) >> shift);
        put(2 * x + 1, (c + r + bias_r) >> shift);
    }
}

/// Interleave full-size planes into RGBA8.
pub(crate) fn to_rgba(planes: &[Vec<u8>], colour: Colour, pixels: usize) -> Vec<u8> {
    let mut out = vec![255u8; pixels * 4];
    match colour {
        Colour::Gray => {
            for (o, &g) in out.chunks_exact_mut(4).zip(&planes[0]) {
                o[..3].fill(g);
            }
        }
        Colour::Rgb => {
            for (i, o) in out.chunks_exact_mut(4).enumerate() {
                o[0] = planes[0][i];
                o[1] = planes[1][i];
                o[2] = planes[2][i];
            }
        }
        Colour::YCbCr => {
            for (i, o) in out.chunks_exact_mut(4).enumerate() {
                let [r, g, b] = ycc_to_rgb(planes[0][i], planes[1][i], planes[2][i]);
                o[0] = r;
                o[1] = g;
                o[2] = b;
            }
        }
    }
    out
}

const SCALE: i32 = 16;
const ONE_HALF: i32 = 1 << (SCALE - 1);

const fn fix(x: f64) -> i32 {
    (x * (1u32 << SCALE) as f64 + 0.5) as i32
}

/// libjpeg's jdcolor.c arithmetic, bit for bit.
fn ycc_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    let (y, cb, cr) = (y as i32, cb as i32 - 128, cr as i32 - 128);
    let r = y + ((fix(1.40200) * cr + ONE_HALF) >> SCALE);
    let g = y + ((-fix(0.34414) * cb - fix(0.71414) * cr + ONE_HALF) >> SCALE);
    let b = y + ((fix(1.77200) * cb + ONE_HALF) >> SCALE);
    [r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ycc_matches_libjpeg_at_known_points() {
        assert_eq!(ycc_to_rgb(128, 128, 128), [128, 128, 128]);
        assert_eq!(ycc_to_rgb(255, 128, 128), [255, 255, 255]);
        assert_eq!(ycc_to_rgb(0, 128, 128), [0, 0, 0]);
        // Pure red: Y=76, Cb=85, Cr=255.
        assert_eq!(ycc_to_rgb(76, 85, 255), [254, 0, 0]);
    }

    #[test]
    fn h2v1_fancy_edges_replicate() {
        let plane = [10u8, 20, 30, 40];
        let out = upsample(&plane, 4, (4, 1), (2, 1), (8, 1));
        assert_eq!(out, [10, 13, 17, 23, 27, 33, 37, 40]);
    }

    #[test]
    fn narrow_planes_replicate() {
        let plane = [10u8, 20];
        let out = upsample(&plane, 2, (2, 1), (2, 2), (4, 2));
        assert_eq!(out, [10, 10, 20, 20, 10, 10, 20, 20]);
    }
}
