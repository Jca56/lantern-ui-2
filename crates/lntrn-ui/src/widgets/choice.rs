//! Radio groups (one of a few, all visible) and spinners (a number with
//! − and + either side).

use lntrn_math::{Rect, Vec2};

use crate::icons::Icon;
use crate::state::CursorIcon;
use crate::ui::{FILL, KeyStep, Sense, Ui};

impl Ui<'_> {
    /// One of `options`, each with a dot: a column, or a line inside a
    /// row. One Tab stop; Up and Down move the pick. Returns `true` when
    /// it changed.
    pub fn radio(&mut self, label: &str, selected: &mut usize, options: &[&str]) -> bool {
        let id = self.id(label);
        let m = self.m;
        let style = self.text_style();
        let dot = m.px(25.0);
        let across = self.in_row();
        let widths: Vec<f64> = options.iter().map(|o| dot + m.gap + self.measure(o, &style) + m.pad * 2.0).collect();
        let size = if across { Vec2::new(widths.iter().sum(), m.widget_h) } else { Vec2::new(FILL, m.widget_h * options.len().max(1) as f64) };
        let rect = self.alloc(size);
        let mut changed = false;
        if self.focusable(id, rect) && !options.is_empty() {
            let last = options.len() - 1;
            let next = match self.key_step(id) {
                KeyStep::By(n) => (*selected as i64 - n as i64).clamp(0, last as i64) as usize,
                KeyStep::Min => 0,
                KeyStep::Max => last,
                KeyStep::None => *selected,
            };
            if next != *selected {
                *selected = next;
                changed = true;
            }
        }
        let mut at = rect.min;
        for (i, option) in options.iter().enumerate() {
            let r = if across { Rect::from_min_size(at, Vec2::new(widths[i], m.widget_h)) } else { Rect::from_min_size(at, Vec2::new(rect.width(), m.widget_h)) };
            let resp = self.interact(id.with_index(i), r, Sense::CLICK);
            if resp.hovered {
                self.state.cursor_icon = CursorIcon::Pointer;
            }
            if resp.pressed {
                self.state.focus = Some(id);
            }
            if resp.clicked && *selected != i {
                *selected = i;
                changed = true;
            }
            let c = Vec2::new(r.min.x + dot * 0.5, r.center().y);
            let well = if resp.hovered { self.theme.hover(self.theme.field) } else { self.theme.field };
            self.draw.circle(c, dot * 0.5 + m.border, self.theme.border_dark);
            self.draw.circle(c, dot * 0.5, well);
            if *selected == i {
                self.draw.circle(c, dot * 0.28, self.theme.accent);
            }
            let text_rect = Rect::new(Vec2::new(r.min.x + dot + m.gap, r.min.y), r.max);
            self.text_in_rect(option, &style, text_rect, self.theme.text);
            if across {
                at.x += widths[i];
            } else {
                at.y += m.widget_h;
            }
        }
        self.focus_ring(id, rect);
        changed
    }

    /// A number with − and + buttons either side; the middle drags and
    /// types like [`Self::drag_value`]. Shift steps by a tenth. Returns
    /// `true` when changed.
    pub fn spinner(&mut self, label: &str, value: &mut f64, step: f64, range: Option<(f64, f64)>, decimals: usize) -> bool {
        let id = self.id(label);
        let m = self.m;
        let w = if self.in_row() { m.px(220.0) } else { FILL };
        let rect = self.alloc(Vec2::new(w, m.widget_h));
        let (dec, rest) = rect.split_x(rect.min.x + m.widget_h);
        let (mid, inc) = rest.split_x(rest.max.x - m.widget_h);
        let (min, max) = range.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        let fine = if self.state.mods.shift() { 0.1 } else { 1.0 };
        let mut changed = false;
        if self.icon_button_in(id.with("dec"), dec, Icon::Minus, None, "").clicked {
            let v = (*value - step * fine).clamp(min, max);
            changed |= v != *value;
            *value = v;
        }
        if self.icon_button_in(id.with("inc"), inc, Icon::Plus, None, "").clicked {
            let v = (*value + step * fine).clamp(min, max);
            changed |= v != *value;
            *value = v;
        }
        changed |= self.drag_value_in(id, mid, label, value, step, range, decimals);
        changed
    }

    /// Integer flavour of [`Self::spinner`].
    pub fn spinner_int(&mut self, label: &str, value: &mut i64, range: Option<(i64, i64)>) -> bool {
        let mut v = *value as f64;
        let range = range.map(|(a, b)| (a as f64, b as f64));
        let changed = self.spinner(label, &mut v, 1.0, range, 0);
        if changed {
            *value = v.round() as i64;
        }
        changed
    }
}
