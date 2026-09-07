//! Filling polygons into an RGBA bitmap: scanlines with sub-samples down
//! the pixel and exact coverage across it, either fill rule, blended
//! over what is there.

use crate::geom::P;

/// Sub-scanlines per pixel row.
const SUB: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    coverage: Vec<f32>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, rgba: vec![0; width * height * 4], coverage: vec![0.0; width * height] }
    }

    /// Fill `polys` with `color` (straight alpha, `alpha` multiplied in).
    pub fn fill(&mut self, polys: &[Vec<P>], rule: FillRule, color: [u8; 4], alpha: f32) {
        let (w, h) = (self.width, self.height);
        self.coverage.iter_mut().for_each(|c| *c = 0.0);
        let mut edges: Vec<(P, P, i32)> = Vec::new();
        for poly in polys {
            let n = poly.len();
            for k in 0..n {
                let (a, b) = (poly[k], poly[(k + 1) % n]);
                if (a.y - b.y).abs() < 1e-9 {
                    continue;
                }
                if a.y < b.y { edges.push((a, b, 1)) } else { edges.push((b, a, -1)) }
            }
        }
        if edges.is_empty() {
            return;
        }
        let y_min = edges.iter().map(|e| e.0.y).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let y_max = (edges.iter().map(|e| e.1.y).fold(f32::MIN, f32::max).ceil().min(h as f32)) as usize;
        let mut xs: Vec<(f32, i32)> = Vec::new();
        let weight = 1.0 / SUB as f32;
        for row in y_min..y_max {
            for s in 0..SUB {
                let y = row as f32 + (s as f32 + 0.5) * weight;
                xs.clear();
                for (a, b, dir) in &edges {
                    if y >= a.y && y < b.y {
                        let x = a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y);
                        xs.push((x, *dir));
                    }
                }
                if xs.len() < 2 {
                    continue;
                }
                xs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
                let mut wind = 0;
                for k in 0..xs.len() - 1 {
                    wind += if rule == FillRule::NonZero { xs[k].1 } else { 1 };
                    let inside = match rule {
                        FillRule::NonZero => wind != 0,
                        FillRule::EvenOdd => wind % 2 == 1,
                    };
                    if inside {
                        self.span(row, xs[k].0, xs[k + 1].0, weight);
                    }
                }
            }
        }
        let a = alpha.clamp(0.0, 1.0) * color[3] as f32 / 255.0;
        for i in 0..w * h {
            let c = self.coverage[i].min(1.0) * a;
            if c <= 0.0 {
                continue;
            }
            let px = &mut self.rgba[i * 4..i * 4 + 4];
            let da = px[3] as f32 / 255.0;
            let out_a = c + da * (1.0 - c);
            for k in 0..3 {
                let s = color[k] as f32 / 255.0;
                let d = px[k] as f32 / 255.0;
                let v = if out_a > 0.0 { (s * c + d * da * (1.0 - c)) / out_a } else { 0.0 };
                px[k] = (v * 255.0).round() as u8;
            }
            px[3] = (out_a * 255.0).round() as u8;
        }
    }

    /// Add `weight` coverage to `[x0, x1)` on `row`, fractionally at the ends.
    fn span(&mut self, row: usize, x0: f32, x1: f32, weight: f32) {
        let w = self.width as f32;
        let (x0, x1) = (x0.max(0.0), x1.min(w));
        if x1 <= x0 {
            return;
        }
        let base = row * self.width;
        let (i0, i1) = (x0.floor() as usize, (x1.ceil() as usize).min(self.width));
        for i in i0..i1 {
            let (l, r) = (i as f32, i as f32 + 1.0);
            let overlap = (x1.min(r) - x0.max(l)).max(0.0);
            self.coverage[base + i] += overlap * weight;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_a_square_with_soft_edges() {
        let mut c = Canvas::new(8, 8);
        let sq = vec![vec![P::new(2.0, 2.0), P::new(6.0, 2.0), P::new(6.0, 6.0), P::new(2.0, 6.0)]];
        c.fill(&sq, FillRule::NonZero, [255, 0, 0, 255], 1.0);
        let px = |x: usize, y: usize| c.rgba[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].to_vec();
        assert_eq!(px(3, 3), vec![255, 0, 0, 255], "inside");
        assert_eq!(px(0, 0), vec![0, 0, 0, 0], "outside");
        let half = vec![vec![P::new(2.5, 2.0), P::new(6.0, 2.0), P::new(6.0, 6.0), P::new(2.5, 6.0)]];
        let mut c2 = Canvas::new(8, 8);
        c2.fill(&half, FillRule::NonZero, [255, 0, 0, 255], 1.0);
        let a = c2.rgba[(3 * 8 + 2) * 4 + 3];
        assert!((120..=136).contains(&a), "half-covered edge pixel: {a}");
        // Even-odd: a square inside a square leaves a hole.
        let ring = vec![sq[0].clone(), vec![P::new(3.0, 3.0), P::new(5.0, 3.0), P::new(5.0, 5.0), P::new(3.0, 5.0)]];
        let mut c3 = Canvas::new(8, 8);
        c3.fill(&ring, FillRule::EvenOdd, [0, 255, 0, 255], 1.0);
        assert_eq!(c3.rgba[(4 * 8 + 4) * 4 + 3], 0, "hole");
        assert_eq!(c3.rgba[(2 * 8 + 2) * 4 + 3], 255, "ring");
    }
}
