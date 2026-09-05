//! Range sliders (a span with two ends) and vertical sliders (faders).

use lntrn_math::{Rect, Vec2};

use crate::state::CursorIcon;
use crate::ui::{FILL, KeyStep, Sense, Ui};
use crate::widgets::slider::{clamp_step, format_number};

impl Ui<'_> {
    /// Pick a `lo..=hi` span of `min..=max`: drag either end, or the span
    /// itself to move both. Each end is a Tab stop; arrows nudge it.
    /// Returns `true` when changed.
    pub fn range_slider(&mut self, label: &str, lo: &mut f64, hi: &mut f64, min: f64, max: f64, step: f64) -> bool {
        let id = self.id(label);
        let m = self.m;
        let rect = self.alloc(Vec2::new(FILL, m.widget_h));
        let range = (max - min).max(1.0e-12);
        let thumb_w = m.px(20.0);
        let track = Rect::new(Vec2::new(rect.min.x + thumb_w * 0.5, rect.min.y), Vec2::new(rect.max.x - thumb_w * 0.5, rect.max.y));
        let x_of = |v: f64| track.min.x + ((v - min) / range).clamp(0.0, 1.0) * track.width();
        let v_of = |x: f64| clamp_step(min + ((x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0) * range, min, max, step);
        let (lo_id, hi_id) = (id.with("lo"), id.with("hi"));
        let thumb = |v: f64| Rect::from_center_size(Vec2::new(x_of(v), rect.center().y), Vec2::new(thumb_w, rect.height()));
        let (lo_rect, hi_rect) = (thumb(*lo), thumb(*hi));

        // The ends first, so they win over the span between them.
        let rl = self.interact(lo_id, lo_rect, Sense::DRAG);
        let rh = self.interact(hi_id, hi_rect, Sense::DRAG);
        let fl = self.focusable(lo_id, lo_rect);
        let fh = self.focusable(hi_id, hi_rect);
        let span = Rect::new(Vec2::new(lo_rect.max.x, rect.min.y), Vec2::new(hi_rect.min.x.max(lo_rect.max.x), rect.max.y));
        let rs = self.interact(id, span, Sense::DRAG);
        if rl.hovered || rh.hovered || rs.hovered || rl.held || rh.held || rs.held {
            self.state.cursor_icon = CursorIcon::EwResize;
        }
        let mut changed = false;
        let pointer_x = self.state.pointer.x;
        if rl.dragging {
            let v = v_of(pointer_x).min(*hi);
            changed |= v != *lo;
            *lo = v;
        }
        if rh.dragging {
            let v = v_of(pointer_x).max(*lo);
            changed |= v != *hi;
            *hi = v;
        }
        if rs.pressed {
            *self.state.drag_start(id) = *lo;
        }
        if rs.dragging && !rs.pressed {
            let width = *hi - *lo;
            let start = *self.state.drag_start(id);
            let moved = (pointer_x - self.state.press_pos.x) / track.width().max(1.0) * range;
            let new_lo = clamp_step(start + moved, min, max - width, step);
            if new_lo != *lo {
                *lo = new_lo;
                *hi = new_lo + width;
                changed = true;
            }
        }
        let fine = if self.state.mods.shift() { 0.001 } else { 0.01 };
        let by = if step > 0.0 { step.max(range * fine) } else { range * fine };
        if fl {
            let v = match self.key_step(lo_id) {
                KeyStep::By(n) => clamp_step(*lo + by * n as f64, min, *hi, step),
                KeyStep::Min => min,
                KeyStep::Max => *hi,
                KeyStep::None => *lo,
            };
            changed |= v != *lo;
            *lo = v;
        }
        if fh {
            let v = match self.key_step(hi_id) {
                KeyStep::By(n) => clamp_step(*hi + by * n as f64, *lo, max, step),
                KeyStep::Min => *lo,
                KeyStep::Max => max,
                KeyStep::None => *hi,
            };
            changed |= v != *hi;
            *hi = v;
        }

        // ---- draw ----
        let well = if rs.hovered || rs.held { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(rect, well);
        let filled = Rect::new(Vec2::new(x_of(*lo), rect.min.y), Vec2::new(x_of(*hi), rect.max.y));
        if filled.width() >= 1.0 {
            self.draw.push_clip(filled);
            self.fill_shaded(rect.shrink(m.border), self.theme.shaded(self.theme.accent));
            self.draw.pop_clip();
        }
        let style = self.text_style();
        let inner = Rect::new(Vec2::new(rect.min.x + m.pad, rect.min.y), Vec2::new(rect.max.x - m.pad, rect.max.y));
        self.text_in_rect(label, &style, inner, self.theme.text);
        self.text_right(&format!("{} – {}", format_number(*lo, 3), format_number(*hi, 3)), &style, inner, self.theme.text);
        for (r, held, tid) in [(thumb(*lo), rl.held, lo_id), (thumb(*hi), rh.held, hi_id)] {
            let base = if held { self.theme.shaded(self.theme.accent) } else { self.theme.widget };
            self.raised(r.shrink(m.border), base, held);
            self.focus_ring(tid, r);
        }
        changed
    }

    /// A vertical slider `height` physical pixels tall (a fader): drag up
    /// for more, the label above, the value below. Double-click types.
    /// Returns `true` when changed.
    pub fn vslider(&mut self, label: &str, value: &mut f64, min: f64, max: f64, step: f64, height: f64) -> bool {
        let id = self.id(label);
        let m = self.m;
        let style = self.text_style();
        let line = style.line_height() as f64 + m.gap;
        let text_w = self.measure(label, &style).max(self.measure(&format_number(max, 2), &style)) + m.pad * 2.0;
        let w = if self.in_row() { text_w.max(m.widget_h) } else { FILL };
        let height = height.max(m.widget_h * 2.0);
        let rect = self.alloc(Vec2::new(w, line + height + line));
        let cx = rect.center().x;
        let track = Rect::from_center_size(Vec2::new(cx, rect.min.y + line + height * 0.5), Vec2::new(m.px(30.0), height));
        let grab = Rect::from_center_size(track.center(), Vec2::new(m.widget_h, height));
        let label_rect = Rect::new(rect.min, Vec2::new(rect.max.x, track.min.y));
        let value_rect = Rect::new(Vec2::new(rect.min.x, track.max.y), rect.max);
        let range = (max - min).max(1.0e-12);

        let mut changed = false;
        let mut typing = false;
        if let Some(done) = self.number_editing(id, value_rect, *value) {
            typing = true;
            if let Some(v) = done {
                *value = clamp_step(v, min, max, step);
                changed = true;
            }
        }
        let r = self.interact(id, grab, Sense::DRAG);
        let focused = self.focusable(id, track);
        if r.double_clicked && !typing {
            self.begin_number_edit(id, *value);
            self.state.request_rebuild = true;
        }
        if r.hovered || r.held {
            self.state.cursor_icon = CursorIcon::NsResize;
        }
        if r.dragging && !typing {
            let t = ((track.max.y - self.state.pointer.y) / track.height()).clamp(0.0, 1.0);
            let v = clamp_step(min + t * range, min, max, step);
            changed |= v != *value;
            *value = v;
        }
        if focused && !typing {
            let fine = if self.state.mods.shift() { 0.001 } else { 0.01 };
            let by = if step > 0.0 { step.max(range * fine) } else { range * fine };
            let v = match self.key_step(id) {
                KeyStep::By(n) => clamp_step(*value + by * n as f64, min, max, step),
                KeyStep::Min => min,
                KeyStep::Max => max,
                KeyStep::None => *value,
            };
            changed |= v != *value;
            *value = v;
        }

        // ---- draw ----
        let t = ((*value - min) / range).clamp(0.0, 1.0);
        let well = if r.hovered || r.held { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(track, well);
        let top = track.max.y - track.height() * t;
        let filled = Rect::new(Vec2::new(track.min.x, top), track.max);
        if filled.height() >= 1.0 {
            self.draw.push_clip(filled);
            self.fill_shaded(track.shrink(m.border), self.theme.shaded(self.theme.accent));
            self.draw.pop_clip();
        }
        let thumb = Rect::from_center_size(Vec2::new(cx, top.clamp(track.min.y + m.px(7.0), track.max.y - m.px(7.0))), Vec2::new(track.width() + m.px(10.0), m.px(14.0)));
        self.raised(thumb, if r.held { self.theme.shaded(self.theme.accent) } else { self.theme.widget }, r.held);
        self.text_centered(label, &style, label_rect, self.theme.text_dim);
        if !typing {
            self.text_centered(&format_number(*value, 2), &style, value_rect, self.theme.text);
        }
        self.focus_ring(id, track);
        changed
    }
}
