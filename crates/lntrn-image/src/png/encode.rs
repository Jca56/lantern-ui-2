//! PNG writer: RGBA8, no filtering, stored (uncompressed) deflate blocks
//! inside a zlib stream. Big and quick, with exact pixels: what the
//! clipboard and a screenshot want. Compression can come later without
//! changing a caller.

use crate::Image;

/// `img` as a PNG file.
pub fn encode(img: &Image) -> Vec<u8> {
    let row = img.width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * img.height as usize);
    for y in 0..img.height as usize {
        raw.push(0); // filter: none
        raw.extend_from_slice(&img.rgba[y * row..(y + 1) * row]);
    }
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 64);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&img.width.to_be_bytes());
    ihdr.extend_from_slice(&img.height.to_be_bytes());
    // 8 bits per sample, RGBA, deflate, adaptive filtering, no interlace.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// `data` as a zlib stream (RFC 1950) of stored deflate blocks (RFC 1951,
/// BTYPE 00): no compression, sixty-five kilobytes a block.
pub fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 11);
    // CMF: deflate with a 32 KB window; FLG: no dictionary, fastest level;
    // together a multiple of 31, as the check demands.
    out.extend_from_slice(&[0x78, 0x01]);
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    let mut blocks = data.chunks(65535).peekable();
    while let Some(block) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none())); // BFINAL, BTYPE 00
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &x in data {
        crc ^= u32::from(x);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inflate::inflate;

    #[test]
    fn checksums() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn stored_zlib_inflates_back() {
        let big: Vec<u8> = (0..150_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let z = zlib_stored(&big);
        assert_eq!(inflate(&z, big.len()).as_deref(), Some(big.as_slice()), "three blocks");
        assert_eq!(inflate(&zlib_stored(&[]), 0).as_deref(), Some(&[][..]));
    }

    #[test]
    fn png_round_trips() {
        let mut img = Image::solid(37, 19, [0, 0, 0, 255]);
        for (i, px) in img.rgba.chunks_mut(4).enumerate() {
            px[0] = (i % 256) as u8;
            px[1] = (i / 3 % 256) as u8;
            px[2] = 200;
            px[3] = if i % 5 == 0 { 128 } else { 255 };
        }
        let bytes = encode(&img);
        assert_eq!(crate::Format::sniff(&bytes), Some(crate::Format::Png));
        let back = crate::decode(&bytes).expect("our own PNG decodes");
        assert_eq!(back, img);
    }
}
