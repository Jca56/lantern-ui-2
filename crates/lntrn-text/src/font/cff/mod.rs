//! CFF (Compact Font Format) — PostScript-flavored `.otf` outlines.
//!
//! Parses the `CFF ` table's INDEX/DICT structures and interprets Type2
//! charstrings into cubic-bézier [`Outline`](crate::raster::outline::Outline)s, including local/global
//! subroutines (with count-based bias), hint skipping, the flex family, and
//! CID-keyed fonts (FDSelect → per-FD private dicts, e.g. Noto CJK OTFs).
//! `seac`-style accent composition via 4-argument `endchar` is not emitted
//! by modern tools and is skipped (logged). CFF2 arrives with variable fonts
//! in Phase 10.

use super::sfnt::{read_u16_at, read_u32_at, read_u8_at};
use super::FontError;
mod charstring;

pub(crate) use charstring::outline;

/// Charstring operand-stack limit per the Type2 spec.
const MAX_STACK: usize = 48;
const MAX_SUBR_DEPTH: u8 = 10;

// ── INDEX ───────────────────────────────────────────────────────────────────

/// A parsed CFF INDEX: `count`, offset array, then packed data.
#[derive(Clone, Copy, Debug, Default)]
struct Index {
    count: u32,
    off_size: u8,
    /// Absolute position of the offset array.
    offsets: usize,
    /// Absolute position offsets are relative to (data start − 1).
    data_base: usize,
    /// Absolute end of the whole INDEX structure.
    end: usize,
}

impl Index {
    fn parse(d: &[u8], pos: usize) -> Result<Index, FontError> {
        let count = read_u16_at(d, pos)? as u32;
        if count == 0 {
            return Ok(Index {
                end: pos + 2,
                ..Index::default()
            });
        }
        let off_size = read_u8_at(d, pos + 2)?;
        if !(1..=4).contains(&off_size) {
            return Err(FontError::Truncated);
        }
        let offsets = pos + 3;
        let data_base = offsets + (count as usize + 1) * off_size as usize - 1;
        let last = read_offset(d, offsets + count as usize * off_size as usize, off_size)?;
        Ok(Index {
            count,
            off_size,
            offsets,
            data_base,
            end: data_base + last as usize,
        })
    }

    /// Byte range of item `i`, absolute.
    fn get(&self, d: &[u8], i: u32) -> Result<(usize, usize), FontError> {
        if i >= self.count {
            return Err(FontError::Truncated);
        }
        let sz = self.off_size as usize;
        let a = read_offset(d, self.offsets + i as usize * sz, self.off_size)? as usize;
        let b = read_offset(d, self.offsets + (i as usize + 1) * sz, self.off_size)? as usize;
        if a == 0 || b < a {
            return Err(FontError::Truncated);
        }
        Ok((self.data_base + a, self.data_base + b))
    }
}

fn read_offset(d: &[u8], pos: usize, size: u8) -> Result<u32, FontError> {
    let mut v = 0u32;
    for k in 0..size as usize {
        v = (v << 8) | read_u8_at(d, pos + k)? as u32;
    }
    Ok(v)
}

// ── DICT ────────────────────────────────────────────────────────────────────

/// Parse a DICT, returning the operands for each operator we care about.
/// Operators are keyed as `op` for single-byte and `1200 + op` for 12-x.
fn parse_dict(d: &[u8], start: usize, end: usize) -> Vec<(u16, Vec<f64>)> {
    let mut out = Vec::new();
    let mut operands: Vec<f64> = Vec::new();
    let mut pos = start;
    while pos < end {
        let Ok(b0) = read_u8_at(d, pos) else {
            break;
        };
        match b0 {
            32..=246 => {
                operands.push(b0 as f64 - 139.0);
                pos += 1;
            }
            247..=250 => {
                let Ok(b1) = read_u8_at(d, pos + 1) else {
                    break;
                };
                operands.push((b0 as f64 - 247.0) * 256.0 + b1 as f64 + 108.0);
                pos += 2;
            }
            251..=254 => {
                let Ok(b1) = read_u8_at(d, pos + 1) else {
                    break;
                };
                operands.push(-(b0 as f64 - 251.0) * 256.0 - b1 as f64 - 108.0);
                pos += 2;
            }
            28 => {
                let Ok(v) = read_u16_at(d, pos + 1) else {
                    break;
                };
                operands.push(v as i16 as f64);
                pos += 3;
            }
            29 => {
                let Ok(v) = read_u32_at(d, pos + 1) else {
                    break;
                };
                operands.push(v as i32 as f64);
                pos += 5;
            }
            30 => {
                // Real number: nibble-encoded decimal.
                let mut s = String::new();
                pos += 1;
                'nibbles: while let Ok(byte) = read_u8_at(d, pos) {
                    pos += 1;
                    for nib in [byte >> 4, byte & 0xF] {
                        match nib {
                            0..=9 => s.push((b'0' + nib) as char),
                            0xA => s.push('.'),
                            0xB => s.push('E'),
                            0xC => s.push_str("E-"),
                            0xE => s.push('-'),
                            0xF => break 'nibbles,
                            _ => {}
                        }
                    }
                }
                operands.push(s.parse().unwrap_or(0.0));
            }
            12 => {
                let Ok(b1) = read_u8_at(d, pos + 1) else {
                    break;
                };
                out.push((1200 + b1 as u16, std::mem::take(&mut operands)));
                pos += 2;
            }
            0..=21 => {
                out.push((b0 as u16, std::mem::take(&mut operands)));
                pos += 1;
            }
            _ => break, // reserved
        }
    }
    out
}

fn dict_get(dict: &[(u16, Vec<f64>)], op: u16) -> Option<&[f64]> {
    dict.iter()
        .find(|(o, _)| *o == op)
        .map(|(_, v)| v.as_slice())
}

// ── Font-level structures ───────────────────────────────────────────────────

struct PrivateData {
    local_subrs: Index,
}

enum FdData {
    Single(PrivateData),
    Cid {
        /// Absolute offset of FDSelect.
        fd_select: usize,
        fds: Vec<PrivateData>,
    },
}

pub(crate) struct Cff {
    char_strings: Index,
    global_subrs: Index,
    fd: FdData,
}

impl Cff {
    pub fn parse(d: &[u8], (table_off, _len): (usize, usize)) -> Result<Cff, FontError> {
        let base = table_off;
        let hdr_size = read_u8_at(d, base + 2)? as usize;
        let name_index = Index::parse(d, base + hdr_size)?;
        let top_index = Index::parse(d, name_index.end)?;
        let string_index = Index::parse(d, top_index.end)?;
        let global_subrs = Index::parse(d, string_index.end)?;

        let (td_start, td_end) = top_index.get(d, 0)?;
        let top = parse_dict(d, td_start, td_end);

        let cs_off = dict_get(&top, 17)
            .and_then(|v| v.first().copied())
            .ok_or(FontError::Unsupported("CFF without CharStrings"))?
            as usize;
        let char_strings = Index::parse(d, base + cs_off)?;

        let fd = if let (Some(fda), Some(fds)) = (dict_get(&top, 1236), dict_get(&top, 1237)) {
            // CID-keyed: per-glyph FD selection.
            let fd_array = Index::parse(d, base + fda[0] as usize)?;
            let mut fds_vec = Vec::with_capacity(fd_array.count as usize);
            for i in 0..fd_array.count {
                let (s, e) = fd_array.get(d, i)?;
                let fd_dict = parse_dict(d, s, e);
                fds_vec.push(parse_private(d, base, &fd_dict)?);
            }
            FdData::Cid {
                fd_select: base + fds[0] as usize,
                fds: fds_vec,
            }
        } else {
            FdData::Single(parse_private(d, base, &top)?)
        };

        Ok(Cff {
            char_strings,
            global_subrs,
            fd,
        })
    }

    fn private_for(&self, d: &[u8], gid: u16) -> &PrivateData {
        match &self.fd {
            FdData::Single(p) => p,
            FdData::Cid { fd_select, fds } => {
                let fd = fd_select_lookup(d, *fd_select, gid).unwrap_or(0) as usize;
                fds.get(fd).unwrap_or(&fds[0])
            }
        }
    }
}

fn parse_private(
    d: &[u8],
    base: usize,
    dict: &[(u16, Vec<f64>)],
) -> Result<PrivateData, FontError> {
    let mut local_subrs = Index::default();
    if let Some(pv) = dict_get(dict, 18)
        && pv.len() == 2 {
            let (size, off) = (pv[0] as usize, base + pv[1] as usize);
            let private = parse_dict(d, off, off + size);
            if let Some(sub) = dict_get(&private, 19).and_then(|v| v.first()) {
                local_subrs = Index::parse(d, off + *sub as usize)?;
            }
        }
    Ok(PrivateData { local_subrs })
}

/// FDSelect formats 0 (array) and 3 (ranges).
fn fd_select_lookup(d: &[u8], sel: usize, gid: u16) -> Option<u8> {
    match read_u8_at(d, sel).ok()? {
        0 => read_u8_at(d, sel + 1 + gid as usize).ok(),
        3 => {
            let n_ranges = read_u16_at(d, sel + 1).ok()? as usize;
            let (mut lo, mut hi) = (0usize, n_ranges);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let rec = sel + 3 + mid * 3;
                let first = read_u16_at(d, rec).ok()?;
                let next = read_u16_at(d, rec + 3).ok()?;
                if next <= gid {
                    lo = mid + 1;
                } else if first > gid {
                    hi = mid;
                } else {
                    return read_u8_at(d, rec + 2).ok();
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::font::Font;
    use crate::raster;

    /// Integration test against a real system OTF (URW/Noto CFF). Skips
    /// quietly when the machine has none installed.
    #[test]
    fn parses_and_rasterizes_a_system_otf() {
        let candidates = [
            "/usr/share/fonts/urw-fonts/NimbusSans-Regular.otf",
            "/usr/share/fonts/urw-fonts/NimbusRoman-Regular.otf",
            "/usr/share/fonts/urw-fonts/C059-Roman.otf",
        ];
        let Some(data) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else {
            eprintln!("no system OTF found — skipping CFF integration test");
            return;
        };
        let font = Font::parse(data, 0).expect("CFF font should parse");
        let gid = font.glyph_index('A');
        assert_ne!(gid, 0, "cmap should map 'A'");
        let outline = font.outline(gid).expect("charstring should interpret");
        assert!(
            !outline.cmds.is_empty(),
            "outline should have path commands"
        );
        let glyph = raster::rasterize(&outline, 24.0 / 1000.0 * 24.0, 0.0)
            .expect("outline should rasterize");
        assert!(
            glyph.width > 4 && glyph.height > 4,
            "A should have real ink"
        );
        let ink: u32 = glyph.coverage.iter().map(|&c| c as u32).sum();
        assert!(ink > 1000, "coverage should be substantial, got {ink}");
    }
}
