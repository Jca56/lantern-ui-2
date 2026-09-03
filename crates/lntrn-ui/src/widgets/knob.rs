//! Rotary knobs: the plugin-face control. Big by default (80 logical px),
//! drag up or right for more (D036), Shift for fine, double-click to type,
//! arrows when focused.

use core::f64::consts::PI;

use lntrn_math::{Rect, Vec2};

use crate::state::CursorIcon;
use crate::ui::{FILL, KeyStep, Sense, Ui};
use crate::widgets::slider::format_number;

/// Where the sweep starts and ends: 7 o'clock round to 5 o'clock.
const A0: f64 = 0.75 * PI;
const SWEEP: f64 = 1.5 * PI;
/// Logical pixels of drag for the full range.
const DRAG_PX: f64 = 200.0;

impl Ui<'_> {
    /// A knob 80 logical pixels across with its label above and value
    /// below. Returns `true` when the value changed.
    pub fn knob(&mut self, label: &str, value: &mut f64, min: f64, max: f64) -> bool {
        self.knob_sized(label, value, min, max, 80.0)
    }

    /// A knob `diameter` logical pixels across (never under 64).
    pub fn knob_sized(&mut self, label: &str, value: &mut f64, min: f64, max: f64, diameter: f64) -> bool {
        let id = self.id(label);
        let d = self.m.px(diameter.max(64.0));
        let style = self.text_style();
        let line = style.line_height() as f64 + self.m.gap;
        let text_w = self.measure(label, &style).max(self.measure(&format_number(max, 2), &style)) + self.m.pad * 2.0;
        let w = if self.in_row() { d.max(text_w) } else { FILL };
        let rect = self.alloc(Vec2::new(w, line + d + line));
        let cx = rect.center().x;
        let circle = Rect::from_center_size(Vec2::new(cx, rect.min.y + line + d * 0.5), Vec2::splat(d));
        let value_rect = Rect::new(Vec2::new(rect.min.x, circle.max.y), Vec2::new(rect.max.x, rect.max.y));
        let label_rect = Rect::new(rect.min, Vec2::new(rect.max.x, circle.min.y));
        let range = (max - min).max(1.0e-12);

        // Typing a value (double-click) takes over the value line.
        let mut changed = false;
        let mut typing = false;
        if let Some(done) = self.number_editing(id, value_rect, *value) {
            typing = true;
            if let Some(v) = done {
                *value = v.clamp(min, max);
                changed = true;
            }
        }

        let r = self.interact(id, circle, Sense::DRAG);
        let focused = self.focusable(id, circle);
        if r.double_clicked && !typing {
            self.begin_number_edit(id, *value);
            self.state.request_rebuild = true;
        }
        if r.pressed {
            *self.state.drag_start(id) = *value;
        }
        if r.hovered || r.held {
            self.state.cursor_icon = CursorIcon::NsResize;
        }
        if r.held && !typing {
            let up = self.state.press_pos.y - self.state.pointer.y;
            let right = self.state.pointer.x - self.state.press_pos.x;
            let travelled = (up + right) / self.m.scale;
            if travelled.abs() >= 1.0 {
                let fine = if self.state.mods.shift() { 0.1 } else { 1.0 };
                let start = *self.state.drag_start(id);
                let v = (start + travelled / DRAG_PX * range * fine).clamp(min, max);
                if v != *value {
                    *value = v;
                    changed = true;
                }
            }
        }
        if focused && !typing {
            let v = match self.key_step(id) {
                KeyStep::By(n) => {
                    let fine = if self.state.mods.shift() { 0.001 } else { 0.01 };
                    (*value + n as f64 * range * fine).clamp(min, max)
                }
                KeyStep::Min => min,
                KeyStep::Max => max,
                KeyStep::None => *value,
            };
            if v != *value {
                *value = v;
                changed = true;
            }
        }

        // ---- draw ----
        let t = ((*value - min) / range).clamp(0.0, 1.0);
        let c = circle.center();
        let radius = d * 0.5;
        let theme = self.theme;
        let track_w = self.m.px(6.0);
        let track_r = radius - track_w * 0.5 - self.m.border;
        self.draw.arc(c, track_r, A0, A0 + SWEEP, track_w, theme.shade(theme.field));
        if t > 0.0 {
            self.draw.arc(c, track_r, A0, A0 + SWEEP * t, track_w, theme.accent);
        }
        let cap_r = radius - track_w - self.m.px(4.0);
        let base = if r.held { theme.shade(theme.widget) } else if r.hovered { theme.hover(theme.widget) } else { theme.widget };
        self.draw.circle(c, cap_r + self.m.border, theme.border_dark);
        self.draw.circle_gradient(c, cap_r, theme.top(base), theme.bottom(base));
        self.draw.ring(c, cap_r, self.m.border, theme.highlight(base).fade(0.6));
        let angle = A0 + SWEEP * t;
        let dir = Vec2::from_angle(angle);
        self.draw.line(c + dir * (cap_r * 0.35), c + dir * (cap_r * 0.85), self.m.px(4.0), if t > 0.0 { theme.accent } else { theme.text });
        if focused && self.state.focus_visible {
            self.draw.ring(c, radius + self.m.px(2.0), self.m.px(2.0), theme.focus);
        }
        self.text_centered(label, &style, label_rect, theme.text_dim);
        if !typing {
            let shown = format_number(*value, 2);
            self.text_centered(&shown, &style, value_rect, theme.text);
        }
        changed
    }
}
