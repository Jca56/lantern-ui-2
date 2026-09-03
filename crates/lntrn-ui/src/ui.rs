//! The immediate-mode context: lays widgets out top to bottom (or left to
//! right inside a row), hit-tests them against this frame's input, and draws
//! into the shared `DrawList`.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_render::DrawList;
use lntrn_text::{GlyphQuad, TextEngine, TextMetrics, TextStyle};

use crate::event::Key;
use crate::id::WidgetId;
use crate::state::UiState;
use crate::theme::{Metrics, Theme};

/// Pass as a width to take all available room.
pub const FILL: f64 = f64::INFINITY;

/// Tooltips draw above popups.
const TOOLTIP_LAYER: usize = 3;

/// What kinds of input a widget reacts to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sense {
    pub click: bool,
    pub drag: bool,
    pub focus: bool,
}

impl Sense {
    pub const NONE: Sense = Sense { click: false, drag: false, focus: false };
    pub const CLICK: Sense = Sense { click: true, drag: false, focus: false };
    pub const DRAG: Sense = Sense { click: true, drag: true, focus: false };
    pub const FOCUS: Sense = Sense { click: true, drag: true, focus: true };
}

/// What the arrow keys asked of a focused value (see [`Ui::key_step`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyStep {
    #[default]
    None,
    /// Net steps: positive is more.
    By(i32),
    Min,
    Max,
}

/// What happened to a widget this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    pub id: WidgetId,
    pub rect: Rect,
    pub hovered: bool,
    /// The pointer went down on this widget and is still down.
    pub held: bool,
    pub pressed: bool,
    pub clicked: bool,
    pub double_clicked: bool,
    pub released: bool,
    pub dragging: bool,
    pub drag_delta: Vec2,
    pub focused: bool,
}

impl Default for WidgetId {
    fn default() -> Self {
        WidgetId::ROOT
    }
}

struct RowState {
    x: f64,
    y: f64,
    max_h: f64,
    remaining: f64,
}

pub struct Ui<'a> {
    pub draw: &'a mut DrawList,
    pub text: &'a mut TextEngine,
    pub theme: &'a Theme,
    pub m: Metrics,
    pub state: &'a mut UiState,
    clip: Rect,
    cursor: Vec2,
    avail_w: f64,
    max_y: f64,
    row: Option<RowState>,
    ids: Vec<WidgetId>,
    layer: usize,
    window: Rect,
    quads: Vec<GlyphQuad>,
}

impl<'a> Ui<'a> {
    /// Start laying out inside `content`, clipped to `clip`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draw: &'a mut DrawList,
        text: &'a mut TextEngine,
        theme: &'a Theme,
        m: Metrics,
        state: &'a mut UiState,
        content: Rect,
        clip: Rect,
        id: WidgetId,
        layer: usize,
    ) -> Self {
        draw.set_layer(layer);
        draw.push_clip_absolute(clip);
        Self {
            draw,
            text,
            theme,
            m,
            state,
            clip,
            cursor: content.min,
            avail_w: content.width(),
            max_y: content.min.y,
            row: None,
            ids: vec![id],
            layer,
            window: Rect::from_min_size(Vec2::ZERO, Vec2::new(1.0e6, 1.0e6)),
            quads: Vec::new(),
        }
    }

    /// Tell the context how big the window is (popups stay inside it).
    pub fn set_window_rect(&mut self, window: Rect) {
        self.window = window;
    }

    /// Finish: pops the clip pushed by `new`. Returns the content bottom.
    pub fn finish(self) -> f64 {
        self.draw.pop_clip();
        self.max_y
    }

    // ---- identity -------------------------------------------------------

    pub fn id(&self, label: &str) -> WidgetId {
        self.ids.last().copied().unwrap_or(WidgetId::ROOT).with(label)
    }

    pub fn push_id(&mut self, label: &str) {
        let id = self.id(label);
        self.ids.push(id);
    }

    pub fn push_index(&mut self, i: usize) {
        let id = self.ids.last().copied().unwrap_or(WidgetId::ROOT).with_index(i);
        self.ids.push(id);
    }

    pub fn pop_id(&mut self) {
        if self.ids.len() > 1 {
            self.ids.pop();
        }
    }

    // ---- geometry -------------------------------------------------------

    pub fn clip(&self) -> Rect {
        self.clip
    }

    pub fn cursor(&self) -> Vec2 {
        self.cursor
    }

    pub fn avail_width(&self) -> f64 {
        match &self.row {
            Some(r) => r.remaining.max(0.0),
            None => self.avail_w,
        }
    }

    pub fn remaining_height(&self) -> f64 {
        (self.clip.max.y - self.cursor.y).max(0.0)
    }

    pub fn layer(&self) -> usize {
        self.layer
    }

    /// Inside a `row(...)` call: widgets size to content instead of filling.
    pub fn in_row(&self) -> bool {
        self.row.is_some()
    }

    /// Reserve space for a widget. `size.x == FILL` takes the available width.
    pub fn alloc(&mut self, size: Vec2) -> Rect {
        let gap = self.m.gap;
        match &mut self.row {
            Some(row) => {
                let w = if size.x == FILL { row.remaining.max(0.0) } else { size.x };
                let r = Rect::from_min_size(Vec2::new(row.x, row.y), Vec2::new(w, size.y));
                row.x += w + gap;
                row.remaining -= w + gap;
                row.max_h = row.max_h.max(size.y);
                self.max_y = self.max_y.max(r.max.y);
                r
            }
            None => {
                let w = if size.x == FILL { self.avail_w } else { size.x.min(self.avail_w) };
                let r = Rect::from_min_size(self.cursor, Vec2::new(w, size.y));
                self.cursor.y += size.y + gap;
                self.max_y = self.max_y.max(r.max.y);
                r
            }
        }
    }

    /// Lay the widgets declared in `f` left to right on one line.
    pub fn row(&mut self, f: impl FnOnce(&mut Ui)) {
        debug_assert!(self.row.is_none(), "rows do not nest");
        self.row = Some(RowState { x: self.cursor.x, y: self.cursor.y, max_h: 0.0, remaining: self.avail_w });
        f(self);
        let row = self.row.take().expect("row state");
        if row.max_h > 0.0 {
            self.cursor.y += row.max_h + self.m.gap;
        }
    }

    /// A label column followed by whatever `f` declares, on one line.
    pub fn labelled(&mut self, label: &str, f: impl FnOnce(&mut Ui)) {
        let label_w = self.m.label_w.min(self.avail_w * 0.5);
        let r = Rect::from_min_size(self.cursor, Vec2::new(label_w, self.m.widget_h));
        let style = self.text_style();
        self.text_in_rect(label, &style, r, self.theme.text_dim);
        let saved = (self.cursor.x, self.avail_w);
        self.cursor.x += label_w + self.m.gap;
        self.avail_w -= label_w + self.m.gap;
        let before = self.cursor.y;
        f(self);
        if self.cursor.y == before {
            // Nothing was declared; still advance past the label.
            self.cursor.y += self.m.widget_h + self.m.gap;
        }
        self.cursor.x = saved.0;
        self.avail_w = saved.1;
    }

    /// Shift everything after this point right by `amount`.
    pub fn indent(&mut self, amount: f64, f: impl FnOnce(&mut Ui)) {
        let saved = (self.cursor.x, self.avail_w);
        self.cursor.x += amount;
        self.avail_w -= amount;
        f(self);
        self.cursor.x = saved.0;
        self.avail_w = saved.1;
    }

    pub fn space(&mut self, h: f64) {
        self.cursor.y += h;
        self.max_y = self.max_y.max(self.cursor.y);
    }

    /// Side-by-side columns from the same top. `widths` are pixels, or
    /// [`FILL`] to share what is left; `f(ui, i)` declares column `i`.
    /// Layout continues below the tallest column.
    pub fn columns(&mut self, widths: &[f64], mut f: impl FnMut(&mut Ui, usize)) {
        if widths.is_empty() {
            return;
        }
        let gap = self.m.gap;
        let fixed: f64 = widths.iter().filter(|w| **w != FILL).sum();
        let fills = widths.iter().filter(|w| **w == FILL).count();
        let fill_w = if fills == 0 { 0.0 } else { ((self.avail_w - fixed - gap * (widths.len() as f64 - 1.0)) / fills as f64).max(0.0) };
        let (top, saved_w) = (self.cursor, self.avail_w);
        let mut bottom = top.y;
        let mut x = top.x;
        for (i, w) in widths.iter().enumerate() {
            let w = if *w == FILL { fill_w } else { *w };
            self.cursor = Vec2::new(x, top.y);
            self.avail_w = w;
            self.push_index(i);
            f(self, i);
            self.pop_id();
            bottom = bottom.max(self.cursor.y);
            x += w + gap;
        }
        self.cursor = Vec2::new(top.x, bottom);
        self.avail_w = saved_w;
        self.max_y = self.max_y.max(bottom);
    }

    // ---- time -------------------------------------------------------------

    /// Seconds since the app started, as of this rebuild.
    pub fn now(&self) -> f64 {
        self.state.now
    }

    /// Ease a per-widget value toward `target`, settling in about `seconds`.
    /// Keeps asking for rebuilds while it moves, so hover fades and sliding
    /// panels cost nothing once they rest. A fresh slot starts at `target`.
    pub fn animate(&mut self, id: WidgetId, target: f64, seconds: f64) -> f64 {
        let now = self.state.now;
        let a = self.state.anim(id, target);
        // A long idle must not turn into one huge step.
        let dt = (now - a.time).clamp(0.0, 0.1);
        a.time = now;
        if seconds <= 0.0 {
            a.value = target;
            return target;
        }
        let k = 1.0 - (-dt * 4.0 / seconds).exp();
        a.value += (target - a.value) * k;
        let settled = (a.value - target).abs() < 0.001;
        if settled {
            a.value = target;
        }
        let v = a.value;
        if !settled {
            self.state.request_redraw_after(1.0 / 60.0);
        }
        v
    }

    // ---- keyboard focus ---------------------------------------------------

    /// Register `id` as a stop on the Tab order and report whether it has
    /// keyboard focus. Call after [`Self::interact`]: a press on the widget
    /// focuses it too.
    pub fn focusable(&mut self, id: WidgetId) -> bool {
        self.state.focus_order.push(id);
        if self.state.pressed && self.state.active == Some(id) {
            self.state.focus = Some(id);
        }
        self.state.focus == Some(id)
    }

    /// Enter or Space on the keyboard-focused widget counts as a click.
    pub fn key_click(&mut self, id: WidgetId, r: &mut Response) {
        if self.state.focus == Some(id) && self.state.take_key(|k| matches!(k.key, Key::Enter | Key::Space) && k.mods.is_empty()).is_some() {
            r.clicked = true;
            self.state.request_rebuild = true;
        }
    }

    /// The focus ring around `rect`, when `id` has keyboard focus and the
    /// user is navigating by keyboard.
    pub fn focus_ring(&mut self, id: WidgetId, rect: Rect) {
        if self.state.focus == Some(id) && self.state.focus_visible {
            let w = self.m.px(2.0);
            self.draw.stroke_rect(rect.expand(w), w, self.m.radius + w, self.theme.focus);
        }
    }

    /// Arrow keys on the keyboard-focused widget this frame: Up/Right is
    /// more, Down/Left is less, Home and End go to the ends.
    pub fn key_step(&mut self, id: WidgetId) -> KeyStep {
        if self.state.focus != Some(id) {
            return KeyStep::None;
        }
        let mut by = 0;
        let mut end = KeyStep::None;
        while let Some(k) = self.state.take_key(|k| matches!(k.key, Key::ArrowUp | Key::ArrowRight | Key::ArrowDown | Key::ArrowLeft | Key::Home | Key::End)) {
            self.state.request_rebuild = true;
            match k.key {
                Key::ArrowUp | Key::ArrowRight => by += 1,
                Key::ArrowDown | Key::ArrowLeft => by -= 1,
                Key::Home => end = KeyStep::Min,
                _ => end = KeyStep::Max,
            }
        }
        match end {
            KeyStep::None if by != 0 => KeyStep::By(by),
            other => other,
        }
    }

    /// Move the layout origin (scroll areas).
    pub(crate) fn set_cursor(&mut self, p: Vec2) {
        self.cursor = p;
        self.max_y = self.max_y.max(p.y);
    }

    pub(crate) fn set_avail_width(&mut self, w: f64) {
        self.avail_w = w;
    }

    pub(crate) fn set_clip(&mut self, clip: Rect) {
        self.clip = clip;
    }

    pub(crate) fn set_layer_internal(&mut self, layer: usize) {
        self.layer = layer;
    }

    /// Whole-window rect, for placing popups.
    pub(crate) fn state_window_rect(&self) -> Rect {
        self.window
    }

    // ---- interaction ----------------------------------------------------

    /// Hit-test `rect` for `id` against this frame's input.
    pub fn interact(&mut self, id: WidgetId, rect: Rect, sense: Sense) -> Response {
        let st = &mut *self.state;
        if st.record_rects {
            st.rects.insert(id, rect);
        }
        let visible = rect.intersection(&self.clip);
        let blocked = st
            .popup
            .is_some_and(|(prect, layer)| layer > self.layer && prect.contains(st.pointer));
        let over = st.pointer_in_window && !blocked && visible.contains(st.pointer);
        let hovered = over && (st.active.is_none() || st.active == Some(id));
        if hovered {
            st.hot = Some(id);
        }
        let mut r = Response { id, rect, hovered, ..Response::default() };
        let wants = sense.click || sense.drag || sense.focus;
        if wants && st.pressed && !st.press_claimed && !blocked && visible.contains(st.press_pos) {
            st.press_claimed = true;
            st.active = Some(id);
            st.focus = if sense.focus { Some(id) } else { None };
            r.pressed = true;
            r.double_clicked = st.double_click;
        }
        if st.active == Some(id) {
            r.held = st.down;
            if st.released {
                r.released = true;
                r.clicked = sense.click && over;
            }
            if sense.drag && st.down {
                r.dragging = true;
                r.drag_delta = if r.pressed { st.pointer - st.press_pos } else { st.delta };
            }
        }
        r.focused = st.focus == Some(id);
        r
    }

    /// Base color for a raised control in its current role (pressed is
    /// handled by [`Self::raised`]).
    pub fn widget_color(&self, r: &Response) -> Color {
        if r.hovered && !r.held { self.theme.hover(self.theme.widget) } else { self.theme.widget }
    }

    /// Draw a button face for `r` over `rect`, glowing while hovered.
    pub fn button_face(&mut self, rect: Rect, r: &Response) {
        if r.hovered && !r.held {
            self.hover_glow(rect, self.theme.accent);
        }
        let base = self.widget_color(r);
        self.raised(rect, base, r.held);
    }

    /// A glow in `color` around a hovered control: a wide soft halo and a
    /// bright rim just outside the edge. Draw it before the face so the face
    /// sits on top.
    pub fn hover_glow(&mut self, rect: Rect, color: Color) {
        let r = self.m.radius;
        let rim = self.m.px(2.0);
        self.draw.shadow(rect.expand(self.m.px(3.0)), r, self.m.px(18.0), color.fade(0.95));
        self.draw.stroke_rect(rect.expand(rim), rim, r + rim, color);
    }

    /// Show `text` under the control `r` while Alt is held and the pointer is
    /// on it: help on demand, never in the way (Alva).
    pub fn tooltip(&mut self, r: &Response, text: &str) {
        if text.is_empty() || !r.hovered || !self.state.mods.alt() {
            return;
        }
        let style = self.text_style();
        let (pad, gap) = (self.m.pad, self.m.gap);
        let size = Vec2::new(self.measure(text, &style) + pad * 2.0, self.m.widget_h);
        let win = self.window;
        let x = r.rect.min.x.min(win.max.x - size.x - gap).max(win.min.x + gap);
        let below = r.rect.max.y + gap;
        let y = if below + size.y > win.max.y { r.rect.min.y - gap - size.y } else { below };
        let rect = Rect::from_min_size(Vec2::new(x, y), size);
        let (saved_layer, saved_clip) = (self.draw.layer(), self.clip);
        self.draw.set_layer(TOOLTIP_LAYER);
        self.draw.push_clip_absolute(win);
        self.clip = win;
        self.floating_panel(rect, self.theme.header);
        self.text_centered(text, &style, rect, self.theme.text);
        self.clip = saved_clip;
        self.draw.pop_clip();
        self.draw.set_layer(saved_layer);
    }

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
