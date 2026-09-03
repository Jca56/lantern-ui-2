//! Keyboard focus for widgets: the Tab order, Enter and Space as clicks,
//! arrow keys as steps, and the focus ring.

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::id::WidgetId;
use crate::ui::{Response, Ui};

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

impl Ui<'_> {
    // ---- keyboard focus ---------------------------------------------------

    /// Register `id` (laid out at `rect`) as a stop on the Tab order and
    /// report whether it has keyboard focus. Call after [`Self::interact`]:
    /// a press on the widget focuses it too.
    pub fn focusable(&mut self, id: WidgetId, rect: Rect) -> bool {
        self.state.focus_order.push(id);
        if self.state.pressed && self.state.active == Some(id) {
            self.state.focus = Some(id);
        }
        let focused = self.state.focus == Some(id);
        if focused {
            self.state.focus_rect = Some(rect);
        }
        focused
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
}
