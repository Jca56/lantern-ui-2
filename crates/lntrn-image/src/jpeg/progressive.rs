//! Progressive (SOF2) block decoding: DC first/refine and AC first/refine
//! scans with EOB runs. Coefficients accumulate across scans in the
//! component's block store; `al` is the successive-approximation shift.

use super::huffman::{BitReader, HuffTable, corrupt};
use super::idct::ZIGZAG;
use crate::ImageError;

pub(crate) fn dc_first(
    coef: &mut [i16],
    br: &mut BitReader,
    dc: &HuffTable,
    pred: &mut i32,
    al: u32,
) -> Result<(), ImageError> {
    let t = dc.decode(br)? as u32;
    if t > 16 {
        return Err(corrupt("bad DC size"));
    }
    *pred = pred.wrapping_add(br.receive_extend(t));
    coef[0] = pred.wrapping_shl(al) as i16;
    Ok(())
}

pub(crate) fn dc_refine(coef: &mut [i16], br: &mut BitReader, al: u32) {
    if br.bit() {
        coef[0] |= (1i32 << al) as i16;
    }
}

pub(crate) fn ac_first(
    coef: &mut [i16],
    br: &mut BitReader,
    ac: &HuffTable,
    (ss, se): (usize, usize),
    al: u32,
    eobrun: &mut u32,
) -> Result<(), ImageError> {
    if *eobrun > 0 {
        *eobrun -= 1;
        return Ok(());
    }
    let mut k = ss;
    while k <= se {
        let rs = ac.decode(br)?;
        let (r, s) = ((rs >> 4) as usize, (rs & 15) as u32);
        if s == 0 {
            if r < 15 {
                *eobrun = (1 << r) + br.bits(r as u32) - 1;
                break;
            }
            k += 16;
            continue;
        }
        k += r;
        if k > 63 {
            return Err(corrupt("AC run past end of block"));
        }
        coef[ZIGZAG[k]] = br.receive_extend(s).wrapping_shl(al) as i16;
        k += 1;
    }
    Ok(())
}

pub(crate) fn ac_refine(
    coef: &mut [i16],
    br: &mut BitReader,
    ac: &HuffTable,
    (ss, se): (usize, usize),
    al: u32,
    eobrun: &mut u32,
) -> Result<(), ImageError> {
    let bit = (1i32 << al) as i16;
    // Add one correction bit to an already-nonzero coefficient.
    let refine = |c: &mut i16, br: &mut BitReader| {
        if br.bit() && (*c & bit) == 0 {
            *c = if *c >= 0 { c.wrapping_add(bit) } else { c.wrapping_sub(bit) };
        }
    };
    if *eobrun > 0 {
        *eobrun -= 1;
        for &z in &ZIGZAG[ss..=se] {
            if coef[z] != 0 {
                refine(&mut coef[z], br);
            }
        }
        return Ok(());
    }
    let mut k = ss;
    while k <= se {
        let rs = ac.decode(br)?;
        let (mut r, s) = ((rs >> 4) as usize, rs & 15);
        let mut value = 0i16;
        if s == 0 {
            if r < 15 {
                *eobrun = (1 << r) + br.bits(r as u32);
                r = 64; // the rest of this band only takes correction bits
            }
        } else {
            if s != 1 {
                return Err(corrupt("bad AC refinement size"));
            }
            value = if br.bit() { bit } else { bit.wrapping_neg() };
        }
        while k <= se {
            let z = ZIGZAG[k];
            k += 1;
            if coef[z] != 0 {
                refine(&mut coef[z], br);
            } else if r == 0 {
                coef[z] = value;
                break;
            } else {
                r -= 1;
            }
        }
        if *eobrun > 0 {
            *eobrun -= 1;
            break;
        }
    }
    Ok(())
}
