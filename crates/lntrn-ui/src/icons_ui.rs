//! The general icons every app wants (search, close, folder, play, undo,
//! trash, warning, ...), drawn with lines and rects like the rest. `c` is
//! the centre, `s` the half extent of the glyph; `p(x, y)` maps -1..=1
//! into it.

use core::f64::consts::PI;

use lntrn_math::{Color, Rect, Vec2};
use lntrn_render::DrawList;

use crate::icons::Icon;

pub(crate) fn draw(d: &mut DrawList, c: Vec2, s: f64, icon: Icon, color: Color, stroke: f64) {
    let p = |x: f64, y: f64| c + Vec2::new(x * s, y * s);
    match icon {
        Icon::Search => {
            d.ring(p(-0.25, -0.25), s * 0.65, stroke, color);
            d.line(p(0.25, 0.25), p(0.95, 0.95), stroke * 1.4, color);
        }
        Icon::Close => {
            d.line(p(-0.8, -0.8), p(0.8, 0.8), stroke, color);
            d.line(p(-0.8, 0.8), p(0.8, -0.8), stroke, color);
        }
        Icon::Check => d.polyline(&[p(-0.9, 0.05), p(-0.3, 0.65), p(0.9, -0.6)], stroke * 1.3, color, false),
        Icon::Folder => {
            d.stroke_rect(Rect::new(p(-1.0, -0.45), p(1.0, 0.8)), stroke, stroke, color);
            d.polyline(&[p(-1.0, -0.45), p(-1.0, -0.8), p(-0.35, -0.8), p(-0.1, -0.45)], stroke, color, false);
        }
        Icon::File => {
            d.polyline(&[p(-0.7, -1.0), p(0.25, -1.0), p(0.7, -0.55), p(0.7, 1.0), p(-0.7, 1.0)], stroke, color, true);
            d.polyline(&[p(0.25, -1.0), p(0.25, -0.55), p(0.7, -0.55)], stroke, color, false);
        }
        Icon::Gear => {
            d.ring(c, s * 0.55, stroke * 1.2, color);
            for i in 0..8 {
                let dir = Vec2::from_angle(i as f64 * PI / 4.0);
                d.line(c + dir * (s * 0.6), c + dir * s, stroke * 1.5, color);
            }
        }
        Icon::Undo => {
            d.arc(p(0.0, 0.25), s * 0.85, PI, 2.0 * PI, stroke, color);
            let tip = p(-0.85, 0.6);
            d.line(tip, p(-1.2, 0.2), stroke, color);
            d.line(tip, p(-0.5, 0.2), stroke, color);
        }
        Icon::Redo => {
            d.arc(p(0.0, 0.25), s * 0.85, PI, 2.0 * PI, stroke, color);
            let tip = p(0.85, 0.6);
            d.line(tip, p(1.2, 0.2), stroke, color);
            d.line(tip, p(0.5, 0.2), stroke, color);
        }
        Icon::Play => d.triangle(p(-0.7, -0.9), p(0.9, 0.0), p(-0.7, 0.9), color),
        Icon::Pause => {
            d.rounded_rect(Rect::new(p(-0.8, -0.9), p(-0.25, 0.9)), stroke * 0.5, color);
            d.rounded_rect(Rect::new(p(0.25, -0.9), p(0.8, 0.9)), stroke * 0.5, color);
        }
        Icon::Stop => d.rounded_rect(Rect::new(p(-0.8, -0.8), p(0.8, 0.8)), stroke * 0.5, color),
        Icon::Record => d.circle(c, s * 0.8, color),
        Icon::Copy => {
            d.stroke_rect(Rect::new(p(-0.9, -0.9), p(0.4, 0.4)), stroke, stroke, color);
            let front = Rect::new(p(-0.3, -0.3), p(0.9, 0.9));
            d.rounded_rect(front, stroke, color.fade(0.55));
            d.stroke_rect(front, stroke, stroke, color);
        }
        Icon::Paste => {
            d.stroke_rect(Rect::new(p(-0.8, -0.7), p(0.8, 1.0)), stroke, stroke, color);
            d.rounded_rect(Rect::new(p(-0.35, -1.0), p(0.35, -0.5)), stroke * 0.5, color);
            d.line(p(-0.4, 0.05), p(0.4, 0.05), stroke, color);
            d.line(p(-0.4, 0.45), p(0.4, 0.45), stroke, color);
        }
        Icon::Trash => {
            d.line(p(-1.0, -0.7), p(1.0, -0.7), stroke, color);
            d.line(p(-0.3, -1.0), p(0.3, -1.0), stroke, color);
            d.polyline(&[p(-0.75, -0.7), p(-0.6, 1.0), p(0.6, 1.0), p(0.75, -0.7)], stroke, color, false);
            d.line(p(-0.2, -0.3), p(-0.15, 0.7), stroke, color);
            d.line(p(0.2, -0.3), p(0.15, 0.7), stroke, color);
        }
        Icon::Warning => {
            d.polyline(&[p(0.0, -0.95), p(1.0, 0.85), p(-1.0, 0.85)], stroke, color, true);
            d.line(p(0.0, -0.35), p(0.0, 0.25), stroke * 1.3, color);
            d.circle(p(0.0, 0.55), stroke * 0.8, color);
        }
        Icon::Info => {
            d.ring(c, s * 0.95, stroke, color);
            d.circle(p(0.0, -0.45), stroke * 0.8, color);
            d.line(p(0.0, -0.1), p(0.0, 0.55), stroke * 1.3, color);
        }
        Icon::ChevronLeft => d.polyline(&[p(0.4, -0.9), p(-0.4, 0.0), p(0.4, 0.9)], stroke, color, false),
        Icon::ChevronRight => d.polyline(&[p(-0.4, -0.9), p(0.4, 0.0), p(-0.4, 0.9)], stroke, color, false),
        Icon::Lock => {
            d.rounded_rect(Rect::new(p(-0.8, -0.1), p(0.8, 1.0)), stroke, color);
            d.arc(p(0.0, -0.3), s * 0.5, PI, 2.0 * PI, stroke, color);
            d.line(p(-0.5, -0.3), p(-0.5, -0.1), stroke, color);
            d.line(p(0.5, -0.3), p(0.5, -0.1), stroke, color);
        }
        Icon::Eye => {
            d.bezier(p(-1.0, 0.0), p(-0.45, -0.85), p(0.45, -0.85), p(1.0, 0.0), stroke, color);
            d.bezier(p(-1.0, 0.0), p(-0.45, 0.85), p(0.45, 0.85), p(1.0, 0.0), stroke, color);
            d.circle(c, s * 0.32, color);
        }
        Icon::Link => {
            d.stroke_rect(Rect::new(p(-1.0, -0.35), p(0.2, 0.35)), stroke, s * 0.35, color);
            d.stroke_rect(Rect::new(p(-0.2, -0.35), p(1.0, 0.35)), stroke, s * 0.35, color);
        }
        Icon::Pin => {
            d.circle(p(0.0, -0.5), s * 0.45, color);
            d.line(p(-0.6, 0.05), p(0.6, 0.05), stroke, color);
            d.line(p(0.0, 0.05), p(0.0, 1.0), stroke, color);
        }
        Icon::Menu => {
            for y in [-0.7, 0.0, 0.7] {
                d.line(p(-1.0, y), p(1.0, y), stroke, color);
            }
        }
        Icon::Save => {
            d.stroke_rect(Rect::new(p(-0.9, -0.9), p(0.9, 0.9)), stroke, stroke, color);
            d.rounded_rect(Rect::new(p(-0.5, -0.9), p(0.5, -0.35)), 0.0, color);
            d.stroke_rect(Rect::new(p(-0.55, 0.2), p(0.55, 0.9)), stroke, 0.0, color);
        }
        Icon::Filter => d.polyline(&[p(-1.0, -0.9), p(1.0, -0.9), p(0.15, 0.1), p(0.15, 0.9), p(-0.15, 0.7), p(-0.15, 0.1)], stroke, color, true),
        Icon::Star => {
            let pts: Vec<Vec2> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 1.0 } else { 0.45 };
                    c + Vec2::from_angle(-PI / 2.0 + i as f64 * PI / 5.0) * (s * r)
                })
                .collect();
            d.polyline(&pts, stroke, color, true);
        }
        // The Prism set is drawn in `icons`.
        _ => {}
    }
}
