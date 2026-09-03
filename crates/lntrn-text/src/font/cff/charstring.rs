//! The Type2 charstring interpreter: turns a glyph's program (and the
//! subroutines it calls) into a cubic-bézier [`Outline`]. Hints are
//! skipped, the flex family is flattened to curves, and `seac`-style
//! accents via 4-argument `endchar` are logged and skipped.

use super::super::FontError;
use super::super::sfnt::{read_u16_at, read_u32_at, read_u8_at};
use super::{Cff, Index, MAX_STACK, MAX_SUBR_DEPTH};
use crate::raster::outline::{Outline, PathCmd};

/// Subroutine-number bias per the Type2 spec.
fn bias(count: u32) -> i32 {
    if count < 1240 {
        107
    } else if count < 33900 {
        1131
    } else {
        32768
    }
}

// ── Type2 charstring interpreter ────────────────────────────────────────────

struct Interp<'a> {
    d: &'a [u8],
    global: Index,
    local: Index,
    stack: Vec<f64>,
    x: f64,
    y: f64,
    n_stems: usize,
    width_done: bool,
    open: bool,
    out: &'a mut Outline,
}

pub(crate) fn outline(d: &[u8], cff: &Cff, gid: u16, out: &mut Outline) -> Result<(), FontError> {
    let (start, end) = cff.char_strings.get(d, gid as u32)?;
    let private = cff.private_for(d, gid);
    let mut interp = Interp {
        d,
        global: cff.global_subrs,
        local: private.local_subrs,
        stack: Vec::with_capacity(MAX_STACK),
        x: 0.0,
        y: 0.0,
        n_stems: 0,
        width_done: false,
        open: false,
        out,
    };
    interp.run(start, end, 0)
}

impl Interp<'_> {
    fn run(&mut self, mut pos: usize, end: usize, depth: u8) -> Result<(), FontError> {
        if depth > MAX_SUBR_DEPTH {
            return Err(FontError::Unsupported("charstring subrs nested too deep"));
        }
        while pos < end {
            let b0 = read_u8_at(self.d, pos)?;
            match b0 {
                // ── Operands ──
                32..=246 => {
                    self.push(b0 as f64 - 139.0)?;
                    pos += 1;
                }
                247..=250 => {
                    let b1 = read_u8_at(self.d, pos + 1)?;
                    self.push((b0 as f64 - 247.0) * 256.0 + b1 as f64 + 108.0)?;
                    pos += 2;
                }
                251..=254 => {
                    let b1 = read_u8_at(self.d, pos + 1)?;
                    self.push(-(b0 as f64 - 251.0) * 256.0 - b1 as f64 - 108.0)?;
                    pos += 2;
                }
                28 => {
                    self.push(read_u16_at(self.d, pos + 1)? as i16 as f64)?;
                    pos += 3;
                }
                255 => {
                    self.push(read_u32_at(self.d, pos + 1)? as i32 as f64 / 65536.0)?;
                    pos += 5;
                }
                // ── Hints (collected + skipped) ──
                1 | 3 | 18 | 23 => {
                    self.take_width_if(self.stack.len() % 2 == 1);
                    self.n_stems += self.stack.len() / 2;
                    self.stack.clear();
                    pos += 1;
                }
                19 | 20 => {
                    // hintmask/cntrmask: implicit vstems from pending args.
                    self.take_width_if(self.stack.len() % 2 == 1);
                    self.n_stems += self.stack.len() / 2;
                    self.stack.clear();
                    pos += 1 + self.n_stems.div_ceil(8);
                }
                // ── Path construction ──
                21 => {
                    // rmoveto (take_width_if strips a leading width first)
                    self.take_width_if(self.stack.len() > 2);
                    let dx = self.stack.first().copied().unwrap_or(0.0);
                    let dy = self.stack.get(1).copied().unwrap_or(0.0);
                    self.move_to(dx, dy);
                    self.stack.clear();
                    pos += 1;
                }
                22 => {
                    // hmoveto
                    self.take_width_if(self.stack.len() > 1);
                    let dx = self.stack.first().copied().unwrap_or(0.0);
                    self.move_to(dx, 0.0);
                    self.stack.clear();
                    pos += 1;
                }
                4 => {
                    // vmoveto
                    self.take_width_if(self.stack.len() > 1);
                    let dy = self.stack.first().copied().unwrap_or(0.0);
                    self.move_to(0.0, dy);
                    self.stack.clear();
                    pos += 1;
                }
                5 => {
                    // rlineto
                    let mut i = 0;
                    while i + 1 < self.stack.len() {
                        self.line_to(self.stack[i], self.stack[i + 1]);
                        i += 2;
                    }
                    self.stack.clear();
                    pos += 1;
                }
                6 | 7 => {
                    // hlineto / vlineto: alternating axes.
                    let mut horizontal = b0 == 6;
                    for i in 0..self.stack.len() {
                        let v = self.stack[i];
                        if horizontal {
                            self.line_to(v, 0.0);
                        } else {
                            self.line_to(0.0, v);
                        }
                        horizontal = !horizontal;
                    }
                    self.stack.clear();
                    pos += 1;
                }
                8 => {
                    // rrcurveto
                    let mut i = 0;
                    while i + 5 < self.stack.len() {
                        self.curve(&[
                            self.stack[i],
                            self.stack[i + 1],
                            self.stack[i + 2],
                            self.stack[i + 3],
                            self.stack[i + 4],
                            self.stack[i + 5],
                        ]);
                        i += 6;
                    }
                    self.stack.clear();
                    pos += 1;
                }
                24 => {
                    // rcurveline: curves then one line.
                    let n = self.stack.len();
                    let mut i = 0;
                    while n - i >= 8 {
                        self.curve(&[
                            self.stack[i],
                            self.stack[i + 1],
                            self.stack[i + 2],
                            self.stack[i + 3],
                            self.stack[i + 4],
                            self.stack[i + 5],
                        ]);
                        i += 6;
                    }
                    if i + 1 < n {
                        self.line_to(self.stack[i], self.stack[i + 1]);
                    }
                    self.stack.clear();
                    pos += 1;
                }
                25 => {
                    // rlinecurve: lines then one curve.
                    let n = self.stack.len();
                    let mut i = 0;
                    while n - i >= 8 {
                        self.line_to(self.stack[i], self.stack[i + 1]);
                        i += 2;
                    }
                    if i + 5 < n {
                        self.curve(&[
                            self.stack[i],
                            self.stack[i + 1],
                            self.stack[i + 2],
                            self.stack[i + 3],
                            self.stack[i + 4],
                            self.stack[i + 5],
                        ]);
                    }
                    self.stack.clear();
                    pos += 1;
                }
                26 | 27 => {
                    // vvcurveto / hhcurveto: optional leading cross-axis delta.
                    let mut i = 0;
                    let mut d1 = 0.0;
                    if self.stack.len() % 4 == 1 {
                        d1 = self.stack[0];
                        i = 1;
                    }
                    while i + 3 < self.stack.len() {
                        let (a, b, c, dd) = (
                            self.stack[i],
                            self.stack[i + 1],
                            self.stack[i + 2],
                            self.stack[i + 3],
                        );
                        if b0 == 26 {
                            self.curve(&[d1, a, b, c, 0.0, dd]);
                        } else {
                            self.curve(&[a, d1, b, c, dd, 0.0]);
                        }
                        d1 = 0.0;
                        i += 4;
                    }
                    self.stack.clear();
                    pos += 1;
                }
                30 | 31 => {
                    // vhcurveto / hvcurveto: alternating start axis, optional
                    // trailing cross-axis delta on the last curve.
                    let mut horizontal = b0 == 31;
                    let n = self.stack.len();
                    let mut i = 0;
                    while n - i >= 4 {
                        let last = n - i < 8;
                        let extra = if last && n - i == 5 {
                            self.stack[n - 1]
                        } else {
                            0.0
                        };
                        let (a, b, c, dd) = (
                            self.stack[i],
                            self.stack[i + 1],
                            self.stack[i + 2],
                            self.stack[i + 3],
                        );
                        if horizontal {
                            self.curve(&[a, 0.0, b, c, extra, dd]);
                        } else {
                            self.curve(&[0.0, a, b, c, dd, extra]);
                        }
                        horizontal = !horizontal;
                        i += 4;
                    }
                    self.stack.clear();
                    pos += 1;
                }
                // ── Subroutines ──
                10 | 29 => {
                    let index = if b0 == 10 { self.local } else { self.global };
                    let Some(num) = self.stack.pop() else {
                        pos += 1;
                        continue;
                    };
                    let idx = num as i32 + bias(index.count);
                    if idx >= 0 {
                        let (s, e) = index.get(self.d, idx as u32)?;
                        self.run(s, e, depth + 1)?;
                    }
                    pos += 1;
                }
                11 => return Ok(()), // return
                14 => {
                    // endchar (seac-style accents unsupported; logged).
                    self.take_width_if(self.stack.len() == 1 || self.stack.len() == 5);
                    if self.stack.len() >= 4 {
                        lntrn_core::log_warn!("CFF seac accent composition skipped");
                    }
                    self.stack.clear();
                    return Ok(());
                }
                12 => {
                    let b1 = read_u8_at(self.d, pos + 1)?;
                    self.flex(b1);
                    pos += 2;
                }
                _ => {
                    // Reserved/arithmetic ops we don't need: clear and go on.
                    self.stack.clear();
                    pos += 1;
                }
            }
        }
        Ok(())
    }

    fn push(&mut self, v: f64) -> Result<(), FontError> {
        if self.stack.len() >= MAX_STACK {
            return Err(FontError::Unsupported("charstring stack overflow"));
        }
        self.stack.push(v);
        Ok(())
    }

    fn take_width_if(&mut self, condition: bool) {
        if !self.width_done && condition && !self.stack.is_empty() {
            self.stack.remove(0);
        }
        self.width_done = true;
    }

    fn move_to(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        self.out
            .cmds
            .push(PathCmd::Move([self.x as f32, self.y as f32]));
        self.open = true;
    }

    fn line_to(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        self.out
            .cmds
            .push(PathCmd::Line([self.x as f32, self.y as f32]));
    }

    /// One cubic from six deltas: c1(dx,dy), c2(dx,dy), end(dx,dy).
    fn curve(&mut self, d6: &[f64; 6]) {
        let (c1x, c1y) = (self.x + d6[0], self.y + d6[1]);
        let (c2x, c2y) = (c1x + d6[2], c1y + d6[3]);
        self.x = c2x + d6[4];
        self.y = c2y + d6[5];
        self.out.cmds.push(PathCmd::Cubic(
            [c1x as f32, c1y as f32],
            [c2x as f32, c2y as f32],
            [self.x as f32, self.y as f32],
        ));
    }

    /// The flex family (12 34/35/36/37): two cubics with various implied
    /// components; the flex-height argument is ignored (we always curve).
    fn flex(&mut self, op: u8) {
        let s = std::mem::take(&mut self.stack);
        let y0 = self.y;
        match (op, s.len()) {
            (35, 13) => {
                // flex: two full cubics + fd.
                self.curve(&[s[0], s[1], s[2], s[3], s[4], s[5]]);
                self.curve(&[s[6], s[7], s[8], s[9], s[10], s[11]]);
            }
            (34, 7) => {
                // hflex: horizontal, c1y = 0, returns to start y.
                self.curve(&[s[0], 0.0, s[1], s[2], s[3], 0.0]);
                let back = y0 - self.y;
                self.curve(&[s[4], 0.0, s[5], back, s[6], 0.0]);
            }
            (36, 9) => {
                // hflex1: hflex with explicit dy on the outer control points;
                // the end returns to the starting y.
                self.curve(&[s[0], s[1], s[2], s[3], s[4], 0.0]);
                self.curve(&[s[5], 0.0, s[6], s[7], s[8], y0 - self.y - s[7]]);
            }
            (37, 11) => {
                // flex1: the end snaps back to the start's x or y (whichever
                // axis moved less across the five explicit deltas).
                let x0 = self.x;
                let dx = s[0] + s[2] + s[4] + s[6] + s[8];
                let dy = s[1] + s[3] + s[5] + s[7] + s[9];
                self.curve(&[s[0], s[1], s[2], s[3], s[4], s[5]]);
                let (ex, ey);
                if dx.abs() > dy.abs() {
                    ex = s[10];
                    ey = y0 - self.y - s[7] - s[9];
                } else {
                    ex = x0 - self.x - s[6] - s[8];
                    ey = s[10];
                }
                self.curve(&[s[6], s[7], s[8], s[9], ex, ey]);
            }
            _ => {}
        }
    }
}
