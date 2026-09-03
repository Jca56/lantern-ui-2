//! Lantern image: pure-std decoders for the pictures textures come from,
//! PNG (with our own DEFLATE inflater) and JPEG (baseline and progressive),
//! and a PNG writer for the clipboard and screenshots.
//! Pixels come out as straight (non-premultiplied) RGBA8 in sRGB, rows top
//! to bottom. Nothing here knows about the GPU or the document.

pub mod inflate;
pub mod jpeg;
pub mod png;

use core::fmt;

/// A decoded picture: RGBA8, straight alpha, sRGB, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        debug_assert_eq!(rgba.len(), width as usize * height as usize * 4);
        Self { width, height, rgba }
    }

    /// A solid image, for placeholders and tests.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        Self { width, height, rgba: rgba.repeat(width as usize * height as usize) }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba[i..i + 4].try_into().expect("in bounds")
    }

    /// Half size in each direction (at least 1×1), each output pixel the
    /// plain average of the 2×2 block it covers. Odd edges reuse the last
    /// row / column. This is the next level of a mip chain.
    pub fn half(&self) -> Image {
        let (w, h) = (self.width.max(1), self.height.max(1));
        let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
        let mut out = Vec::with_capacity(hw as usize * hh as usize * 4);
        for y in 0..hh {
            let y0 = (y * 2).min(h - 1);
            let y1 = (y * 2 + 1).min(h - 1);
            for x in 0..hw {
                let x0 = (x * 2).min(w - 1);
                let x1 = (x * 2 + 1).min(w - 1);
                let ps = [self.pixel(x0, y0), self.pixel(x1, y0), self.pixel(x0, y1), self.pixel(x1, y1)];
                for c in 0..4 {
                    let sum: u32 = ps.iter().map(|p| p[c] as u32).sum();
                    out.push(((sum + 2) / 4) as u8);
                }
            }
        }
        Image { width: hw, height: hh, rgba: out }
    }

    /// The image followed by every half-size level down to 1×1.
    pub fn mip_chain(&self) -> Vec<Image> {
        let mut levels = vec![self.clone()];
        while levels.last().is_some_and(|l| l.width > 1 || l.height > 1) {
            let next = levels.last().expect("non-empty").half();
            levels.push(next);
        }
        levels
    }
}

/// What kind of file some bytes are, by signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

impl Format {
    pub fn sniff(bytes: &[u8]) -> Option<Format> {
        if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Some(Format::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            Some(Format::Jpeg)
        } else {
            None
        }
    }

    /// The MIME type exchange formats (glTF) name this by.
    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
        }
    }

    /// The usual file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// Not a PNG or JPEG.
    UnknownFormat,
    /// The file is broken.
    Corrupt(String),
    /// Valid, but a feature this decoder does not handle.
    Unsupported(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::UnknownFormat => write!(f, "not a PNG or JPEG image"),
            ImageError::Corrupt(s) => write!(f, "broken image: {s}"),
            ImageError::Unsupported(s) => write!(f, "unsupported image: {s}"),
        }
    }
}

impl std::error::Error for ImageError {}

/// `img` as a PNG file (RGBA8, uncompressed).
pub fn encode_png(img: &Image) -> Vec<u8> {
    png::encode(img)
}

/// Decode a PNG or JPEG, whichever the bytes are.
pub fn decode(bytes: &[u8]) -> Result<Image, ImageError> {
    match Format::sniff(bytes) {
        Some(Format::Png) => png::decode(bytes),
        Some(Format::Jpeg) => jpeg::decode(bytes),
        None => Err(ImageError::UnknownFormat),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_averages_blocks_and_stops_at_one_pixel() {
        let mut img = Image::solid(4, 2, [0, 0, 0, 255]);
        img.rgba[0] = 200;
        img.rgba[4] = 100;
        let h = img.half();
        assert_eq!((h.width, h.height), (2, 1));
        assert_eq!(h.pixel(0, 0), [75, 0, 0, 255]);
        let chain = Image::solid(5, 3, [9, 9, 9, 9]).mip_chain();
        let sizes: Vec<(u32, u32)> = chain.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, vec![(5, 3), (2, 1), (1, 1)]);
        let one = Image::solid(1, 1, [1, 2, 3, 4]).half();
        assert_eq!((one.width, one.height, one.pixel(0, 0)), (1, 1, [1, 2, 3, 4]));
    }

    #[test]
    fn sniffs_signatures() {
        assert_eq!(Format::sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]), Some(Format::Png));
        assert_eq!(Format::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Format::Jpeg));
        assert_eq!(Format::sniff(b"hello"), None);
        assert_eq!(decode(b"hello"), Err(ImageError::UnknownFormat));
    }
}
