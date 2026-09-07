//! SVG path data (`d="M0 0 L…"`) to polylines: every command including
//! arcs, relative and absolute, curves flattened.

use crate::geom::{Affine, P};

/// A number in path data, with the odd shapes `-.5.5` takes.
fn number(s: &[u8], i: &mut usize) -> Option<f32> {
    while *i < s.len() && (s[*i].is_ascii_whitespace() || s[*i] == b',') {
        *i += 1;
    }
    let start = *i;
    if *i < s.len() && (s[*i] == b'-' || s[*i] == b'+') {
        *i += 1;
    }
    let mut seen_dot = false;
    let mut digits = 0;
    while *i < s.len() {
        let c = s[*i];
        if c.is_ascii_digit() {
            digits += 1;
        } else if c == b'.' && !seen_dot {
            seen_dot = true;
        } else {
            break;
        }
        *i += 1;
    }
    if digits == 0 {
        *i = start;
        return None;
    }
    if *i < s.len() && (s[*i] == b'e' || s[*i] == b'E') {
        let save = *i;
        *i += 1;
        if *i < s.len() && (s[*i] == b'-' || s[*i] == b'+') {
            *i += 1;
        }
        let d0 = *i;
        while *i < s.len() && s[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i == d0 {
            *i = save;
        }
    }
    std::str::from_utf8(&s[start..*i]).ok()?.parse().ok()
}

/// An arc flag: one digit, spaces or commas before it.
fn flag(s: &[u8], i: &mut usize) -> Option<bool> {
    while *i < s.len() && (s[*i].is_ascii_whitespace() || s[*i] == b',') {
        *i += 1;
    }
    let c = *s.get(*i)?;
    if c == b'0' || c == b'1' {
        *i += 1;
        Some(c == b'1')
    } else {
        None
    }
}

/// Segments per curve when flattening; icons are small.
const CURVE_STEPS: usize = 12;

/// Every subpath of `d` as a closed polyline (an open one is closed for
/// filling), transformed by `t`.
pub fn flatten(d: &str, t: &Affine) -> Vec<Vec<P>> {
    let s = d.as_bytes();
    let mut i = 0;
    let mut out: Vec<Vec<P>> = Vec::new();
    let mut cur: Vec<P> = Vec::new();
    let mut pos = P::ZERO;
    let mut start = P::ZERO;
    let mut last_ctrl: Option<P> = None;
    let mut last_qctrl: Option<P> = None;
    let mut cmd = b'M';
    let push_poly = |cur: &mut Vec<P>, out: &mut Vec<Vec<P>>| {
        if cur.len() >= 2 {
            out.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    loop {
        while i < s.len() && (s[i].is_ascii_whitespace() || s[i] == b',') {
            i += 1;
        }
        if i >= s.len() {
            break;
        }
        if s[i].is_ascii_alphabetic() {
            cmd = s[i];
            i += 1;
        } else if cmd == b'Z' || cmd == b'z' {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { pos } else { P::ZERO };
        let mut keep_ctrl = false;
        let mut keep_qctrl = false;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (Some(x), Some(y)) = (number(s, &mut i), number(s, &mut i)) else { break };
                push_poly(&mut cur, &mut out);
                pos = P::new(base.x + x, base.y + y);
                start = pos;
                cur.push(t.apply(pos));
                // Further pairs are line-tos.
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let (Some(x), Some(y)) = (number(s, &mut i), number(s, &mut i)) else { break };
                pos = P::new(base.x + x, base.y + y);
                cur.push(t.apply(pos));
            }
            b'H' => {
                let Some(x) = number(s, &mut i) else { break };
                pos = P::new(base.x + x, pos.y);
                cur.push(t.apply(pos));
            }
            b'V' => {
                let Some(y) = number(s, &mut i) else { break };
                pos = P::new(pos.x, base.y + y);
                cur.push(t.apply(pos));
            }
            b'C' | b'S' => {
                let c1 = if cmd.to_ascii_uppercase() == b'C' {
                    let (Some(x), Some(y)) = (number(s, &mut i), number(s, &mut i)) else { break };
                    P::new(base.x + x, base.y + y)
                } else {
                    last_ctrl.map_or(pos, |c| pos.reflect(c))
                };
                let (Some(x2), Some(y2), Some(x), Some(y)) = (number(s, &mut i), number(s, &mut i), number(s, &mut i), number(s, &mut i)) else { break };
                let c2 = P::new(base.x + x2, base.y + y2);
                let end = P::new(base.x + x, base.y + y);
                cubic(&mut cur, t, pos, c1, c2, end);
                last_ctrl = Some(c2);
                keep_ctrl = true;
                pos = end;
            }
            b'Q' | b'T' => {
                let c = if cmd.to_ascii_uppercase() == b'Q' {
                    let (Some(x), Some(y)) = (number(s, &mut i), number(s, &mut i)) else { break };
                    P::new(base.x + x, base.y + y)
                } else {
                    last_qctrl.map_or(pos, |c| pos.reflect(c))
                };
                let (Some(x), Some(y)) = (number(s, &mut i), number(s, &mut i)) else { break };
                let end = P::new(base.x + x, base.y + y);
                quad(&mut cur, t, pos, c, end);
                last_qctrl = Some(c);
                keep_qctrl = true;
                pos = end;
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rot)) = (number(s, &mut i), number(s, &mut i), number(s, &mut i)) else { break };
                let (Some(large), Some(sweep)) = (flag(s, &mut i), flag(s, &mut i)) else { break };
                let (Some(x), Some(y)) = (number(s, &mut i), number(s, &mut i)) else { break };
                let end = P::new(base.x + x, base.y + y);
                arc(&mut cur, t, pos, rx, ry, rot, large, sweep, end);
                pos = end;
            }
            b'Z' => {
                pos = start;
                push_poly(&mut cur, &mut out);
                cur.push(t.apply(pos));
                cur.clear();
            }
            _ => break,
        }
        if !keep_ctrl {
            last_ctrl = None;
        }
        if !keep_qctrl {
            last_qctrl = None;
        }
    }
    push_poly(&mut cur, &mut out);
    out
}

fn cubic(cur: &mut Vec<P>, t: &Affine, p0: P, p1: P, p2: P, p3: P) {
    for k in 1..=CURVE_STEPS {
        let u = k as f32 / CURVE_STEPS as f32;
        let v = 1.0 - u;
        let x = v * v * v * p0.x + 3.0 * v * v * u * p1.x + 3.0 * v * u * u * p2.x + u * u * u * p3.x;
        let y = v * v * v * p0.y + 3.0 * v * v * u * p1.y + 3.0 * v * u * u * p2.y + u * u * u * p3.y;
        cur.push(t.apply(P::new(x, y)));
    }
}

fn quad(cur: &mut Vec<P>, t: &Affine, p0: P, p1: P, p2: P) {
    for k in 1..=CURVE_STEPS {
        let u = k as f32 / CURVE_STEPS as f32;
        let v = 1.0 - u;
        let x = v * v * p0.x + 2.0 * v * u * p1.x + u * u * p2.x;
        let y = v * v * p0.y + 2.0 * v * u * p1.y + u * u * p2.y;
        cur.push(t.apply(P::new(x, y)));
    }
}

/// An elliptical arc from the endpoint form, as the SVG spec converts it.
#[allow(clippy::too_many_arguments)]
fn arc(cur: &mut Vec<P>, t: &Affine, from: P, rx: f32, ry: f32, rot_deg: f32, large: bool, sweep: bool, to: P) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-6 || ry < 1e-6 || (from.x - to.x).abs() < 1e-6 && (from.y - to.y).abs() < 1e-6 {
        cur.push(t.apply(to));
        return;
    }
    let phi = rot_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (from.x - to.x) * 0.5;
    let dy = (from.y - to.y) * 0.5;
    let x1 = cos * dx + sin * dy;
    let y1 = -sin * dx + cos * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = if den > 0.0 { (num / den).sqrt() } else { 0.0 };
    if large == sweep {
        coef = -coef;
    }
    let cx1 = coef * rx * y1 / ry;
    let cy1 = -coef * ry * x1 / rx;
    let cx = cos * cx1 - sin * cy1 + (from.x + to.x) * 0.5;
    let cy = sin * cx1 + cos * cy1 + (from.y + to.y) * 0.5;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = angle((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry);
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }
    let steps = ((delta.abs() / std::f32::consts::FRAC_PI_2).ceil() as usize * 6).max(4);
    for k in 1..=steps {
        let a = theta1 + delta * k as f32 / steps as f32;
        let (sa, ca) = a.sin_cos();
        let x = cos * rx * ca - sin * ry * sa + cx;
        let y = sin * rx * ca + cos * ry * sa + cy;
        cur.push(t.apply(P::new(x, y)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_odd_numbers_and_commands() {
        let polys = flatten("M0 0h10v10H0z", &Affine::IDENTITY);
        assert_eq!(polys.len(), 1);
        assert_eq!(polys[0], vec![P::new(0.0, 0.0), P::new(10.0, 0.0), P::new(10.0, 10.0), P::new(0.0, 10.0)]);
        let p = flatten("m1-.5.5.5", &Affine::IDENTITY);
        assert_eq!(p[0], vec![P::new(1.0, -0.5), P::new(1.5, 0.0)]);
        let arc = flatten("M0 0a5 5 0 0 1 10 0", &Affine::IDENTITY);
        let last = *arc[0].last().unwrap();
        assert!((last.x - 10.0).abs() < 1e-3 && last.y.abs() < 1e-3, "arc lands on its end: {last:?}");
        assert!(arc[0].iter().any(|p| p.y < -4.0), "the arc bulges");
        let two = flatten("M0 0L1 0 1 1M5 5l1 0", &Affine::IDENTITY);
        assert_eq!(two.len(), 2, "implicit line-tos after M, then a second subpath");
    }
}
