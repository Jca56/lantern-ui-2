//! One SOS segment: parse the scan header, then walk MCUs (interleaved or
//! single-component), honouring restart intervals, into the coefficient
//! store. Baseline blocks are decoded here; progressive ones defer to
//! `progressive`.

use super::huffman::{BitReader, HuffTable, corrupt};
use super::idct::ZIGZAG;
use super::progressive;
use super::{Component, Decoder, Frame};
use crate::ImageError;

struct Scan {
    /// Frame component indices in scan order.
    comps: Vec<usize>,
    ss: usize,
    se: usize,
    ah: u32,
    al: u32,
}

/// Decode the scan whose header is `body` and whose entropy-coded data
/// starts at `pos`. Returns where the marker parser resumes.
pub(crate) fn decode_scan(dec: &mut Decoder, body: &[u8], pos: usize) -> Result<usize, ImageError> {
    let Decoder { data, quant, dc_tables, ac_tables, frame, restart_interval, scans_done, .. } = dec;
    let tables = (&*dc_tables, &*ac_tables);
    let frame = frame.as_mut().ok_or_else(|| corrupt("SOS before SOF"))?;
    let scan = parse_header(frame, body)?;
    for &ci in &scan.comps {
        let c = &mut frame.components[ci];
        if c.quant.is_none() {
            c.quant = Some(quant[c.tq].ok_or_else(|| corrupt("missing quantisation table"))?);
        }
        c.dc_pred = 0;
    }

    let interleaved = scan.comps.len() > 1;
    let (mcus_x, mcus_y) = if interleaved {
        (frame.mcus_x, frame.mcus_y)
    } else {
        let c = &frame.components[scan.comps[0]];
        (c.width(frame).div_ceil(8), c.height(frame).div_ceil(8))
    };
    let mut br = BitReader::new(data, pos);
    let mut eobrun = 0u32;
    for m in 0..mcus_x * mcus_y {
        if *restart_interval > 0 && m > 0 && m % *restart_interval == 0 {
            br.restart();
            eobrun = 0;
            for &ci in &scan.comps {
                frame.components[ci].dc_pred = 0;
            }
        }
        let (mx, my) = (m % mcus_x, m / mcus_x);
        for &ci in &scan.comps {
            let c = &mut frame.components[ci];
            let (bw, bh) = if interleaved { (c.h, c.v) } else { (1, 1) };
            for by in 0..bh {
                for bx in 0..bw {
                    let block = (my * bh + by) * c.blocks_w + mx * bw + bx;
                    decode_block(tables, frame.progressive, &scan, c, block, &mut br, &mut eobrun)?;
                }
            }
        }
        if br.truncated {
            return Err(corrupt("truncated scan data"));
        }
    }
    *scans_done += 1;
    Ok(br.end_pos())
}

fn parse_header(frame: &mut Frame, body: &[u8]) -> Result<Scan, ImageError> {
    let ns = *body.first().ok_or_else(|| corrupt("SOS header"))? as usize;
    if ns == 0 || ns > 4 || body.len() < 4 + ns * 2 {
        return Err(corrupt("SOS header"));
    }
    let mut comps = Vec::with_capacity(ns);
    for i in 0..ns {
        let (id, tables) = (body[1 + i * 2], body[2 + i * 2]);
        let ci = frame
            .components
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| corrupt("scan names an unknown component"))?;
        if comps.contains(&ci) {
            return Err(corrupt("component repeated in scan"));
        }
        let c = &mut frame.components[ci];
        c.dc_table = (tables >> 4) as usize;
        c.ac_table = (tables & 15) as usize;
        if c.dc_table > 3 || c.ac_table > 3 {
            return Err(corrupt("bad Huffman table selector"));
        }
        comps.push(ci);
    }
    let (ss, se) = (body[1 + ns * 2] as usize, body[2 + ns * 2] as usize);
    let (ah, al) = ((body[3 + ns * 2] >> 4) as u32, (body[3 + ns * 2] & 15) as u32);
    let ok = if frame.progressive {
        (ss == 0 && se == 0 || ss > 0 && ss <= se && se <= 63 && ns == 1) && ah <= 13 && al <= 13
    } else {
        ss == 0 && se == 63 && ah == 0 && al == 0
    };
    if !ok {
        return Err(corrupt("bad spectral selection / approximation"));
    }
    Ok(Scan { comps, ss, se, ah, al })
}

type Tables<'t> = (&'t [Option<HuffTable>; 4], &'t [Option<HuffTable>; 4]);

fn decode_block(
    tables: Tables,
    progressive: bool,
    scan: &Scan,
    c: &mut Component,
    block: usize,
    br: &mut BitReader,
    eobrun: &mut u32,
) -> Result<(), ImageError> {
    let coef = c
        .coefs
        .get_mut(block * 64..block * 64 + 64)
        .ok_or_else(|| corrupt("block outside the image"))?;
    let dc = || tables.0[c.dc_table].as_ref().ok_or_else(|| corrupt("missing DC table"));
    let ac = || tables.1[c.ac_table].as_ref().ok_or_else(|| corrupt("missing AC table"));
    if !progressive {
        return baseline_block(coef, br, dc()?, ac()?, &mut c.dc_pred);
    }
    let band = (scan.ss, scan.se);
    match (scan.ss == 0, scan.ah == 0) {
        (true, true) => progressive::dc_first(coef, br, dc()?, &mut c.dc_pred, scan.al),
        (true, false) => {
            progressive::dc_refine(coef, br, scan.al);
            Ok(())
        }
        (false, true) => progressive::ac_first(coef, br, ac()?, band, scan.al, eobrun),
        (false, false) => progressive::ac_refine(coef, br, ac()?, band, scan.al, eobrun),
    }
}

fn baseline_block(
    coef: &mut [i16],
    br: &mut BitReader,
    dc: &HuffTable,
    ac: &HuffTable,
    pred: &mut i32,
) -> Result<(), ImageError> {
    let t = dc.decode(br)? as u32;
    if t > 16 {
        return Err(corrupt("bad DC size"));
    }
    *pred = pred.wrapping_add(br.receive_extend(t));
    coef[0] = *pred as i16;
    let mut k = 1;
    while k < 64 {
        let rs = ac.decode(br)?;
        let (r, s) = ((rs >> 4) as usize, (rs & 15) as u32);
        if s == 0 {
            if r == 15 {
                k += 16;
                continue;
            }
            break;
        }
        k += r;
        if k > 63 {
            return Err(corrupt("AC run past end of block"));
        }
        coef[ZIGZAG[k]] = br.receive_extend(s) as i16;
        k += 1;
    }
    Ok(())
}
