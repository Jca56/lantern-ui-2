//! A two-axis pad: drag a point around a square. Right is more x, up is
//! more y.

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};
use crate::widgets::slider::format_number;

impl Ui<'_> {
    /// A square `size` logical pixels across (never under 100) with the
    /// point at (`x`, `y`) within `x_range` and `y_range`; drag it, or
    /// nudge it with the arrows when focused (Shift for fine). Returns
    /// `true` when either value changed.
    #[allow(clippy::too_many_arguments)]
    pub fn xy_pad(&mut self, label: &str, x: &mut f64, y: &mut f64, x_range: (f64, f64), y_range: (f64, f64), size: f64) -> bool {
        let id = self.id(label);
        let m = self.m;
        let style = self.text_style();
        let line = style.line_height() as f64 + m.gap;
        let d = m.px(size.max(100.0));
        let w = if self.in_row() { d } else { FILL };
        let rect = self.alloc(Vec2::new(w, line + d));
        let square = Rect::from_center_size(Vec2::new(rect.center().x, rect.min.y + line + d * 0.5), Vec2::splat(d));
        let label_rect = Rect::new(rect.min, Vec2::new(rect.max.x, square.min.y));
        let (xs, ys) = ((x_range.1 - x_range.0).max(1.0e-12), (y_range.1 - y_range.0).max(1.0e-12));

        let r = self.interact(id, square, Sense::DRAG);
        let focused = self.focusable(id, square);
        if r.hovered || r.held {
            self.state.cursor_icon = if r.held { CursorIcon::Grabbing } else { CursorIcon::Pointer };
        }
        let mut changed = false;
        if r.dragging {
            let tx = ((self.state.pointer.x - square.min.x) / d).clamp(0.0, 1.0);
            let ty = ((square.max.y - self.state.pointer.y) / d).clamp(0.0, 1.0);
            let (nx, ny) = (x_range.0 + tx * xs, y_range.0 + ty * ys);
            changed |= nx != *x || ny != *y;
            *x = nx;
            *y = ny;
        }
        if focused {
            let fine = if self.state.mods.shift() { 0.001 } else { 0.01 };
            while let Some(k) = self.state.take_key(|k| matches!(k.key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown)) {
                let (nx, ny) = match k.key {
                    Key::ArrowLeft => (*x - xs * fine, *y),
                    Key::ArrowRight => (*x + xs * fine, *y),
                    Key::ArrowUp => (*x, *y + ys * fine),
                    _ => (*x, *y - ys * fine),
                };
                *x = nx.clamp(x_range.0, x_range.1);
                *y = ny.clamp(y_range.0, y_range.1);
                changed = true;
                self.state.request_rebuild = true;
            }
        }

        // ---- draw ----
        let well = if r.hovered || r.held { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(square, well);
        let faint = self.theme.border_light.fade(0.3);
        let c = square.center();
        self.draw.vline(c.x, square.min.y, square.max.y, m.border, faint);
        self.draw.hline(square.min.x, square.max.x, c.y, m.border, faint);
        let tx = ((*x - x_range.0) / xs).clamp(0.0, 1.0);
        let ty = ((*y - y_range.0) / ys).clamp(0.0, 1.0);
        let p = Vec2::new(square.min.x + tx * d, square.max.y - ty * d);
        let cross = self.theme.text_dim.fade(0.5);
        self.draw.vline(p.x, square.min.y, square.max.y, m.border, cross);
        self.draw.hline(square.min.x, square.max.x, p.y, m.border, cross);
        self.draw.circle(p, m.px(9.0), self.theme.border_dark);
        self.draw.circle(p, m.px(7.0), self.theme.accent);
        let inner = Rect::new(Vec2::new(label_rect.min.x + m.pad, label_rect.min.y), Vec2::new(label_rect.max.x - m.pad, label_rect.max.y));
        self.text_in_rect(label, &style, inner, self.theme.text_dim);
        self.text_right(&format!("{}, {}", format_number(*x, 2), format_number(*y, 2)), &style, inner, self.theme.text);
        self.focus_ring(id, square);
        changed
    }
}
