//! Painting helpers on [`Ui`]: text styles and placement, and the shaded
//! shapes every widget is built from (raised, recessed, floating panels).

use lntrn_math::{Color, Rect, Vec2};
use lntrn_text::{TextMetrics, TextStyle};

use crate::ui::Ui;

impl Ui<'_> {
    // ---- text -----------------------------------------------------------

    pub fn text_style(&self) -> TextStyle {
        TextStyle::new(self.m.text_size)
    }

    pub fn heading_style(&self) -> TextStyle {
        TextStyle::new(self.m.heading_size).bold()
    }

    pub fn mono_style(&self) -> TextStyle {
        TextStyle::new(self.m.text_size).mono()
    }

    pub fn measure(&mut self, s: &str, style: &TextStyle) -> f64 {
        self.text.measure(s, style) as f64
    }

    /// Draw `s` with its line box's top-left at `pos`, wrapping at `max_w`.
    pub fn text_at(&mut self, s: &str, style: &TextStyle, pos: Vec2, max_w: f64, color: Color) -> TextMetrics {
        self.quads.clear();
        let m = self.text.place(s, style, pos.x as f32, pos.y as f32, max_w as f32, color.to_gpu(), &mut self.quads);
        self.draw.glyphs(&self.quads);
        m
    }

    /// Draw one line of `s`, left-aligned and vertically centred in `rect`,
    /// clipped to it.
    pub fn text_in_rect(&mut self, s: &str, style: &TextStyle, rect: Rect, color: Color) {
        let lh = style.line_height() as f64;
        let y = (rect.center().y - lh * 0.5).round();
        let clip = rect.intersection(&self.clip);
        self.draw.push_clip(clip);
        self.text_at(s, style, Vec2::new(rect.min.x, y), 1.0e6, color);
        self.draw.pop_clip();
    }

    /// Draw one line of `s` centred both ways in `rect`.
    pub fn text_centered(&mut self, s: &str, style: &TextStyle, rect: Rect, color: Color) {
        let w = self.measure(s, style);
        let lh = style.line_height() as f64;
        let x = (rect.center().x - w * 0.5).round();
        let y = (rect.center().y - lh * 0.5).round();
        let clip = rect.intersection(&self.clip);
        self.draw.push_clip(clip);
        self.text_at(s, style, Vec2::new(x, y), 1.0e6, color);
        self.draw.pop_clip();
    }

    /// Draw one line of `s` right-aligned and vertically centred in `rect`.
    pub fn text_right(&mut self, s: &str, style: &TextStyle, rect: Rect, color: Color) {
        let w = self.measure(s, style);
        let lh = style.line_height() as f64;
        let x = (rect.max.x - w).round();
        let y = (rect.center().y - lh * 0.5).round();
        let clip = rect.intersection(&self.clip);
        self.draw.push_clip(clip);
        self.text_at(s, style, Vec2::new(x, y), 1.0e6, color);
        self.draw.pop_clip();
    }

    // ---- shapes ---------------------------------------------------------

    pub fn fill(&mut self, rect: Rect, color: Color) {
        self.draw.rounded_rect(rect, self.m.radius, color);
    }

    /// Rounded rect shaded from the theme's top to bottom of `base`.
    pub fn fill_shaded(&mut self, rect: Rect, base: Color) {
        let (t, b) = (self.theme.top(base), self.theme.bottom(base));
        self.draw.rounded_rect_gradient(rect, self.m.radius, t, b);
    }

    /// One-pixel bevel: a bright stroke along the top half and a dark stroke
    /// along the bottom half of the rounded rect, so edges follow the corners.
    fn bevel(&mut self, rect: Rect, radius: f64, top: Color, bottom: Color) {
        let inner = rect.shrink(self.m.border);
        let mid = rect.center().y;
        let clip = self.clip;
        self.draw.push_clip(Rect::new(rect.min, Vec2::new(rect.max.x, mid)).intersection(&clip));
        self.draw.stroke_rect(inner, self.m.border, (radius - self.m.border).max(0.0), top);
        self.draw.pop_clip();
        self.draw.push_clip(Rect::new(Vec2::new(rect.min.x, mid), rect.max).intersection(&clip));
        self.draw.stroke_rect(inner, self.m.border, (radius - self.m.border).max(0.0), bottom);
        self.draw.pop_clip();
    }

    /// A raised control: shaded fill, light top edge, dark bottom edge, dark
    /// outline. `pressed` sinks it (darker, shading inverted).
    pub fn raised(&mut self, rect: Rect, base: Color, pressed: bool) {
        let r = self.m.radius;
        let t = self.theme;
        if pressed {
            let base = base.scale_rgb(0.85);
            self.draw.rounded_rect_gradient(rect, r, t.bottom(base), t.top(base));
            self.bevel(rect, r, t.shade(base), t.top(base));
        } else {
            self.draw.rounded_rect_gradient(rect, r, t.top(base), t.bottom(base));
            self.bevel(rect, r, t.highlight(base), t.shade(base));
        }
        self.draw.stroke_rect(rect, self.m.border, r, t.border_dark);
    }

    /// A recessed well: dark fill, inner shadow at the top, faint light edge
    /// at the bottom, dark outline.
    pub fn recessed(&mut self, rect: Rect, base: Color) {
        let r = self.m.radius;
        let t = self.theme;
        self.draw.rounded_rect_gradient(rect, r, t.shade(base), base);
        self.bevel(rect, r, t.shade(base).scale_rgb(0.8), t.border_light.fade(0.35));
        self.draw.stroke_rect(rect, self.m.border, r, t.border_dark);
    }

    /// A floating panel (menus, popups): shaded fill, light top edge, dark
    /// outline, soft shadow underneath.
    pub fn floating_panel(&mut self, rect: Rect, base: Color) {
        let r = self.m.radius;
        let t = self.theme;
        self.draw.shadow(rect.translate(Vec2::new(0.0, self.m.px(4.0))), r, self.m.px(12.0), Color::BLACK.fade(0.55));
        self.draw.rounded_rect_gradient(rect, r, t.top(base), t.bottom(base));
        self.bevel(rect, r, t.highlight(base), t.shade(base));
        self.draw.stroke_rect(rect, self.m.border, r, t.border_dark);
    }

    /// Two-tone etched line across `rect`'s vertical centre.
    pub fn etched_line(&mut self, x0: f64, x1: f64, y: f64) {
        let b = self.m.border;
        self.draw.hline(x0, x1, y, b, self.theme.border_dark);
        self.draw.hline(x0, x1, y + b, b, self.theme.border_light.fade(0.5));
    }

    pub fn fill_square(&mut self, rect: Rect, color: Color) {
        self.draw.rect(rect, color);
    }

    pub fn outline(&mut self, rect: Rect, width: f64, color: Color) {
        self.draw.stroke_rect(rect, width, self.m.radius, color);
    }

    pub fn hline(&mut self, y: f64, x0: f64, x1: f64, color: Color) {
        self.draw.hline(x0, x1, y, self.m.border, color);
    }
}
