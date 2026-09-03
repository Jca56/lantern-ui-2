//! Entropy-coded segment reading: the bit reader (with 0xFF byte stuffing
//! and marker detection) and Huffman tables (9-bit fast lookup plus the
//! canonical slow path for longer codes).

use crate::ImageError;

pub(crate) fn corrupt(msg: &str) -> ImageError {
    ImageError::Corrupt(msg.to_string())
}

/// A JPEG Huffman table built from the DHT counts and values.
pub(crate) struct HuffTable {
    /// Indexed by the next 9 bits: `len << 8 | value`, 0 = longer code.
    fast: [u16; 512],
    /// Largest code of each length (−1 when the length is unused).
    maxcode: [i32; 17],
    /// Value index of the first code of each length minus that code.
    valoffset: [i32; 17],
    values: Vec<u8>,
}

impl HuffTable {
    pub fn build(counts: &[u8; 16], values: &[u8]) -> Result<HuffTable, ImageError> {
        let total: usize = counts.iter().map(|&c| c as usize).sum();
        if total > 256 || values.len() < total {
            return Err(corrupt("bad Huffman table"));
        }
        let mut fast = [0u16; 512];
        let mut maxcode = [-1i32; 17];
        let mut valoffset = [0i32; 17];
        let mut code = 0i32;
        let mut k = 0usize;
        for len in 1..=16usize {
            let n = counts[len - 1] as usize;
            valoffset[len] = k as i32 - code;
            for _ in 0..n {
                if code >= 1 << len {
                    return Err(corrupt("over-subscribed Huffman table"));
                }
                if len <= 9 {
                    let base = (code as usize) << (9 - len);
                    for slot in &mut fast[base..base + (1 << (9 - len))] {
                        *slot = ((len as u16) << 8) | values[k] as u16;
                    }
                }
                code += 1;
                k += 1;
            }
            maxcode[len] = if n > 0 { code - 1 } else { -1 };
            code <<= 1;
        }
        Ok(HuffTable { fast, maxcode, valoffset, values: values[..total].to_vec() })
    }

    pub fn decode(&self, br: &mut BitReader) -> Result<u8, ImageError> {
        let peek = br.peek16();
        let entry = self.fast[(peek >> 7) as usize];
        if entry != 0 {
            br.consume((entry >> 8) as u32);
            return Ok(entry as u8);
        }
        for len in 10..=16u32 {
            let code = (peek >> (16 - len)) as i32;
            if code <= self.maxcode[len as usize] {
                br.consume(len);
                let idx = (code + self.valoffset[len as usize]) as usize;
                return self.values.get(idx).copied().ok_or_else(|| corrupt("bad Huffman code"));
            }
        }
        Err(corrupt("bad Huffman code"))
    }
}

/// MSB-first bit reader over entropy-coded data. Stops at markers (the
/// stream then reads as zeros) and at the end of input. `pos` is where the
/// marker parser should resume.
pub(crate) struct BitReader<'a> {
    data: &'a [u8],
    pub pos: usize,
    acc: u64,
    nbits: u32,
    /// Zero bits appended because the data ran out or hit a marker.
    pad_bits: u32,
    /// The marker byte that stopped us, and its index.
    marker: Option<(u8, usize)>,
    /// A real byte ran out (not a marker) and its padding got consumed.
    pub truncated: bool,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos, acc: 0, nbits: 0, pad_bits: 0, marker: None, truncated: false }
    }

    fn next_byte(&mut self) -> Option<u8> {
        if self.marker.is_some() {
            return None;
        }
        let b = *self.data.get(self.pos)?;
        if b != 0xFF {
            self.pos += 1;
            return Some(b);
        }
        // 0xFF: skip fill bytes; 0x00 means a stuffed data byte, anything
        // else is a marker we must not consume.
        let mut p = self.pos + 1;
        while self.data.get(p) == Some(&0xFF) {
            p += 1;
        }
        match self.data.get(p) {
            Some(0x00) => {
                self.pos = p + 1;
                Some(0xFF)
            }
            Some(&m) => {
                self.marker = Some((m, p));
                None
            }
            None => {
                self.pos = p;
                None
            }
        }
    }

    fn fill(&mut self) {
        while self.nbits <= 56 {
            match self.next_byte() {
                Some(b) => self.acc |= (b as u64) << (56 - self.nbits),
                None => self.pad_bits += 8,
            }
            self.nbits += 8;
        }
    }

    pub fn peek16(&mut self) -> u32 {
        if self.nbits < 16 {
            self.fill();
        }
        (self.acc >> 48) as u32
    }

    pub fn consume(&mut self, n: u32) {
        self.acc <<= n;
        self.nbits -= n;
        if self.nbits < self.pad_bits && self.marker.is_none() {
            self.truncated = true;
        }
    }

    pub fn bits(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = self.peek16() >> (16 - n);
        self.consume(n);
        v
    }

    pub fn bit(&mut self) -> bool {
        self.bits(1) != 0
    }

    /// RECEIVE + EXTEND: an `s`-bit magnitude with the JPEG sign convention.
    pub fn receive_extend(&mut self, s: u32) -> i32 {
        if s == 0 {
            return 0;
        }
        let v = self.bits(s) as i32;
        if v < 1 << (s - 1) { v - (1 << s) + 1 } else { v }
    }

    /// Drop buffered bits and step over the RSTn marker that should follow.
    /// Any other marker is left for the segment parser; the scan then reads
    /// zeros for its remaining MCUs, like libjpeg does.
    pub fn restart(&mut self) {
        self.acc = 0;
        self.nbits = 0;
        self.pad_bits = 0;
        if self.marker.is_none() {
            // Scan forward to the next marker, skipping stuffed bytes.
            while self.pos < self.data.len() {
                if self.data[self.pos] == 0xFF {
                    match self.data.get(self.pos + 1) {
                        Some(0x00) | Some(0xFF) => self.pos += 1,
                        Some(&m) => {
                            self.marker = Some((m, self.pos + 1));
                            break;
                        }
                        None => break,
                    }
                } else {
                    self.pos += 1;
                }
            }
        }
        if let Some((m, at)) = self.marker
            && (0xD0..=0xD7).contains(&m)
        {
            self.pos = at + 1;
            self.marker = None;
        }
    }

    /// Where the segment parser continues after this scan.
    pub fn end_pos(&self) -> usize {
        match self.marker {
            Some((_, at)) => at - 1,
            None => self.pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bits_and_stuffed_bytes() {
        let data = [0b1010_0000, 0xFF, 0x00, 0xFF, 0xD9];
        let mut br = BitReader::new(&data, 0);
        assert_eq!(br.bits(3), 0b101);
        assert_eq!(br.bits(5), 0);
        assert_eq!(br.bits(8), 0xFF);
        assert_eq!(br.bits(8), 0); // marker: zeros, not truncated
        assert!(!br.truncated);
        assert_eq!(br.end_pos(), 3);
    }

    #[test]
    fn truncation_is_flagged_only_when_padding_is_consumed() {
        let data = [0x12];
        let mut br = BitReader::new(&data, 0);
        assert_eq!(br.bits(8), 0x12);
        assert!(!br.truncated);
        br.bits(1);
        assert!(br.truncated);
    }

    #[test]
    fn decodes_short_and_long_codes() {
        // Two 1-bit codes are impossible (that would be complete); use one
        // 1-bit code, one 2-bit code, and one 12-bit code.
        let mut counts = [0u8; 16];
        counts[0] = 1;
        counts[1] = 1;
        counts[11] = 1;
        let t = HuffTable::build(&counts, &[7, 8, 9]).unwrap();
        // codes: 0, 10, 110000000000
        let data = [0b0101_1000, 0b0000_0000, 0b0000_0000];
        let mut br = BitReader::new(&data, 0);
        assert_eq!(t.decode(&mut br).unwrap(), 7);
        assert_eq!(t.decode(&mut br).unwrap(), 8);
        assert_eq!(t.decode(&mut br).unwrap(), 9);
        assert_eq!(br.receive_extend(3), -7);
    }
}
