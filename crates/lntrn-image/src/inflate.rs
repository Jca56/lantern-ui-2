//! DEFLATE (RFC 1951) inside a zlib wrapper (RFC 1950): the PNG payload.
//! Table-driven canonical Huffman decoding, pure std. Moved here from
//! lntrn-text's emoji PNG loader and sped up with lookup tables.

/// Inflate a zlib stream. `size_hint` is the expected output size: it sizes
/// the buffer and bounds how much we are willing to produce, so a hostile
/// stream cannot make us allocate without limit (anything past a few times
/// the hint is rejected). Returns `None` on any malformed input. The
/// trailing Adler-32 is not checked.
pub fn inflate(zlib: &[u8], size_hint: usize) -> Option<Vec<u8>> {
    if zlib.len() < 2 {
        return None;
    }
    let (cmf, flg) = (zlib[0], zlib[1]);
    if cmf & 0x0F != 8 || flg & 0x20 != 0 || !(cmf as u32 * 256 + flg as u32).is_multiple_of(31) {
        return None; // not deflate / preset dictionary / bad check
    }
    let max_out = size_hint.saturating_mul(4).max(1 << 20);
    let mut br = BitReader::new(&zlib[2..]);
    let mut out: Vec<u8> = Vec::with_capacity(size_hint.min(1 << 24));
    loop {
        let last = br.bits(1)?;
        match br.bits(2)? {
            0 => stored_block(&mut br, &mut out)?,
            1 => {
                let (lit, dist) = fixed_tables();
                inflate_block(&mut br, &lit, &dist, &mut out, max_out)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut br)?;
                inflate_block(&mut br, &lit, &dist, &mut out, max_out)?;
            }
            _ => return None,
        }
        if last == 1 {
            return Some(out);
        }
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u64,
    nbits: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, acc: 0, nbits: 0 }
    }

    fn refill(&mut self) {
        while self.nbits <= 56 {
            let Some(&b) = self.data.get(self.pos) else { break };
            self.pos += 1;
            self.acc |= (b as u64) << self.nbits;
            self.nbits += 8;
        }
    }

    /// Read `n` (≤ 32) bits, LSB first. `None` when the input is exhausted.
    fn bits(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        self.refill();
        if self.nbits < n {
            return None;
        }
        let v = (self.acc & ((1u64 << n) - 1)) as u32;
        self.acc >>= n;
        self.nbits -= n;
        Some(v)
    }

    /// Look at the next `n` bits without consuming; missing input reads as 0.
    fn peek(&mut self, n: u32) -> usize {
        self.refill();
        (self.acc & ((1u64 << n) - 1)) as usize
    }

    fn consume(&mut self, n: u32) -> Option<()> {
        if self.nbits < n {
            return None;
        }
        self.acc >>= n;
        self.nbits -= n;
        Some(())
    }

    /// Drop the rest of the current byte (stored blocks start byte-aligned).
    fn align_byte(&mut self) {
        let r = self.nbits % 8;
        self.acc >>= r;
        self.nbits -= r;
    }
}

/// Canonical Huffman decoder as one lookup table indexed by the next
/// `bits` input bits (bit-reversed, since DEFLATE packs codes MSB first
/// into an LSB-first stream). Entry = `len << 9 | symbol`, 0 = invalid.
struct Huffman {
    table: Vec<u16>,
    bits: u32,
}

impl Huffman {
    fn build(code_lengths: &[u8]) -> Option<Huffman> {
        let max_len = code_lengths.iter().copied().max().unwrap_or(0) as u32;
        if max_len > 15 || code_lengths.len() > 512 {
            return None;
        }
        let bits = max_len.max(1);
        let mut count = [0u32; 16];
        for &l in code_lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for len in 1..16 {
            code = (code + count[len - 1]) << 1;
            next_code[len] = code;
        }
        let mut table = vec![0u16; 1 << bits];
        for (sym, &len) in code_lengths.iter().enumerate() {
            if len == 0 {
                continue;
            }
            let len = len as u32;
            let code = next_code[len as usize];
            next_code[len as usize] += 1;
            if code >= 1 << len {
                return None; // over-subscribed
            }
            let rev = (code.reverse_bits() >> (32 - len)) as usize;
            let entry = ((len as u16) << 9) | sym as u16;
            let mut k = rev;
            while k < table.len() {
                table[k] = entry;
                k += 1 << len;
            }
        }
        Some(Huffman { table, bits })
    }

    fn decode(&self, br: &mut BitReader) -> Option<u16> {
        let entry = self.table[br.peek(self.bits)];
        if entry == 0 {
            return None;
        }
        br.consume((entry >> 9) as u32)?;
        Some(entry & 0x1FF)
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn stored_block(br: &mut BitReader, out: &mut Vec<u8>) -> Option<()> {
    br.align_byte();
    let len = br.bits(16)? as usize;
    let nlen = br.bits(16)? as usize;
    if len != !nlen & 0xFFFF {
        return None;
    }
    let mut remaining = len;
    // Whole bytes already buffered come first, then a straight copy.
    while remaining > 0 && br.nbits >= 8 {
        out.push(br.acc as u8);
        br.acc >>= 8;
        br.nbits -= 8;
        remaining -= 1;
    }
    let src = br.data.get(br.pos..br.pos.checked_add(remaining)?)?;
    out.extend_from_slice(src);
    br.pos += remaining;
    Some(())
}

fn fixed_tables() -> (Huffman, Huffman) {
    let mut litlen = [0u8; 288];
    for (i, l) in litlen.iter_mut().enumerate() {
        *l = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let lit = Huffman::build(&litlen).expect("fixed literal table is valid");
    let dist = Huffman::build(&[5u8; 30]).expect("fixed distance table is valid");
    (lit, dist)
}

fn dynamic_tables(br: &mut BitReader) -> Option<(Huffman, Huffman)> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;
    let mut cl_lengths = [0u8; 19];
    for &slot in CL_ORDER.iter().take(hclen) {
        cl_lengths[slot] = br.bits(3)? as u8;
    }
    let cl = Huffman::build(&cl_lengths)?;
    let mut lengths = vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < lengths.len() {
        let (value, repeat) = match cl.decode(br)? {
            sym @ 0..=15 => (sym as u8, 1),
            16 => (*lengths.get(i.checked_sub(1)?)?, br.bits(2)? as usize + 3),
            17 => (0, br.bits(3)? as usize + 3),
            18 => (0, br.bits(7)? as usize + 11),
            _ => return None,
        };
        if i + repeat > lengths.len() {
            return None;
        }
        lengths[i..i + repeat].fill(value);
        i += repeat;
    }
    if lengths[256] == 0 {
        return None; // no end-of-block code
    }
    Some((Huffman::build(&lengths[..hlit])?, Huffman::build(&lengths[hlit..])?))
}

fn inflate_block(
    br: &mut BitReader,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    max_out: usize,
) -> Option<()> {
    loop {
        let sym = lit.decode(br)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Some(()),
            257..=285 => {
                let li = sym as usize - 257;
                let length = LENGTH_BASE[li] as usize + br.bits(LENGTH_EXTRA[li] as u32)? as usize;
                let ds = dist.decode(br)? as usize;
                if ds >= 30 {
                    return None;
                }
                let distance = DIST_BASE[ds] as usize + br.bits(DIST_EXTRA[ds] as u32)? as usize;
                if distance > out.len() {
                    return None;
                }
                let start = out.len() - distance;
                if distance >= length {
                    out.extend_from_within(start..start + length);
                } else {
                    for k in 0..length {
                        out.push(out[start + k]);
                    }
                }
            }
            _ => return None,
        }
        if out.len() > max_out {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// zlib.compress(b"hello hello hello hello", 9): a fixed-table block.
    const HELLO: &[u8] = &[
        0x78, 0xDA, 0xCB, 0x48, 0xCD, 0xC9, 0xC9, 0x57, 0xC8, 0x40, 0x27, 0x01, 0x68, 0x03, 0x08,
        0xB1,
    ];

    #[test]
    fn inflates_fixed_block_with_backrefs() {
        assert_eq!(inflate(HELLO, 23).as_deref(), Some(&b"hello hello hello hello"[..]));
    }

    #[test]
    fn stored_block_roundtrip() {
        // zlib level 0: one stored block.
        let mut z = vec![0x78, 0x01, 0x01, 0x05, 0x00, 0xFA, 0xFF];
        z.extend_from_slice(b"abcde");
        z.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(inflate(&z, 5).as_deref(), Some(&b"abcde"[..]));
    }

    #[test]
    fn rejects_garbage_and_truncation() {
        assert_eq!(inflate(&[0x78], 1), None);
        assert_eq!(inflate(&[0x79, 0x9C, 0x00], 1), None);
        assert_eq!(inflate(&HELLO[..8], 23), None);
        assert_eq!(inflate(&[0x78, 0x01, 0x07], 1), None); // block type 3
    }
}
