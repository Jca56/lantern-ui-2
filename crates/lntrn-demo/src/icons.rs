//! The demo's own icons: what an app draws for itself with
//! `Icon::Custom`, beside the framework's built-ins. Same shape as theirs:
//! centred in the rect, the glyph within 0.28 of the smaller side.

use lntrn_app::lntrn_render::DrawList;
use lntrn_math::{Color, Rect, Vec2};

/// A prism: a triangle with a lit edge, for the tab that shows pictures.
pub fn prism(d: &mut DrawList, rect: Rect, color: Color, stroke: f64) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.28;
    let p = |x: f64, y: f64| c + Vec2::new(x * s, y * s);
    let (top, left, right) = (p(0.0, -1.0), p(-1.0, 0.85), p(1.0, 0.85));
    d.line(top, left, stroke, color);
    d.line(left, right, stroke, color);
    d.line(right, top, stroke, color);
    // The beam: in at the left, out the right as a fan.
    d.line(p(-1.6, -0.1), p(-0.45, 0.2), stroke, color.fade(0.7));
    d.line(p(0.45, 0.2), p(1.6, -0.35), stroke, color.fade(0.7));
    d.line(p(0.45, 0.2), p(1.6, 0.05), stroke, color.fade(0.5));
    d.line(p(0.45, 0.2), p(1.6, 0.45), stroke, color.fade(0.3));
}
