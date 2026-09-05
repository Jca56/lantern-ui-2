//! Colour picking: a swatch that opens a big popup with a
//! saturation/value square, a hue bar and a hex field. A gradient's
//! swatch opens the same popup with two swatches in it, one per end;
//! the selected one is what the square and the field change.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_props::Gradient;

use crate::event::Key;
use crate::id::WidgetId;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

impl Ui<'_> {
    /// A swatch of `color` with `label` beside it; click (or Enter) opens
    /// the picker. Returns `true` on the frames the colour changes.
    pub fn color_picker(&mut self, label: &str, color: &mut Color) -> bool {
        let (id, swatch) = self.swatch_row(label);
        let open = *self.state.open(id);
        let r = self.swatch_interact(id, swatch);
        self.draw_swatch(swatch, *color, r.hovered || open);
        self.focus_ring(id, swatch);
        if !*self.state.open(id) {
            return false;
        }
        let mut colors = [*color];
        let changed = self.picker_popup(id, swatch, &mut colors);
        *color = colors[0];
        changed
    }

    /// A swatch showing `g` top to bottom; the picker has a swatch for
    /// each end.
    pub fn gradient_picker(&mut self, label: &str, g: &mut Gradient) -> bool {
        let (id, swatch) = self.swatch_row(label);
        let open = *self.state.open(id);
        let r = self.swatch_interact(id, swatch);
        self.draw_gradient_swatch(swatch, *g, r.hovered || open);
        self.focus_ring(id, swatch);
        if !*self.state.open(id) {
            return false;
        }
        let mut colors = [g.top, g.bottom];
        let changed = self.picker_popup(id, swatch, &mut colors);
        *g = Gradient::new(colors[0], colors[1]);
        changed
    }

    /// The row a picker sits in: its id and the swatch's rect, the label
    /// drawn beside it.
    fn swatch_row(&mut self, label: &str) -> (WidgetId, Rect) {
        let id = self.id(label);
        let style = self.text_style();
        let sw = self.m.widget_h;
        let w = if self.in_row() { sw + if label.is_empty() { 0.0 } else { self.m.gap + self.measure(label, &style) } } else { FILL };
        let rect = self.alloc(Vec2::new(w, sw));
        let swatch = Rect::from_min_size(rect.min, Vec2::splat(sw));
        if !label.is_empty() {
            let text_rect = Rect::new(Vec2::new(swatch.max.x + self.m.gap, rect.min.y), rect.max);
            self.text_in_rect(label, &style, text_rect, self.theme.text);
        }
        (id, swatch)
    }

    fn swatch_interact(&mut self, id: WidgetId, swatch: Rect) -> crate::ui::Response {
        let mut r = self.interact(id, swatch, Sense::CLICK);
        self.focusable(id, swatch);
        self.key_click(id, &mut r);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        if r.clicked {
            let open = *self.state.open(id);
            *self.state.open(id) = !open;
            self.state.request_rebuild = true;
        }
        r
    }

    /// A colour in a well.
    fn draw_swatch(&mut self, rect: Rect, color: Color, hot: bool) {
        let well = if hot { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(rect, well);
        self.draw.rect(rect.shrink(self.m.bevel * 2.0), color);
    }

    /// A gradient in a well, top to bottom.
    fn draw_gradient_swatch(&mut self, rect: Rect, g: Gradient, hot: bool) {
        let well = if hot { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(rect, well);
        self.draw.rect_gradient(rect.shrink(self.m.bevel * 2.0), g.top, g.bottom);
    }

    /// The picker popup under `anchor`, editing one of `colors` at a time
    /// (a swatch for each when there are two). Returns `true` when a
    /// colour changed.
    fn picker_popup(&mut self, id: WidgetId, anchor: Rect, colors: &mut [Color]) -> bool {
        let m = self.m;
        let two = colors.len() > 1;
        let w = m.px(560.0);
        let square_h = m.px(340.0);
        let bar_h = m.px(36.0);
        let ends_h = if two { m.widget_h * 1.6 + m.gap } else { 0.0 };
        let h = m.pad * 2.0 + ends_h + square_h + m.gap + bar_h + m.gap + m.widget_h;
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

        // Which end is being edited, and the hue and saturation that
        // survive a black or grey colour (the colour alone cannot say).
        let mem = *self.state.floats(id, [-1.0; 4]);
        let mut selected = (mem[3].max(0.0) as usize).min(colors.len() - 1);
        let mut changed = false;

        let saved_layer = self.layer();
        let saved_clip = self.clip();
        self.draw.set_layer(layer);
        self.set_layer_internal(layer);
        self.set_clip(rect);
        self.draw.push_clip_absolute(rect.expand(m.px(20.0)));
        let panel = self.theme.header;
        self.floating_panel(rect, panel);
        self.draw.pop_clip();
        self.draw.push_clip_absolute(rect);
        let inner = rect.shrink(m.pad);
        let mut top = inner.min.y;

        // ---- the two ends: click one to edit it ----
        if two {
            let strip = Rect::from_min_size(Vec2::new(inner.min.x, top), Vec2::new(inner.width(), m.widget_h * 1.6));
            let (l, r) = strip.split_x(strip.center().x);
            let cells = [Rect::new(l.min, Vec2::new(l.max.x - m.gap * 0.5, l.max.y)), Rect::new(Vec2::new(r.min.x + m.gap * 0.5, r.min.y), r.max)];
            for (k, cell) in cells.into_iter().enumerate() {
                let sid = id.with("end").with_index(k);
                let resp = self.interact(sid, cell, Sense::CLICK);
                if resp.hovered {
                    self.state.cursor_icon = CursorIcon::Pointer;
                }
                if resp.clicked {
                    selected = k;
                    self.state.request_rebuild = true;
                }
                self.draw_swatch(cell, colors[k], false);
                let style = self.text_style();
                let name = if k == 0 { "Top" } else { "Bottom" };
                let ink = if colors[k].to_linear().luminance_linear() > 0.35 { Color::BLACK } else { Color::WHITE };
                self.text_centered(name, &style, cell, ink.fade(0.85));
                if k == selected {
                    self.draw.stroke_rect(cell, m.px(3.0), m.radius, self.theme.accent);
                }
            }
            top = strip.max.y + m.gap;
        }
        let color = colors[selected];
        let [h0, s0, v0] = color.to_hsv();
        let remembered = mem[0] >= 0.0 && Color::from_hsv(mem[0], mem[1], mem[2]).approx_eq(Color::rgb(color.r, color.g, color.b), 1e-6);
        let (mut hue, mut sat, mut val) = if remembered { (mem[0], mem[1], mem[2]) } else { (h0, s0, v0) };

        // ---- saturation / value square ----
        let square = Rect::from_min_size(Vec2::new(inner.min.x, top), Vec2::new(inner.width(), square_h));
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
        self.draw.ring(marker, m.px(9.0), m.px(2.5), Color::BLACK);
        self.draw.ring(marker, m.px(6.5), m.px(2.5), Color::WHITE);

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

        // ---- hex field and preview ----
        let row = Rect::from_min_size(Vec2::new(inner.min.x, hue_bar.max.y + m.gap), Vec2::new(inner.width(), m.widget_h));
        let (preview, field) = row.take_left(m.widget_h);
        let field = Rect::new(Vec2::new(field.min.x + m.gap, field.min.y), field.max);
        let hex_id = id.with("hex");
        let editing = self.state.has_focus(hex_id);
        let shown = Color::from_hsv(hue, sat, val).with_alpha(color.a);
        let mut text = if editing { self.state.text_edit(hex_id).buffer.take().unwrap_or_else(|| shown.to_hex_string()) } else { shown.to_hex_string() };
        let tr = self.text_edit_core(hex_id, field, &mut text);
        if tr.focused {
            if tr.changed && let Some(c) = Color::parse_hex(&text) {
                [hue, sat, val] = c.to_hsv();
                changed = true;
            }
            self.state.text_edit(hex_id).buffer = Some(text);
        }
        if tr.committed || tr.cancelled {
            self.state.focus = None;
            self.state.request_rebuild = true;
        }
        self.draw_swatch(preview, shown, false);

        self.draw.pop_clip();
        self.set_clip(saved_clip);
        self.set_layer_internal(saved_layer);
        self.draw.set_layer(saved_layer);

        *self.state.floats(id, [-1.0; 4]) = [hue, sat, val, selected as f64];
        if changed {
            let c = Color::from_hsv(hue, sat, val).with_alpha(color.a);
            if !c.approx_eq(color, 1e-12) {
                colors[selected] = c;
                return true;
            }
        }
        false
    }

    fn draw_bar_marker(&mut self, bar: Rect, t: f64) {
        let x = bar.min.x + bar.width() * t;
        let w = self.m.px(4.0);
        let edge = self.m.bevel;
        self.draw.rect(Rect::new(Vec2::new(x - w * 0.5 - edge, bar.min.y - edge), Vec2::new(x + w * 0.5 + edge, bar.max.y + edge)), Color::BLACK);
        self.draw.rect(Rect::new(Vec2::new(x - w * 0.5, bar.min.y), Vec2::new(x + w * 0.5, bar.max.y)), Color::WHITE);
    }
}
