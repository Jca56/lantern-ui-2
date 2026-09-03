//! PNG decoder: every colour type and bit depth, tRNS, all five filters,
//! Adam7 interlacing. CRCs, gamma and colour-space chunks are ignored.
//! 16-bit samples keep their high byte. The writer (`encode`) does RGBA8
//! with stored deflate blocks.

mod encode;
mod rows;

pub use encode::{encode, zlib_stored};

use crate::inflate::inflate;
use crate::{Image, ImageError};

/// Refuse anything bigger than this many pixels (RGBA8 would be 512 MB).
const MAX_PIXELS: u64 = 1 << 27;
const MAX_SIDE: u32 = 1 << 16;

/// Everything the scanline expander needs to know about the picture.
pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    pub depth: u8,
    pub color_type: u8,
    pub interlaced: bool,
}

impl Header {
    pub fn channels(&self) -> usize {
        match self.color_type {
            0 | 3 => 1,
            4 => 2,
            2 => 3,
            _ => 4,
        }
    }

    /// Bytes per complete pixel, at least 1 (the filter's "bpp").
    pub fn bpp(&self) -> usize {
        (self.channels() * self.depth as usize).div_ceil(8).max(1)
    }

    /// Bytes in one unfiltered scanline of `width` pixels.
    pub fn row_bytes(&self, width: u32) -> usize {
        (width as usize * self.channels() * self.depth as usize).div_ceil(8)
    }
}

/// Transparency from a tRNS chunk, interpreted per colour type.
pub(crate) enum Trns {
    None,
    /// Palette: alpha per index (shorter than the palette means opaque rest).
    Palette(Vec<u8>),
    /// Gray colour key, at the image's bit depth.
    Gray(u16),
    /// RGB colour key, at the image's bit depth.
    Rgb([u16; 3]),
}

fn corrupt(msg: &str) -> ImageError {
    ImageError::Corrupt(msg.to_string())
}

fn be32(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?))
}

fn be16(b: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(..2)?.try_into().ok()?))
}

pub fn decode(data: &[u8]) -> Result<Image, ImageError> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.len() < 8 || data[..8] != SIG {
        return Err(corrupt("bad PNG signature"));
    }
    let mut pos = 8usize;
    let mut header: Option<Header> = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns_raw: Option<Vec<u8>> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_iend = false;

    while pos + 8 <= data.len() {
        let len = be32(&data[pos..]).ok_or_else(|| corrupt("chunk length"))? as usize;
        if len > i32::MAX as usize {
            return Err(corrupt("chunk length"));
        }
        let ctype = &data[pos + 4..pos + 8];
        let body = data
            .get(pos + 8..pos + 8 + len)
            .ok_or_else(|| corrupt("truncated chunk"))?;
        match ctype {
            b"IHDR" => {
                if header.is_some() {
                    return Err(corrupt("duplicate IHDR"));
                }
                header = Some(parse_ihdr(body)?);
            }
            b"PLTE" => {
                if !len.is_multiple_of(3) || len == 0 || len > 768 {
                    return Err(corrupt("bad PLTE length"));
                }
                palette = body.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            }
            b"tRNS" => trns_raw = Some(body.to_vec()),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => {
                seen_iend = true;
                break;
            }
            _ => {}
        }
        if header.is_none() {
            return Err(corrupt("IHDR must come first"));
        }
        pos += 12 + len; // length + type + data + crc
    }
    let header = header.ok_or_else(|| corrupt("missing IHDR"))?;
    if !seen_iend && idat.is_empty() {
        return Err(corrupt("truncated before image data"));
    }
    if idat.is_empty() {
        return Err(corrupt("no IDAT"));
    }
    if header.color_type == 3 && palette.is_empty() {
        return Err(corrupt("palette image without PLTE"));
    }
    let trns = parse_trns(&header, trns_raw.as_deref())?;

    let raw_len = expected_raw_len(&header);
    let raw = inflate(&idat, raw_len).ok_or_else(|| corrupt("bad IDAT deflate stream"))?;
    if raw.len() < raw_len {
        return Err(corrupt("IDAT shorter than the image"));
    }

    let mut rgba = vec![0u8; header.width as usize * header.height as usize * 4];
    let mut expander = rows::Expander::new(&header, &palette, &trns);
    if header.interlaced {
        let mut offset = 0usize;
        for (pass, &(x0, y0, dx, dy)) in ADAM7.iter().enumerate() {
            let (pw, ph) = pass_size(&header, pass);
            if pw == 0 || ph == 0 {
                continue;
            }
            let row_bytes = header.row_bytes(pw);
            let end = offset + (row_bytes + 1) * ph as usize;
            let lines = rows::unfilter(&raw[offset..end], row_bytes, header.bpp(), ph as usize)?;
            offset = end;
            for (r, line) in lines.chunks_exact(row_bytes).enumerate() {
                let y = y0 + r as u32 * dy;
                expander.expand_row(line, pw, |i, px| {
                    let x = x0 + i as u32 * dx;
                    let o = (y as usize * header.width as usize + x as usize) * 4;
                    rgba[o..o + 4].copy_from_slice(&px);
                })?;
            }
        }
    } else {
        let row_bytes = header.row_bytes(header.width);
        let lines = rows::unfilter(&raw[..raw_len], row_bytes, header.bpp(), header.height as usize)?;
        for (y, line) in lines.chunks_exact(row_bytes).enumerate() {
            let row = &mut rgba[y * header.width as usize * 4..(y + 1) * header.width as usize * 4];
            expander.expand_row(line, header.width, |i, px| {
                row[i * 4..i * 4 + 4].copy_from_slice(&px);
            })?;
        }
    }
    Ok(Image { width: header.width, height: header.height, rgba })
}

fn parse_ihdr(body: &[u8]) -> Result<Header, ImageError> {
    if body.len() != 13 {
        return Err(corrupt("IHDR length"));
    }
    let width = be32(body).ok_or_else(|| corrupt("IHDR"))?;
    let height = be32(&body[4..]).ok_or_else(|| corrupt("IHDR"))?;
    let (depth, color_type, compression, filter, interlace) =
        (body[8], body[9], body[10], body[11], body[12]);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(corrupt("bad image dimensions"));
    }
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err(ImageError::Unsupported(format!("{width}x{height} is too large")));
    }
    let depth_ok = match color_type {
        0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
        3 => matches!(depth, 1 | 2 | 4 | 8),
        2 | 4 | 6 => matches!(depth, 8 | 16),
        _ => return Err(corrupt("bad colour type")),
    };
    if !depth_ok {
        return Err(corrupt("bad bit depth for colour type"));
    }
    if compression != 0 || filter != 0 {
        return Err(corrupt("unknown compression or filter method"));
    }
    let interlaced = match interlace {
        0 => false,
        1 => true,
        _ => return Err(corrupt("unknown interlace method")),
    };
    Ok(Header { width, height, depth, color_type, interlaced })
}

fn parse_trns(h: &Header, raw: Option<&[u8]>) -> Result<Trns, ImageError> {
    let Some(raw) = raw else { return Ok(Trns::None) };
    Ok(match h.color_type {
        3 => Trns::Palette(raw.to_vec()),
        0 => Trns::Gray(be16(raw).ok_or_else(|| corrupt("tRNS length"))?),
        2 => {
            if raw.len() < 6 {
                return Err(corrupt("tRNS length"));
            }
            let key = |i: usize| be16(&raw[i * 2..]).expect("checked length");
            Trns::Rgb([key(0), key(1), key(2)])
        }
        _ => Trns::None, // tRNS is not allowed with an alpha channel; ignore it
    })
}

/// Adam7 pass geometry: (x start, y start, x step, y step).
const ADAM7: [(u32, u32, u32, u32); 7] =
    [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];

fn pass_size(h: &Header, pass: usize) -> (u32, u32) {
    let (x0, y0, dx, dy) = ADAM7[pass];
    let pw = if h.width > x0 { (h.width - x0).div_ceil(dx) } else { 0 };
    let ph = if h.height > y0 { (h.height - y0).div_ceil(dy) } else { 0 };
    (pw, ph)
}

/// Bytes of filtered scanline data the image needs.
fn expected_raw_len(h: &Header) -> usize {
    if h.interlaced {
        (0..7)
            .map(|p| {
                let (pw, ph) = pass_size(h, p);
                if pw == 0 || ph == 0 { 0 } else { (h.row_bytes(pw) + 1) * ph as usize }
            })
            .sum()
    } else {
        (h.row_bytes(h.width) + 1) * h.height as usize
    }
}
