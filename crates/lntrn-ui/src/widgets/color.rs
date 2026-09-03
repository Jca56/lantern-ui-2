//! Colour picking: a swatch that opens a popup with a saturation/value
//! square, a hue bar, an alpha bar and a hex field.

use lntrn_math::{Color, Rect, Vec2};

use crate::event::Key;
use crate::id::WidgetId;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

impl Ui<'_> {
    /// A swatch of `color` with `label` beside it; click (or Enter) opens
    /// the picker. Returns `true` on the frames the colour changes.
    pub fn color_picker(&mut self, label: &str, color: &mut Color) -> bool {
        let id = self.id(label);
        let style = self.text_style();
        let sw = self.m.widget_h;
        let w = if self.in_row() { sw + if label.is_empty() { 0.0 } else { self.m.gap + self.measure(label, &style) } } else { FILL };
        let rect = self.alloc(Vec2::new(w, sw));
        let swatch = Rect::from_min_size(rect.min, Vec2::splat(sw));
        let mut r = self.interact(id, swatch, Sense::CLICK);
        self.focusable(id);
        self.key_click(id, &mut r);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let open = *self.state.open(id);
        self.draw_swatch(swatch, *color, r.hovered || open);
        if !label.is_empty() {
            let text_rect = Rect::new(Vec2::new(swatch.max.x + self.m.gap, rect.min.y), rect.max);
            self.text_in_rect(label, &style, text_rect, self.theme.text);
        }
        self.focus_ring(id, swatch);
        if r.clicked {
            *self.state.open(id) = !open;
            self.state.request_rebuild = true;
        }
        if *self.state.open(id) { self.color_popup(id, swatch, color) } else { false }
    }

    /// A colour over a dark well so alpha shows.
    fn draw_swatch(&mut self, rect: Rect, color: Color, hot: bool) {
        let well = if hot { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(rect, well);
        let inner = rect.shrink(self.m.border * 2.0);
        // Left half over the well, right half over white: alpha reads either way.
        let (l, rr) = inner.split_x(inner.center().x);
        self.draw.rect(rr, Color::WHITE);
        self.draw.rect(l, color);
        self.draw.rect(rr, color);
    }

    /// The picker popup under `anchor`. Returns `true` when the colour changed.
    fn color_popup(&mut self, id: WidgetId, anchor: Rect, color: &mut Color) -> bool {
        let m = self.m;
        let w = m.px(360.0);
        let square_h = m.px(220.0);
        let bar_h = m.px(30.0);
        let h = m.pad * 2.0 + square_h + m.gap + bar_h + m.gap + bar_h + m.gap + m.widget_h;
        let window = self.state_window_rect();
        let below = anchor.max.y + m.gap;
        let y = if below + h <= window.max.y { below } else { (anchor.min.y - m.gap - h).max(window.min.y) };
        let x = anchor.min.x.min(window.max.x - w).max(window.min.x);
        let rect = Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h));
        let layer = self.layer() + 1;
        self.state.keep_popup(rect, layer);
        let mut close = self.state.take_key(|k| k.key == Key::Escape).is_some();
        if self.state.pressed && !rect.contains(self.state.press_pos) && !anchor.contains(self.state.press_pos) {
            close = true;
        }
        if close {
            *self.state.open(id) = false;
            self.state.request_rebuild = true;
            return false;
        }

        // Hue and saturation survive a black or grey colour, where the
        // colour alone cannot say what they were.
        let mem = *self.state.floats(id, [-1.0; 4]);
        let [h0, s0, v0] = color.to_hsv();
        let remembered = mem[0] >= 0.0 && Color::from_hsv(mem[0], mem[1], mem[2]).approx_eq(Color::rgb(color.r, color.g, color.b), 1e-6);
        let (mut hue, mut sat, mut val) = if remembered { (mem[0], mem[1], mem[2]) } else { (h0, s0, v0) };
        let mut alpha = color.a;
        let mut changed = false;

        let saved_layer = self.layer();
        let saved_clip = self.clip();
        self.draw.set_layer(layer);
        self.set_layer_internal(layer);
        self.set_clip(rect);
        self.draw.push_clip_absolute(rect.expand(m.px(20.0)));
        self.floating_panel(rect, self.theme.header);
        self.draw.pop_clip();
        self.draw.push_clip_absolute(rect);
        let inner = rect.shrink(m.pad);

        // ---- saturation / value square ----
        let square = Rect::from_min_size(inner.min, Vec2::new(inner.width(), square_h));
        let r = self.interact(id.with("sv"), square, Sense::DRAG);
        if r.hovered || r.held {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        if r.pressed || r.dragging {
            let p = square.clamp_point(self.state.pointer);
            sat = (p.x - square.min.x) / square.width().max(1.0);
            val = 1.0 - (p.y - square.min.y) / square.height().max(1.0);
            changed = true;
        }
        let pure = Color::from_hsv(hue, 1.0, 1.0);
        self.draw.rect_gradient4(square, Color::WHITE, pure, pure, Color::WHITE);
        self.draw.rect_gradient(square, Color::BLACK.fade(0.0), Color::BLACK);
        self.draw.stroke_rect(square, m.border, 0.0, self.theme.border_dark);
        let marker = Vec2::new(square.min.x + sat * square.width(), square.min.y + (1.0 - val) * square.height());
        self.draw.ring(marker, m.px(7.0), m.px(2.0), Color::BLACK);
        self.draw.ring(marker, m.px(5.0), m.px(2.0), Color::WHITE);

        // ---- hue bar ----
        let hue_bar = Rect::from_min_size(Vec2::new(inner.min.x, square.max.y + m.gap), Vec2::new(inner.width(), bar_h));
        let r = self.interact(id.with("hue"), hue_bar, Sense::DRAG);
        if r.hovered || r.held {
            self.state.cursor_icon = CursorIcon::EwResize;
        }
        if r.pressed || r.dragging {
            let p = hue_bar.clamp_point(self.state.pointer);
            hue = ((p.x - hue_bar.min.x) / hue_bar.width().max(1.0)).clamp(0.0, 0.9999);
            changed = true;
        }
        for i in 0..6 {
            let x0 = hue_bar.min.x + hue_bar.width() * i as f64 / 6.0;
            let x1 = hue_bar.min.x + hue_bar.width() * (i + 1) as f64 / 6.0;
            let seg = Rect::new(Vec2::new(x0, hue_bar.min.y), Vec2::new(x1, hue_bar.max.y));
            self.draw.rect_gradient_h(seg, Color::from_hsv(i as f64 / 6.0, 1.0, 1.0), Color::from_hsv((i + 1) as f64 / 6.0, 1.0, 1.0));
        }
        self.draw.stroke_rect(hue_bar, m.border, 0.0, self.theme.border_dark);
        self.draw_bar_marker(hue_bar, hue);

        // ---- alpha bar ----
        let alpha_bar = Rect::from_min_size(Vec2::new(inner.min.x, hue_bar.max.y + m.gap), Vec2::new(inner.width(), bar_h));
        let r = self.interact(id.with("alpha"), alpha_bar, Sense::DRAG);
        if r.hovered || r.held {
            self.state.cursor_icon = CursorIcon::EwResize;
        }
        if r.pressed || r.dragging {
            let p = alpha_bar.clamp_point(self.state.pointer);
            alpha = ((p.x - alpha_bar.min.x) / alpha_bar.width().max(1.0)).clamp(0.0, 1.0);
            changed = true;
        }
        let opaque = Color::from_hsv(hue, sat, val);
        let (l, rr) = alpha_bar.split_x(alpha_bar.center().x);
        self.draw.rect(l, self.theme.field);
        self.draw.rect(rr, Color::WHITE);
        self.draw.rect_gradient_h(alpha_bar, opaque.with_alpha(0.0), opaque);
        self.draw.stroke_rect(alpha_bar, m.border, 0.0, self.theme.border_dark);
        self.draw_bar_marker(alpha_bar, alpha);

        // ---- hex field and preview ----
        let row = Rect::from_min_size(Vec2::new(inner.min.x, alpha_bar.max.y + m.gap), Vec2::new(inner.width(), m.widget_h));
        let (preview, field) = row.take_left(m.widget_h);
        let field = Rect::new(Vec2::new(field.min.x + m.gap, field.min.y), field.max);
        let hex_id = id.with("hex");
        let editing = self.state.has_focus(hex_id);
        let mut text = if editing { self.state.text_edit(hex_id).buffer.take().unwrap_or_else(|| opaque.with_alpha(alpha).to_hex_string()) } else { opaque.with_alpha(alpha).to_hex_string() };
        let tr = self.text_edit_core(hex_id, field, &mut text);
        if tr.focused {
            if tr.changed && let Some(c) = Color::parse_hex(&text) {
                [hue, sat, val] = c.to_hsv();
                alpha = c.a;
                changed = true;
            }
            self.state.text_edit(hex_id).buffer = Some(text);
        }
        if tr.committed || tr.cancelled {
            self.state.focus = None;
            self.state.request_rebuild = true;
        }
        self.draw_swatch(preview, opaque.with_alpha(alpha), false);

        self.draw.pop_clip();
        self.set_clip(saved_clip);
        self.set_layer_internal(saved_layer);
        self.draw.set_layer(saved_layer);

        *self.state.floats(id, [-1.0; 4]) = [hue, sat, val, 0.0];
        if changed {
            let c = Color::from_hsv(hue, sat, val).with_alpha(alpha);
            if !c.approx_eq(*color, 1e-12) {
                *color = c;
                return true;
            }
        }
        false
    }

    fn draw_bar_marker(&mut self, bar: Rect, t: f64) {
        let x = bar.min.x + bar.width() * t;
        let w = m_px(self, 3.0);
        self.draw.rect(Rect::new(Vec2::new(x - w * 0.5 - self.m.border, bar.min.y - self.m.border), Vec2::new(x + w * 0.5 + self.m.border, bar.max.y + self.m.border)), Color::BLACK);
        self.draw.rect(Rect::new(Vec2::new(x - w * 0.5, bar.min.y), Vec2::new(x + w * 0.5, bar.max.y)), Color::WHITE);
    }
}

fn m_px(ui: &Ui, v: f64) -> f64 {
    ui.m.px(v)
}
