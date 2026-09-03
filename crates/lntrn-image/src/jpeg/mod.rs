//! JPEG decoder: baseline and extended-sequential Huffman (SOF0/SOF1) and
//! progressive (SOF2), 8-bit, grayscale / YCbCr / Adobe RGB, any integer
//! sampling factors, restart intervals. Coefficients for every scan land in
//! a per-component block store; the IDCT, upsampling and colour conversion
//! run once at the end. Arithmetic coding, lossless, hierarchical, 12-bit
//! and CMYK files are reported as `Unsupported`.

mod color;
mod huffman;
mod idct;
mod progressive;
mod scan;

use crate::{Image, ImageError};
use color::Colour;
use huffman::{HuffTable, corrupt};

/// Refuse anything bigger than this many pixels (the coefficient store
/// alone would be 400 MB for three components).
const MAX_PIXELS: u64 = 1 << 26;

pub(crate) struct Component {
    pub id: u8,
    /// Sampling factors.
    pub h: usize,
    pub v: usize,
    /// Quantisation table selector, and the table latched at the first scan.
    pub tq: usize,
    pub quant: Option<[u16; 64]>,
    /// Block store dimensions (padded to whole MCUs).
    pub blocks_w: usize,
    pub blocks_h: usize,
    pub coefs: Vec<i16>,
    pub dc_table: usize,
    pub ac_table: usize,
    pub dc_pred: i32,
}

impl Component {
    /// Real (unpadded) sample dimensions of this component.
    pub fn width(&self, f: &Frame) -> usize {
        (f.width * self.h).div_ceil(f.hmax)
    }

    pub fn height(&self, f: &Frame) -> usize {
        (f.height * self.v).div_ceil(f.vmax)
    }
}

pub(crate) struct Frame {
    pub width: usize,
    pub height: usize,
    pub progressive: bool,
    pub components: Vec<Component>,
    pub hmax: usize,
    pub vmax: usize,
    pub mcus_x: usize,
    pub mcus_y: usize,
}

pub(crate) struct Decoder<'a> {
    pub data: &'a [u8],
    pub quant: [Option<[u16; 64]>; 4],
    pub dc_tables: [Option<HuffTable>; 4],
    pub ac_tables: [Option<HuffTable>; 4],
    pub frame: Option<Frame>,
    pub restart_interval: usize,
    pub scans_done: usize,
    jfif: bool,
    adobe_transform: Option<u8>,
}

fn unsupported(msg: &str) -> ImageError {
    ImageError::Unsupported(msg.to_string())
}

pub fn decode(data: &[u8]) -> Result<Image, ImageError> {
    if data.len() < 4 || data[..2] != [0xFF, 0xD8] {
        return Err(corrupt("missing SOI marker"));
    }
    let mut dec = Decoder {
        data,
        quant: [None; 4],
        dc_tables: [None, None, None, None],
        ac_tables: [None, None, None, None],
        frame: None,
        restart_interval: 0,
        scans_done: 0,
        jfif: false,
        adobe_transform: None,
    };
    let mut pos = 2usize;
    loop {
        // Find the next marker: anything before an 0xFF is junk, runs of
        // 0xFF are fill.
        while pos < data.len() && data[pos] != 0xFF {
            pos += 1;
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        let Some(&marker) = data.get(pos) else { break };
        pos += 1;
        match marker {
            0x00 | 0x01 | 0xD0..=0xD8 => continue, // stuffed byte, TEM, stray RST, SOI
            0xD9 => break,                          // EOI
            _ => {}
        }
        let len = u16::from_be_bytes([
            *data.get(pos).ok_or_else(|| corrupt("truncated segment"))?,
            *data.get(pos + 1).ok_or_else(|| corrupt("truncated segment"))?,
        ]) as usize;
        if len < 2 {
            return Err(corrupt("bad segment length"));
        }
        let body = data.get(pos + 2..pos + len).ok_or_else(|| corrupt("truncated segment"))?;
        pos += len;
        match marker {
            0xC0..=0xC2 => dec.parse_sof(body, marker == 0xC2)?,
            0xC3 | 0xC5..=0xC7 => return Err(unsupported("lossless / hierarchical JPEG")),
            0xC8..=0xCB | 0xCD..=0xCF => return Err(unsupported("arithmetic-coded JPEG")),
            0xC4 => dec.parse_dht(body)?,
            0xDB => dec.parse_dqt(body)?,
            0xDD => {
                dec.restart_interval =
                    u16::from_be_bytes(body.get(..2).and_then(|b| b.try_into().ok()).ok_or_else(|| corrupt("DRI"))?)
                        as usize;
            }
            0xDA => pos = scan::decode_scan(&mut dec, body, pos)?,
            0xDC => return Err(unsupported("DNL segments")),
            0xE0 if body.starts_with(b"JFIF\0") => dec.jfif = true,
            0xEE if body.starts_with(b"Adobe") => dec.adobe_transform = body.get(11).copied(),
            _ => {} // other APPn, COM, DHP/EXP, reserved: skip
        }
    }
    dec.finish()
}

impl Decoder<'_> {
    fn parse_sof(&mut self, body: &[u8], progressive: bool) -> Result<(), ImageError> {
        if self.frame.is_some() {
            return Err(corrupt("duplicate SOF"));
        }
        if body.len() < 6 {
            return Err(corrupt("SOF header"));
        }
        if body[0] != 8 {
            return Err(unsupported(&format!("{}-bit precision", body[0])));
        }
        let height = u16::from_be_bytes([body[1], body[2]]) as usize;
        let width = u16::from_be_bytes([body[3], body[4]]) as usize;
        let n = body[5] as usize;
        if height == 0 {
            return Err(unsupported("height defined by a DNL segment"));
        }
        if width == 0 {
            return Err(corrupt("zero width"));
        }
        if (width as u64) * (height as u64) > MAX_PIXELS {
            return Err(unsupported(&format!("{width}x{height} is too large")));
        }
        match n {
            1 | 3 => {}
            0 => return Err(corrupt("no components")),
            4 => return Err(unsupported("CMYK / YCCK JPEG")),
            _ => return Err(unsupported(&format!("{n}-component JPEG"))),
        }
        if body.len() < 6 + n * 3 {
            return Err(corrupt("SOF header"));
        }
        let mut components = Vec::with_capacity(n);
        for i in 0..n {
            let (id, hv, tq) = (body[6 + i * 3], body[7 + i * 3], body[8 + i * 3] as usize);
            let (h, v) = ((hv >> 4) as usize, (hv & 15) as usize);
            if !(1..=4).contains(&h) || !(1..=4).contains(&v) || tq > 3 {
                return Err(corrupt("bad component sampling or table"));
            }
            if components.iter().any(|c: &Component| c.id == id) {
                return Err(corrupt("duplicate component id"));
            }
            components.push(Component {
                id,
                h,
                v,
                tq,
                quant: None,
                blocks_w: 0,
                blocks_h: 0,
                coefs: Vec::new(),
                dc_table: 0,
                ac_table: 0,
                dc_pred: 0,
            });
        }
        let hmax = components.iter().map(|c| c.h).max().unwrap_or(1);
        let vmax = components.iter().map(|c| c.v).max().unwrap_or(1);
        if components.iter().any(|c| hmax % c.h != 0 || vmax % c.v != 0) {
            return Err(unsupported("fractional chroma sampling"));
        }
        let mcus_x = width.div_ceil(8 * hmax);
        let mcus_y = height.div_ceil(8 * vmax);
        for c in &mut components {
            c.blocks_w = mcus_x * c.h;
            c.blocks_h = mcus_y * c.v;
            c.coefs = vec![0i16; c.blocks_w * c.blocks_h * 64];
        }
        self.frame = Some(Frame { width, height, progressive, components, hmax, vmax, mcus_x, mcus_y });
        Ok(())
    }

    fn parse_dqt(&mut self, mut body: &[u8]) -> Result<(), ImageError> {
        while let Some(&pq_tq) = body.first() {
            let (sixteen, tq) = (pq_tq >> 4 != 0, (pq_tq & 15) as usize);
            if tq > 3 {
                return Err(corrupt("bad quantisation table id"));
            }
            let n = if sixteen { 128 } else { 64 };
            let raw = body.get(1..1 + n).ok_or_else(|| corrupt("truncated DQT"))?;
            let mut table = [0u16; 64];
            for k in 0..64 {
                let q = if sixteen { u16::from_be_bytes([raw[k * 2], raw[k * 2 + 1]]) } else { raw[k] as u16 };
                if q == 0 {
                    return Err(corrupt("zero quantisation value"));
                }
                table[idct::ZIGZAG[k]] = q;
            }
            self.quant[tq] = Some(table);
            body = &body[1 + n..];
        }
        Ok(())
    }

    fn parse_dht(&mut self, mut body: &[u8]) -> Result<(), ImageError> {
        while let Some(&tc_th) = body.first() {
            let (class, th) = (tc_th >> 4, (tc_th & 15) as usize);
            if class > 1 || th > 3 {
                return Err(corrupt("bad Huffman table id"));
            }
            let counts: [u8; 16] =
                body.get(1..17).and_then(|c| c.try_into().ok()).ok_or_else(|| corrupt("truncated DHT"))?;
            let total: usize = counts.iter().map(|&c| c as usize).sum();
            let values = body.get(17..17 + total).ok_or_else(|| corrupt("truncated DHT"))?;
            let table = HuffTable::build(&counts, values)?;
            if class == 0 {
                self.dc_tables[th] = Some(table);
            } else {
                self.ac_tables[th] = Some(table);
            }
            body = &body[17 + total..];
        }
        Ok(())
    }

    /// IDCT every block, upsample, convert.
    fn finish(mut self) -> Result<Image, ImageError> {
        let frame = self.frame.take().ok_or_else(|| corrupt("no frame header"))?;
        if self.scans_done == 0 {
            return Err(corrupt("no scan data"));
        }
        let colour = self.colour(&frame)?;
        let mut planes = Vec::with_capacity(frame.components.len());
        for c in &frame.components {
            let quant = c.quant.ok_or_else(|| corrupt("component never scanned"))?;
            let sq = idct::scaled_quant(&quant);
            let stride = c.blocks_w * 8;
            let mut plane = vec![0u8; stride * c.blocks_h * 8];
            for by in 0..c.blocks_h {
                for bx in 0..c.blocks_w {
                    let i = (by * c.blocks_w + bx) * 64;
                    let coef: &[i16; 64] = c.coefs[i..i + 64].try_into().expect("64 coefficients");
                    idct::idct_block(coef, &sq, &mut plane[by * 8 * stride + bx * 8..], stride);
                }
            }
            let ratio = (frame.hmax / c.h, frame.vmax / c.v);
            let size = (c.width(&frame), c.height(&frame));
            planes.push(color::upsample(&plane, stride, size, ratio, (frame.width, frame.height)));
        }
        let rgba = color::to_rgba(&planes, colour, frame.width * frame.height);
        Ok(Image { width: frame.width as u32, height: frame.height as u32, rgba })
    }

    /// libjpeg's colour-space guess: JFIF means YCbCr, Adobe says which,
    /// otherwise component ids 'R','G','B' mean RGB.
    fn colour(&self, frame: &Frame) -> Result<Colour, ImageError> {
        let ids: Vec<u8> = frame.components.iter().map(|c| c.id).collect();
        Ok(match ids.len() {
            1 => Colour::Gray,
            _ if self.jfif => Colour::YCbCr,
            _ => match self.adobe_transform {
                Some(0) => Colour::Rgb,
                Some(_) => Colour::YCbCr,
                None if ids == [b'R', b'G', b'B'] => Colour::Rgb,
                None => Colour::YCbCr,
            },
        })
    }
}
