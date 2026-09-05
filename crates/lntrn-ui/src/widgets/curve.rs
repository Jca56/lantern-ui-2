//! A curve editor: points in the unit square joined by straight
//! segments. Drag a point; double-click empty space to add one; Delete or
//! Backspace removes the selected one. Envelopes, EQ shapes, response
//! curves.

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

/// What a curve editor did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveResponse {
    pub changed: bool,
    /// The selected point, by index into the (sorted) points.
    pub selected: Option<usize>,
}

impl Ui<'_> {
    /// `points` are `(x, y)` in `0..=1`, kept sorted by `x`. `height`
    /// physical pixels. One Tab stop.
    pub fn curve_editor(&mut self, label: &str, points: &mut Vec<Vec2>, height: f64) -> CurveResponse {
        let id = self.id(label);
        let m = self.m;
        let rect = self.alloc(Vec2::new(FILL, height.max(m.widget_h * 2.0)));
        let inner = rect.shrink(m.pad);
        let focused = self.focusable(id, rect);
        let to_px = |p: Vec2| Vec2::new(inner.min.x + p.x * inner.width(), inner.max.y - p.y * inner.height());
        let from_px = |q: Vec2| Vec2::new(((q.x - inner.min.x) / inner.width().max(1.0)).clamp(0.0, 1.0), ((inner.max.y - q.y) / inner.height().max(1.0)).clamp(0.0, 1.0));
        let handle = m.px(12.0);
        let remembered = self.state.floats(id, [-1.0; 4])[0];
        let mut selected = if remembered >= 0.0 { Some(remembered as usize).filter(|i| *i < points.len()) } else { None };
        let mut changed = false;

        // The points first, so they win over the background.
        for (i, point) in points.iter_mut().enumerate() {
            let hr = Rect::from_center_size(to_px(*point), Vec2::splat(handle * 2.0));
            let r = self.interact(id.with_index(i), hr, Sense::DRAG);
            if r.hovered || r.held {
                self.state.cursor_icon = if r.held { CursorIcon::Grabbing } else { CursorIcon::Pointer };
            }
            if r.pressed {
                selected = Some(i);
                self.state.focus = Some(id);
            }
            if r.dragging {
                let p = from_px(self.state.pointer);
                if p != *point {
                    *point = p;
                    changed = true;
                }
            }
        }
        let bg = self.interact(id, rect, Sense::CLICK);
        if bg.pressed {
            self.state.focus = Some(id);
        }
        if bg.double_clicked {
            points.push(from_px(self.state.pointer));
            selected = Some(points.len() - 1);
            changed = true;
        }
        if changed {
            // Keep the order by x; follow the selected point through it.
            let pick = selected.and_then(|i| points.get(i).copied());
            points.sort_by(|a, b| a.x.total_cmp(&b.x));
            selected = pick.and_then(|p| points.iter().position(|q| *q == p));
        }
        if focused
            && let Some(i) = selected
            && self.state.take_key(|k| matches!(k.key, Key::Delete | Key::Backspace)).is_some()
        {
            points.remove(i);
            selected = None;
            changed = true;
            self.state.request_rebuild = true;
        }
        self.state.floats(id, [-1.0; 4])[0] = selected.map_or(-1.0, |i| i as f64);

        // ---- draw ----
        self.recessed(rect, self.theme.field);
        let faint = self.theme.border_light.fade(0.25);
        for k in 1..4 {
            let t = k as f64 / 4.0;
            self.draw.vline(inner.min.x + inner.width() * t, inner.min.y, inner.max.y, m.border, faint);
            self.draw.hline(inner.min.x, inner.max.x, inner.max.y - inner.height() * t, m.border, faint);
        }
        let pts: Vec<Vec2> = points.iter().map(|p| to_px(*p)).collect();
        if pts.len() >= 2 {
            self.draw.polyline(&pts, m.px(3.0), self.theme.accent, false);
        }
        for (i, p) in pts.iter().enumerate() {
            let face = if selected == Some(i) { self.theme.selection } else { self.theme.widget.mid() };
            self.draw.circle(*p, handle * 0.5 + m.border, self.theme.border_dark);
            self.draw.circle(*p, handle * 0.5, face);
        }
        self.focus_ring(id, rect);
        CurveResponse { changed, selected }
    }
}
