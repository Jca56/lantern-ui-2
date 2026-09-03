//! The immediate-mode context: lays widgets out top to bottom (or left to
//! right inside a row), hit-tests them against this frame's input, and draws
//! into the shared `DrawList`. Text and shape painting live in
//! [`crate::paint`], keyboard focus in [`crate::focus`], the widgets in
//! [`crate::widgets`].

use lntrn_math::{Color, Rect, Vec2};
use lntrn_render::{DrawList, ImageHandle};
use lntrn_text::{GlyphQuad, TextEngine};

pub use crate::focus::KeyStep;
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

pub(crate) struct RowState {
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
    pub(crate) clip: Rect,
    pub(crate) cursor: Vec2,
    pub(crate) avail_w: f64,
    pub(crate) max_y: f64,
    pub(crate) row: Option<RowState>,
    pub(crate) ids: Vec<WidgetId>,
    pub(crate) layer: usize,
    pub(crate) window: Rect,
    pub(crate) quads: Vec<GlyphQuad>,
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

    // ---- pictures ---------------------------------------------------------

    /// Draw `image` at `size` logical pixels (the available width when
    /// `size.x` is [`FILL`], keeping the aspect).
    pub fn image(&mut self, image: ImageHandle, size: Vec2) -> Rect {
        let size = if size.x == FILL { Vec2::new(self.avail_width(), self.avail_width() / image.aspect()) } else { Vec2::new(self.m.px(size.x), self.m.px(size.y)) };
        let rect = self.alloc(size);
        self.draw.image(rect, image, self.m.radius, Color::WHITE);
        rect
    }

    /// Draw `image` as large as fits inside `max` logical pixels without
    /// stretching, centred in that box.
    pub fn image_fit(&mut self, image: ImageHandle, max: Vec2) -> Rect {
        let max = Vec2::new(if max.x == FILL { self.avail_width() } else { self.m.px(max.x) }, self.m.px(max.y));
        let scale = (max.x / image.width.max(1) as f64).min(max.y / image.height.max(1) as f64);
        let size = Vec2::new((image.width as f64 * scale).round(), (image.height as f64 * scale).round());
        let cell = self.alloc(Vec2::new(if self.in_row() { size.x } else { FILL }, size.y));
        let rect = Rect::from_center_size(cell.center(), size).round();
        self.draw.image(rect, image, self.m.radius, Color::WHITE);
        rect
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

}
