//! Scanline work: undo the per-row filters, then turn packed samples of any
//! depth into RGBA8 pixels.

use super::{Header, Trns};
use crate::ImageError;

/// Undo filters on `rows` scanlines of `row_bytes` (+1 filter byte each).
/// Returns the plain scanlines packed back to back.
pub(crate) fn unfilter(
    raw: &[u8],
    row_bytes: usize,
    bpp: usize,
    rows: usize,
) -> Result<Vec<u8>, ImageError> {
    let mut out = vec![0u8; row_bytes * rows];
    let mut cur_start = 0usize;
    for (y, src) in raw.chunks_exact(row_bytes + 1).take(rows).enumerate() {
        let (filter, src) = (src[0], &src[1..]);
        let (done, rest) = out.split_at_mut(cur_start);
        let cur = &mut rest[..row_bytes];
        let prev: &[u8] = if y > 0 { &done[cur_start - row_bytes..] } else { &[] };
        match filter {
            0 => cur.copy_from_slice(src),
            1 => {
                cur[..bpp.min(row_bytes)].copy_from_slice(&src[..bpp.min(row_bytes)]);
                for x in bpp..row_bytes {
                    cur[x] = src[x].wrapping_add(cur[x - bpp]);
                }
            }
            2 => {
                for x in 0..row_bytes {
                    cur[x] = src[x].wrapping_add(prev.get(x).copied().unwrap_or(0));
                }
            }
            3 => {
                for x in 0..row_bytes {
                    let left = if x >= bpp { cur[x - bpp] as u16 } else { 0 };
                    let above = prev.get(x).copied().unwrap_or(0) as u16;
                    cur[x] = src[x].wrapping_add(((left + above) / 2) as u8);
                }
            }
            4 => {
                for x in 0..row_bytes {
                    let left = if x >= bpp { cur[x - bpp] } else { 0 };
                    let above = prev.get(x).copied().unwrap_or(0);
                    let upleft = if x >= bpp { prev.get(x - bpp).copied().unwrap_or(0) } else { 0 };
                    cur[x] = src[x].wrapping_add(paeth(left, above, upleft));
                }
            }
            f => return Err(ImageError::Corrupt(format!("unknown filter type {f}"))),
        }
        cur_start += row_bytes;
    }
    Ok(out)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (a as i16, b as i16, c as i16);
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Turns unfiltered scanlines into RGBA8 pixels. Holds a reusable sample
/// buffer so a row is unpacked once whatever its depth.
pub(crate) struct Expander<'a> {
    depth: u8,
    color_type: u8,
    channels: usize,
    palette: &'a [[u8; 3]],
    trns: &'a Trns,
    samples: Vec<u16>,
}

impl<'a> Expander<'a> {
    pub fn new(h: &Header, palette: &'a [[u8; 3]], trns: &'a Trns) -> Self {
        Self {
            depth: h.depth,
            color_type: h.color_type,
            channels: h.channels(),
            palette,
            trns,
            samples: Vec::new(),
        }
    }

    /// Call `put(index, rgba)` for each of the `width` pixels in `line`.
    pub fn expand_row(
        &mut self,
        line: &[u8],
        width: u32,
        mut put: impl FnMut(usize, [u8; 4]),
    ) -> Result<(), ImageError> {
        let n = width as usize * self.channels;
        self.unpack(line, n);
        let to8 = |v: u16| -> u8 {
            match self.depth {
                1 => (v * 255) as u8,
                2 => (v * 85) as u8,
                4 => (v * 17) as u8,
                8 => v as u8,
                _ => (v >> 8) as u8,
            }
        };
        for (i, px) in self.samples.chunks_exact(self.channels).enumerate() {
            let out = match self.color_type {
                0 => {
                    let a = match self.trns {
                        Trns::Gray(key) if *key == px[0] => 0,
                        _ => 255,
                    };
                    let g = to8(px[0]);
                    [g, g, g, a]
                }
                2 => {
                    let a = match self.trns {
                        Trns::Rgb(key) if *key == [px[0], px[1], px[2]] => 0,
                        _ => 255,
                    };
                    [to8(px[0]), to8(px[1]), to8(px[2]), a]
                }
                3 => {
                    let idx = px[0] as usize;
                    let [r, g, b] = self
                        .palette
                        .get(idx)
                        .copied()
                        .ok_or_else(|| ImageError::Corrupt("palette index out of range".into()))?;
                    let a = match self.trns {
                        Trns::Palette(t) => t.get(idx).copied().unwrap_or(255),
                        _ => 255,
                    };
                    [r, g, b, a]
                }
                4 => {
                    let g = to8(px[0]);
                    [g, g, g, to8(px[1])]
                }
                _ => [to8(px[0]), to8(px[1]), to8(px[2]), to8(px[3])],
            };
            put(i, out);
        }
        Ok(())
    }

    /// Unpack the first `n` samples of `line` into `self.samples`.
    fn unpack(&mut self, line: &[u8], n: usize) {
        self.samples.clear();
        self.samples.reserve(n);
        match self.depth {
            8 => self.samples.extend(line.iter().take(n).map(|&b| b as u16)),
            16 => self
                .samples
                .extend(line.chunks_exact(2).take(n).map(|c| u16::from_be_bytes([c[0], c[1]]))),
            d => {
                let d = d as usize;
                let mask = ((1u16 << d) - 1) as u8;
                let per_byte = 8 / d;
                for i in 0..n {
                    let byte = line[i / per_byte];
                    let shift = 8 - d * (i % per_byte + 1);
                    self.samples.push(((byte >> shift) & mask) as u16);
                }
            }
        }
    }
}
