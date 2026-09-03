//! More things a [`DrawList`] can draw: two-axis gradients, circles and
//! rings, arcs and wedges, Bézier curves, and pictures.

use lntrn_math::{Color, Rect, Vec2};

use crate::draw2d::{DrawList, MODE_IMAGE, MODE_PLAIN};
use crate::images::ImageHandle;

impl DrawList {
    /// Hard-edged rectangle with a colour per corner (top-left, top-right,
    /// bottom-right, bottom-left): two-axis gradients, colour pickers.
    pub fn rect_gradient4(&mut self, r: Rect, tl: Color, tr: Color, br: Color, bl: Color) {
        if r.is_empty() {
            return;
        }
        let c = r.center();
        let h = r.size() * 0.5;
        let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)];
        let col = |k: Color| k.to_linear().to_gpu();
        self.push_quad_colors(corners, [[0.0; 2]; 4], [col(tl), col(tr), col(br), col(bl)], [c.x as f32, c.y as f32, h.x as f32, h.y as f32], [0.0, MODE_PLAIN, 0.0, 0.0]);
    }

    /// Hard-edged rectangle shaded from `left` to `right`.
    pub fn rect_gradient_h(&mut self, r: Rect, left: Color, right: Color) {
        self.rect_gradient4(r, left, right, right, left);
    }

    /// Anti-aliased filled circle.
    pub fn circle(&mut self, center: Vec2, radius: f64, color: Color) {
        self.rounded_rect(Rect::from_center_size(center, Vec2::splat(radius * 2.0)), radius, color);
    }

    /// Filled circle shaded from `top` to `bottom`.
    pub fn circle_gradient(&mut self, center: Vec2, radius: f64, top: Color, bottom: Color) {
        self.rounded_rect_gradient(Rect::from_center_size(center, Vec2::splat(radius * 2.0)), radius, top, bottom);
    }

    /// Anti-aliased ring: an inner stroke of `width` along a circle's edge.
    pub fn ring(&mut self, center: Vec2, radius: f64, width: f64, color: Color) {
        self.stroke_rect(Rect::from_center_size(center, Vec2::splat(radius * 2.0)), width, radius, color);
    }

    /// Points along a circle from angle `a0` to `a1` (radians; 0 is right,
    /// π/2 is down on screen), enough of them for a smooth curve.
    fn arc_points(center: Vec2, radius: f64, a0: f64, a1: f64) -> Vec<Vec2> {
        let sweep = a1 - a0;
        let n = ((sweep.abs() * radius / 3.0).ceil() as usize).clamp(3, 256);
        (0..=n).map(|i| center + Vec2::from_angle(a0 + sweep * i as f64 / n as f64) * radius).collect()
    }

    /// Stroked arc of `width` from angle `a0` to `a1`.
    pub fn arc(&mut self, center: Vec2, radius: f64, a0: f64, a1: f64, width: f64, color: Color) {
        if radius <= 0.0 || a0 == a1 {
            return;
        }
        let pts = Self::arc_points(center, radius, a0, a1);
        self.polyline(&pts, width, color, false);
    }

    /// Filled wedge from angle `a0` to `a1`.
    pub fn pie(&mut self, center: Vec2, radius: f64, a0: f64, a1: f64, color: Color) {
        if radius <= 0.0 || a0 == a1 {
            return;
        }
        let pts = Self::arc_points(center, radius, a0, a1);
        for w in pts.windows(2) {
            self.triangle(center, w[0], w[1], color);
        }
    }

    /// Cubic Bézier stroke of `width`, flattened to a polyline.
    pub fn bezier(&mut self, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, width: f64, color: Color) {
        let approx = (p1 - p0).length() + (p2 - p1).length() + (p3 - p2).length();
        let n = ((approx / 4.0).ceil() as usize).clamp(4, 200);
        let pts: Vec<Vec2> = (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                let u = 1.0 - t;
                p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
            })
            .collect();
        self.polyline(&pts, width, color, false);
    }

    /// Quadratic Bézier stroke of `width`.
    pub fn quad_bezier(&mut self, p0: Vec2, p1: Vec2, p2: Vec2, width: f64, color: Color) {
        let c1 = p0 + (p1 - p0) * (2.0 / 3.0);
        let c2 = p2 + (p1 - p2) * (2.0 / 3.0);
        self.bezier(p0, c1, c2, p2, width, color);
    }

    /// Draw `image` stretched over `r`, tinted by `tint` (white for as is),
    /// with corners rounded by `radius`.
    pub fn image(&mut self, r: Rect, image: ImageHandle, radius: f64, tint: Color) {
        self.image_uv(r, image, Rect::from_xywh(0.0, 0.0, 1.0, 1.0), radius, tint);
    }

    /// Draw the `uv` part of `image` (`0..1` on both axes) over `r`.
    pub fn image_uv(&mut self, r: Rect, image: ImageHandle, uv: Rect, radius: f64, tint: Color) {
        if r.is_empty() || tint.a <= 0.0 {
            return;
        }
        let radius = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
        let c = r.center();
        let h = r.size() * 0.5;
        let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)];
        let (u0, v0, u1, v1) = (uv.min.x as f32, uv.min.y as f32, uv.max.x as f32, uv.max.y as f32);
        self.push_quad(
            corners,
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            tint.to_linear().to_gpu(),
            [c.x as f32, c.y as f32, h.x as f32, h.y as f32],
            [radius as f32, MODE_IMAGE, 0.0, image.id.0 as f32],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw2d::{ImageRun, MODE_STROKE, image_runs};
    use crate::images::ImageId;

    #[test]
    fn circles_arcs_and_curves() {
        let mut d = DrawList::new();
        d.circle(Vec2::new(50.0, 50.0), 20.0, Color::WHITE);
        let v = *d.vertices().next().unwrap();
        assert_eq!(v.rect, [50.0, 50.0, 20.0, 20.0]);
        assert_eq!(v.params[0], 20.0, "a circle is a box rounded by its radius");
        d.clear();
        d.ring(Vec2::ZERO, 10.0, 2.0, Color::WHITE);
        assert_eq!(d.vertices().next().unwrap().params[1], MODE_STROKE);
        d.clear();
        d.arc(Vec2::ZERO, 100.0, 0.0, core::f64::consts::PI, 4.0, Color::WHITE);
        let n = d.vertex_count();
        assert!(n >= 6 * 50 && n.is_multiple_of(6), "a half circle of radius 100 is many segments: {n}");
        let first = d.vertices().next().unwrap().pos;
        assert!((first[0] - 100.0).abs() < 3.0 && first[1].abs() < 3.0, "starts at the right: {first:?}");
        d.clear();
        d.pie(Vec2::ZERO, 10.0, 0.0, 1.0, Color::WHITE);
        assert!(d.vertex_count().is_multiple_of(3) && d.vertex_count() >= 9);
        d.clear();
        d.bezier(Vec2::ZERO, Vec2::new(0.0, 100.0), Vec2::new(100.0, 100.0), Vec2::new(100.0, 0.0), 2.0, Color::WHITE);
        let last = d.vertices().last().unwrap().pos;
        assert!((last[0] - 100.0).abs() < 2.0 && last[1].abs() < 2.0, "ends at p3: {last:?}");
        d.clear();
        d.rect_gradient4(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), Color::WHITE, Color::RED, Color::BLACK, Color::BLUE);
        let v: Vec<_> = d.vertices().collect();
        assert_eq!(v[0].color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(v[5].color, [1.0, 0.0, 0.0, 1.0], "top-right");
        assert_eq!(v[2].color, [0.0, 0.0, 0.0, 1.0], "bottom-right");
        assert_eq!(v[1].color, [0.0, 0.0, 1.0, 1.0], "bottom-left");
    }

    #[test]
    fn images_split_into_runs() {
        let mut d = DrawList::new();
        let a = ImageHandle { id: ImageId(3), width: 10, height: 10 };
        let b = ImageHandle { id: ImageId(7), width: 10, height: 10 };
        d.rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        d.image(Rect::from_xywh(0.0, 0.0, 4.0, 2.0), a, 1.0, Color::WHITE);
        d.image(Rect::from_xywh(0.0, 0.0, 4.0, 2.0), a, 0.0, Color::WHITE);
        d.rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        d.image_uv(Rect::from_xywh(0.0, 0.0, 4.0, 2.0), b, Rect::from_xywh(0.5, 0.0, 0.5, 1.0), 0.0, Color::RED);
        let v: Vec<_> = d.vertices().copied().collect();
        let runs = image_runs(&v);
        assert_eq!(runs, vec![
            ImageRun { start: 0, end: 6, image: None },
            ImageRun { start: 6, end: 18, image: Some(ImageId(3)) },
            ImageRun { start: 18, end: 24, image: None },
            ImageRun { start: 24, end: 30, image: Some(ImageId(7)) },
        ]);
        assert_eq!(v[6].params, [1.0, MODE_IMAGE, 0.0, 3.0]);
        assert_eq!(v[6].uv, [0.0, 0.0]);
        assert_eq!(v[8].uv, [1.0, 1.0], "bottom-right corner samples the far corner");
        assert_eq!(v[24].uv, [0.5, 0.0], "a uv sub-rect");
        assert_eq!(v[24].color, [1.0, 0.0, 0.0, 1.0], "the tint");
        assert!(image_runs(&[]).is_empty());
    }
}
