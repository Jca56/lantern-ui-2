//! A headless harness: run widget code frame by frame with synthetic input
//! and no window. Every [`Ui::interact`] rect is recorded, so a test can
//! click a button by the id its label produces.
//!
//! ```ignore
//! let mut h = Harness::new(800.0, 600.0);
//! let mut on = false;
//! h.click_on(WidgetId::ROOT.with("Snap"), |ui| { ui.toggle("Snap", &mut on); });
//! assert!(on);
//! ```
//!
//! Fonts come from the machine's font directories, like in the app.

use std::collections::HashMap;

use lntrn_math::{Rect, Vec2};
use lntrn_render::DrawList;
use lntrn_text::TextEngine;

use crate::event::{Event, Key, Modifiers, MouseButton, WheelDelta};
use crate::host::Host;
use crate::id::WidgetId;
use crate::shell::{Shell, ShellOutput, WindowState};
use crate::state::UiState;
use crate::theme::{Metrics, Theme};
use crate::ui::Ui;

/// What a frame reported back.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameInfo {
    /// A widget asked for another rebuild.
    pub rebuild_again: bool,
    /// Where layout ended (the content bottom).
    pub bottom: f64,
    pub vertices: usize,
}

pub struct Harness {
    pub text: TextEngine,
    pub draw: DrawList,
    pub state: UiState,
    pub theme: Theme,
    pub scale: f64,
    window: Rect,
    pointer: Vec2,
    mods: Modifiers,
    events: Vec<Event>,
    /// Rects of the last frame, kept after the state clears its own.
    rects: HashMap<WidgetId, Rect>,
}

impl Harness {
    /// A window of `width` × `height` physical pixels at scale 1.
    pub fn new(width: f64, height: f64) -> Self {
        let mut state = UiState::new();
        state.record_rects = true;
        Self {
            text: TextEngine::new("Inter", "JetBrains Mono"),
            draw: DrawList::new(),
            state,
            theme: Theme::default(),
            scale: 1.0,
            window: Rect::from_min_size(Vec2::ZERO, Vec2::new(width, height)),
            pointer: Vec2::new(-1.0, -1.0),
            mods: Modifiers::NONE,
            events: Vec::new(),
            rects: HashMap::new(),
        }
    }

    pub fn window(&self) -> Rect {
        self.window
    }

    pub fn metrics(&self) -> Metrics {
        self.theme.metrics(self.scale)
    }

    pub fn pointer(&self) -> Vec2 {
        self.pointer
    }

    /// The rect a widget occupied in the last frame.
    pub fn rect_of(&self, id: WidgetId) -> Option<Rect> {
        self.rects.get(&id).copied()
    }

    /// Rect of a top-level widget by its label (`WidgetId::ROOT.with(label)`).
    pub fn rect_of_label(&self, label: &str) -> Option<Rect> {
        self.rect_of(WidgetId::ROOT.with(label))
    }

    // ---- queue input for the next frame ----------------------------------

    pub fn event(&mut self, ev: Event) {
        self.events.push(ev);
    }

    pub fn move_to(&mut self, p: Vec2) {
        self.pointer = p;
        self.events.push(Event::PointerMoved(p));
    }

    pub fn press(&mut self) {
        self.events.push(Event::Button { button: MouseButton::Left, pressed: true, pos: self.pointer, mods: self.mods });
    }

    pub fn release(&mut self) {
        self.events.push(Event::Button { button: MouseButton::Left, pressed: false, pos: self.pointer, mods: self.mods });
    }

    pub fn right_press(&mut self) {
        self.events.push(Event::Button { button: MouseButton::Right, pressed: true, pos: self.pointer, mods: self.mods });
        self.events.push(Event::Button { button: MouseButton::Right, pressed: false, pos: self.pointer, mods: self.mods });
    }

    pub fn wheel(&mut self, lines: f64) {
        self.events.push(Event::Wheel { delta: WheelDelta::Lines(Vec2::new(0.0, lines)), pos: self.pointer, mods: self.mods });
    }

    pub fn set_mods(&mut self, mods: Modifiers) {
        self.mods = mods;
        self.events.push(Event::Modifiers(mods));
    }

    /// A key press (and release) with the current modifiers.
    pub fn key(&mut self, key: Key) {
        self.key_with(key, self.mods);
    }

    pub fn key_with(&mut self, key: Key, mods: Modifiers) {
        self.events.push(Event::Key { key, pressed: true, repeat: false, mods });
        self.events.push(Event::Key { key, pressed: false, repeat: false, mods });
    }

    /// Type `s` as the user would: one key press and its text per char.
    pub fn type_text(&mut self, s: &str) {
        for c in s.chars() {
            self.events.push(Event::Key { key: Key::Char(c), pressed: true, repeat: false, mods: self.mods });
            self.events.push(Event::Text(c.to_string()));
            self.events.push(Event::Key { key: Key::Char(c), pressed: false, repeat: false, mods: self.mods });
        }
    }

    // ---- run frames ---------------------------------------------------------

    /// One rebuild: fold the queued events in, run `f` inside a `Ui` that
    /// fills the window, settle the state.
    pub fn frame(&mut self, f: impl FnOnce(&mut Ui)) -> FrameInfo {
        let m = self.metrics();
        let events = std::mem::take(&mut self.events);
        self.state.begin_frame(&events, m.widget_h);
        self.draw.clear();
        let content = self.window.shrink(m.pad);
        let mut ui = Ui::new(&mut self.draw, &mut self.text, &self.theme, m, &mut self.state, content, self.window, WidgetId::ROOT, 0);
        ui.set_window_rect(self.window);
        f(&mut ui);
        let bottom = ui.finish();
        self.state.end_frame();
        self.rects = self.state.rects.clone();
        FrameInfo { rebuild_again: self.state.request_rebuild, bottom, vertices: self.draw.vertex_count() }
    }

    /// Run `f` until it stops asking for rebuilds (at most `max` frames).
    pub fn settle(&mut self, max: usize, mut f: impl FnMut(&mut Ui)) -> FrameInfo {
        let mut info = self.frame(&mut f);
        for _ in 1..max {
            if !info.rebuild_again {
                break;
            }
            info = self.frame(&mut f);
        }
        info
    }

    /// Press at `p` in one frame and release in the next, running `f` for
    /// both (and once more to settle), like a real click.
    pub fn click_at(&mut self, p: Vec2, mut f: impl FnMut(&mut Ui)) -> FrameInfo {
        self.move_to(p);
        self.press();
        self.frame(&mut f);
        self.release();
        self.settle(3, f)
    }

    /// Click the centre of the widget `id` as laid out by `f` (a layout
    /// frame runs first so the rect is known).
    pub fn click_on(&mut self, id: WidgetId, mut f: impl FnMut(&mut Ui)) -> FrameInfo {
        self.frame(&mut f);
        let rect = self.rect_of(id).unwrap_or_else(|| panic!("no widget with id {id:?} was laid out"));
        self.click_at(rect.center(), f)
    }

    /// Drag from `from` to `to` in `steps` motion frames.
    pub fn drag(&mut self, from: Vec2, to: Vec2, steps: usize, mut f: impl FnMut(&mut Ui)) -> FrameInfo {
        self.move_to(from);
        self.press();
        self.frame(&mut f);
        for i in 1..=steps.max(1) {
            let t = i as f64 / steps.max(1) as f64;
            self.move_to(from + (to - from) * t);
            self.frame(&mut f);
        }
        self.release();
        self.settle(3, f)
    }

    /// One shell rebuild with the queued events, like `lntrn-app` does.
    pub fn shell_frame<H: Host>(&mut self, shell: &mut Shell<H>, host: &mut H) -> ShellOutput {
        let events = std::mem::take(&mut self.events);
        self.draw.clear();
        shell.state.record_rects = true;
        let ws = WindowState { maximized: true, focused: true };
        let out = shell.frame(host, &events, self.window, self.scale, ws, &mut self.text, &mut self.draw);
        self.rects = shell.state.rects.clone();
        out
    }

    /// Shell rebuilds until the shell stops asking for more (at most `max`).
    /// `quit` and the window command are gathered across the rebuilds, as
    /// `lntrn-app` gathers them.
    pub fn shell_settle<H: Host>(&mut self, shell: &mut Shell<H>, host: &mut H, max: usize) -> ShellOutput {
        let mut out = self.shell_frame(shell, host);
        let (mut quit, mut command) = (out.quit, out.window_command);
        for _ in 1..max {
            if !out.rebuild_again {
                break;
            }
            out = self.shell_frame(shell, host);
            quit |= out.quit;
            command = command.or(out.window_command);
        }
        ShellOutput { quit, window_command: command, ..out }
    }
}
