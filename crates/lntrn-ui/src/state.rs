//! Cross-frame UI state: pointer and keyboard input gathered for one rebuild,
//! which widget is hot/active/focused, open popups, and (in
//! [`crate::memory`]) per-widget memory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use lntrn_math::{Rect, Vec2};

use crate::event::{Event, Key, Modifiers, MouseButton};
use crate::id::WidgetId;
pub use crate::memory::{AnimMem, History, Snapshot};
use crate::memory::{Mem, MemKind};

/// Two presses closer than this (and within 6px) are a double click.
const DOUBLE_CLICK_SECS: f64 = 0.4;

/// What the pointer should look like, decided by whatever is under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    Grabbing,
}

/// A key press delivered to the focused widget this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyPress {
    pub key: Key,
    pub mods: Modifiers,
    pub repeat: bool,
    /// Position in this frame's event stream, shared with
    /// [`UiState::text_input`], so editors replay keys and text in order.
    pub seq: u32,
}

impl KeyPress {
    /// As the event a [`crate::keymap::Trigger`] matches against.
    pub fn to_event(self) -> Event {
        Event::Key { key: self.key, pressed: true, repeat: self.repeat, mods: self.mods }
    }
}

/// Caret and selection of a text field, in byte offsets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdit {
    pub cursor: usize,
    pub anchor: usize,
    /// Horizontal scroll so the caret stays visible.
    pub scroll: f64,
    /// While editing a number field: the text being typed.
    pub buffer: Option<String>,
}

impl TextEdit {
    pub fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }
    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScrollMem {
    pub offset: f64,
    /// Content height measured last frame.
    pub content: f64,
}

pub struct UiState {
    // ---- pointer ----
    pub pointer: Vec2,
    pub pointer_in_window: bool,
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    pub press_pos: Vec2,
    pub right_pressed: bool,
    pub middle_down: bool,
    pub middle_pressed: bool,
    pub middle_press_pos: Vec2,
    pub double_click: bool,
    /// Some widget took this frame's press. Unclaimed presses clear focus.
    pub press_claimed: bool,
    /// Pointer movement since the previous rebuild.
    pub delta: Vec2,
    /// Wheel movement this frame, in pixels (y > 0 = content scrolls up).
    pub wheel: Vec2,
    // ---- keyboard ----
    pub mods: Modifiers,
    pub keys: Vec<KeyPress>,
    /// Committed text this frame, each piece with its place in the event
    /// stream (see [`KeyPress::seq`]).
    pub text_input: Vec<(u32, String)>,
    // ---- widget roles ----
    pub hot: Option<WidgetId>,
    pub active: Option<WidgetId>,
    pub focus: Option<WidgetId>,
    /// A popup (menu, dropdown list) currently open: its rect and layer.
    pub popup: Option<(Rect, usize)>,
    popup_seen: bool,
    pub cursor_icon: CursorIcon,
    /// Set by widgets that want another rebuild right after this one
    /// (e.g. a popup that just closed and needs the underlying UI to redraw).
    pub request_rebuild: bool,
    /// When set, every [`crate::Ui::interact`] call records its rect in
    /// [`Self::rects`] so tests (and tooling) can find widgets by id.
    pub record_rects: bool,
    /// Widget rects of this frame, when [`Self::record_rects`] is on.
    pub rects: HashMap<WidgetId, Rect>,
    /// Seconds since the state was made, as of this frame.
    pub now: f64,
    /// Rebuild again after this many seconds even with no input (an
    /// animation is running). `None`: sleep until input.
    pub wake_after: Option<f64>,
    /// The clipboard. Widgets read it to paste and write it through
    /// [`Self::set_clipboard`]; the harness pulls the system clipboard in
    /// before a paste and pushes ours out after a copy
    /// ([`Self::take_clipboard_dirty`]).
    pub clipboard: String,
    clipboard_dirty: bool,
    /// A file from outside is being dragged over the window: drop zones
    /// light up.
    pub hovering_files: bool,
    /// Files dropped on the window this frame. A widget under the pointer
    /// may take them ([`crate::Ui::drop_zone`]); whatever is left goes to
    /// [`crate::Host::dropped`].
    pub dropped_files: Vec<PathBuf>,
    /// An input method's composition in progress: the text and the
    /// composition caret (byte range into it). Text widgets show it inline
    /// at their caret; [`Event::Text`] commits it.
    pub ime_preedit: Option<(String, Option<(usize, usize)>)>,
    /// Where the focused text widget's caret is, so the input method can
    /// place its candidate window. Set each frame by the widget.
    pub ime_rect: Option<Rect>,
    /// Keyboard-focusable widgets of this frame in declaration order; Tab
    /// walks it (see [`crate::Ui::focusable`]).
    pub focus_order: Vec<WidgetId>,
    /// Draw focus rings: on after keyboard navigation, off again on a
    /// pointer press.
    pub focus_visible: bool,
    /// Where the keyboard-focused widget was laid out this frame.
    pub focus_rect: Option<Rect>,
    /// Focus moved by keyboard since the previous frame: scroll areas bring
    /// the new widget into view.
    pub focus_moved: bool,
    focus_moved_pending: bool,
    /// Time and place of the last press, for double clicks (frame clock).
    last_click: Option<(f64, Vec2)>,
    start: Instant,
    manual_time: Option<f64>,
    /// Per-widget memory, keyed by id and kind: a number field keeps both
    /// its typing buffer and its drag origin (see [`crate::memory`]).
    pub(crate) mem: HashMap<(WidgetId, MemKind), Mem>,
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    pub fn new() -> Self {
        Self {
            pointer: Vec2::new(-1.0, -1.0),
            pointer_in_window: false,
            down: false,
            pressed: false,
            released: false,
            press_pos: Vec2::ZERO,
            right_pressed: false,
            middle_down: false,
            middle_pressed: false,
            middle_press_pos: Vec2::ZERO,
            double_click: false,
            press_claimed: false,
            delta: Vec2::ZERO,
            wheel: Vec2::ZERO,
            mods: Modifiers::NONE,
            keys: Vec::new(),
            text_input: Vec::new(),
            hot: None,
            active: None,
            focus: None,
            popup: None,
            popup_seen: false,
            cursor_icon: CursorIcon::Default,
            request_rebuild: false,
            record_rects: false,
            rects: HashMap::new(),
            now: 0.0,
            wake_after: None,
            clipboard: String::new(),
            clipboard_dirty: false,
            hovering_files: false,
            dropped_files: Vec::new(),
            ime_preedit: None,
            ime_rect: None,
            focus_order: Vec::new(),
            focus_visible: false,
            focus_rect: None,
            focus_moved: false,
            focus_moved_pending: false,
            last_click: None,
            start: Instant::now(),
            manual_time: None,
            mem: HashMap::new(),
        }
    }

    /// Drive the clock by hand (tests): time no longer follows the wall.
    pub fn set_time(&mut self, seconds: f64) {
        self.manual_time = Some(seconds);
    }

    /// Ask for a rebuild in `seconds` even without input. The soonest
    /// request wins.
    pub fn request_redraw_after(&mut self, seconds: f64) {
        let s = seconds.max(0.0);
        self.wake_after = Some(self.wake_after.map_or(s, |w| w.min(s)));
    }

    /// Put `text` on the clipboard (a copy or cut). The harness pushes it
    /// to the system clipboard after the frame.
    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.clipboard = text.into();
        self.clipboard_dirty = true;
    }

    /// Whether a widget wrote the clipboard since the last call.
    pub fn take_clipboard_dirty(&mut self) -> bool {
        std::mem::take(&mut self.clipboard_dirty)
    }

    /// Take this frame's dropped files, leaving none for anyone after.
    pub fn take_dropped_files(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.dropped_files)
    }

    /// Fold this frame's events into the state. `line_px` converts wheel
    /// notches to pixels.
    pub fn begin_frame(&mut self, events: &[Event], line_px: f64) {
        self.pressed = false;
        self.released = false;
        self.right_pressed = false;
        self.middle_pressed = false;
        self.double_click = false;
        self.press_claimed = false;
        self.wheel = Vec2::ZERO;
        self.keys.clear();
        self.text_input.clear();
        self.hot = None;
        self.cursor_icon = CursorIcon::Default;
        self.request_rebuild = false;
        self.popup_seen = false;
        self.rects.clear();
        self.focus_order.clear();
        self.focus_rect = None;
        self.focus_moved = std::mem::take(&mut self.focus_moved_pending);
        self.wake_after = None;
        self.dropped_files.clear();
        self.ime_rect = None;
        self.now = self.manual_time.unwrap_or_else(|| self.start.elapsed().as_secs_f64());
        let start = self.pointer;
        for (seq, ev) in events.iter().enumerate() {
            let seq = seq as u32;
            match ev {
                Event::PointerMoved(p) => {
                    self.pointer = *p;
                    self.pointer_in_window = true;
                }
                Event::PointerLeft => self.pointer_in_window = false,
                Event::Button { button: MouseButton::Left, pressed: true, pos, .. } => {
                    self.down = true;
                    self.pressed = true;
                    self.press_visible_reset();
                    self.press_pos = *pos;
                    let now = self.now;
                    self.double_click = self.last_click.is_some_and(|(t, p)| now - t < DOUBLE_CLICK_SECS && p.distance(*pos) < 6.0);
                    self.last_click = if self.double_click { None } else { Some((now, *pos)) };
                }
                Event::Button { button: MouseButton::Left, pressed: false, .. } => {
                    self.down = false;
                    self.released = true;
                }
                Event::Button { button: MouseButton::Right, pressed: true, .. } => self.right_pressed = true,
                Event::Button { button: MouseButton::Middle, pressed, pos, .. } => {
                    self.middle_down = *pressed;
                    if *pressed {
                        self.middle_pressed = true;
                        self.middle_press_pos = *pos;
                    }
                }
                Event::Wheel { delta, .. } => self.wheel += delta.to_pixels(line_px),
                Event::Modifiers(m) => self.mods = *m,
                Event::Key { key, pressed: true, repeat, mods } => {
                    self.keys.push(KeyPress { key: *key, mods: *mods, repeat: *repeat, seq });
                }
                Event::Text(t) => {
                    self.text_input.push((seq, t.clone()));
                    // A commit ends the composition it came from.
                    self.ime_preedit = None;
                }
                Event::ImePreedit { text, cursor } => {
                    self.ime_preedit = if text.is_empty() { None } else { Some((text.clone(), *cursor)) };
                }
                Event::FileHovered(_) => self.hovering_files = true,
                Event::FileHoverLeft => self.hovering_files = false,
                Event::FileDropped(p) => {
                    self.hovering_files = false;
                    self.dropped_files.push(p.clone());
                }
                Event::Focus(false) => {
                    self.down = false;
                    self.middle_down = false;
                    self.active = None;
                    self.hovering_files = false;
                }
                _ => {}
            }
        }
        self.delta = self.pointer - start;
    }

    fn press_visible_reset(&mut self) {
        self.focus_visible = false;
    }

    /// Settle roles after every widget has run. A Tab nobody consumed moves
    /// keyboard focus along [`Self::focus_order`] (Shift+Tab backwards).
    pub fn end_frame(&mut self) {
        if self.pressed && !self.press_claimed {
            self.focus = None;
        }
        if self.released {
            self.active = None;
        }
        if !self.popup_seen {
            self.popup = None;
        }
        if let Some(tab) = self.take_key(|k| k.key == Key::Tab && !k.mods.ctrl() && !k.mods.alt()) {
            let n = self.focus_order.len();
            if n > 0 {
                let cur = self.focus.and_then(|f| self.focus_order.iter().position(|&x| x == f));
                let next = match (cur, tab.mods.shift()) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(c), false) => (c + 1) % n,
                    (Some(c), true) => (c + n - 1) % n,
                };
                self.focus = Some(self.focus_order[next]);
                self.focus_visible = true;
                self.focus_moved_pending = true;
                self.request_rebuild = true;
            }
        }
    }

    /// Called by a popup while it is open so it survives the frame.
    pub fn keep_popup(&mut self, rect: Rect, layer: usize) {
        self.popup = Some((rect, layer));
        self.popup_seen = true;
    }

    /// Consume a key press matching `pred`, if one arrived this frame.
    pub fn take_key(&mut self, pred: impl Fn(&KeyPress) -> bool) -> Option<KeyPress> {
        let i = self.keys.iter().position(pred)?;
        Some(self.keys.remove(i))
    }

    pub fn is_active(&self, id: WidgetId) -> bool {
        self.active == Some(id)
    }

    pub fn has_focus(&self, id: WidgetId) -> bool {
        self.focus == Some(id)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::WheelDelta;

    #[test]
    fn ingests_events() {
        let mut s = UiState::new();
        let p = Vec2::new(10.0, 20.0);
        s.begin_frame(
            &[
                Event::PointerMoved(p),
                Event::Button { button: MouseButton::Left, pressed: true, pos: p, mods: Modifiers::NONE },
                Event::Wheel { delta: WheelDelta::Lines(Vec2::new(0.0, 1.0)), pos: p, mods: Modifiers::NONE },
                Event::Key { key: Key::Enter, pressed: true, repeat: false, mods: Modifiers::NONE },
                Event::Text("a".into()),
            ],
            40.0,
        );
        assert!(s.pressed && s.down && !s.released);
        assert_eq!(s.press_pos, p);
        assert_eq!(s.wheel, Vec2::new(0.0, 40.0));
        assert_eq!(s.text_input, vec![(4, "a".to_owned())]);
        assert!(s.take_key(|k| k.key == Key::Enter).is_some());
        assert!(s.take_key(|k| k.key == Key::Enter).is_none());
        s.active = Some(WidgetId::ROOT);
        s.end_frame();
        assert!(s.is_active(WidgetId::ROOT), "still held");
        s.begin_frame(&[Event::Button { button: MouseButton::Left, pressed: false, pos: p, mods: Modifiers::NONE }], 40.0);
        assert!(s.released && !s.down);
        s.end_frame();
        assert_eq!(s.active, None);
    }

    #[test]
    fn double_click_and_popup_lifetime() {
        let mut s = UiState::new();
        let p = Vec2::ZERO;
        let press = Event::Button { button: MouseButton::Left, pressed: true, pos: p, mods: Modifiers::NONE };
        s.begin_frame(std::slice::from_ref(&press), 1.0);
        assert!(!s.double_click);
        s.begin_frame(&[press], 1.0);
        assert!(s.double_click);
        s.keep_popup(Rect::ZERO, 1);
        s.end_frame();
        assert!(s.popup.is_some());
        s.begin_frame(&[], 1.0);
        s.end_frame();
        assert!(s.popup.is_none(), "not kept → closed");
    }

    #[test]
    fn tab_walks_focus_order() {
        let mut s = UiState::new();
        let (a, b, c) = (WidgetId::ROOT.with("a"), WidgetId::ROOT.with("b"), WidgetId::ROOT.with("c"));
        let tab = |mods| Event::Key { key: Key::Tab, pressed: true, repeat: false, mods };
        s.begin_frame(&[tab(Modifiers::NONE)], 1.0);
        s.focus_order.extend([a, b, c]);
        s.end_frame();
        assert_eq!(s.focus, Some(a), "nothing focused: Tab goes to the first");
        assert!(s.focus_visible && s.request_rebuild);
        s.begin_frame(&[tab(Modifiers::SHIFT)], 1.0);
        s.focus_order.extend([a, b, c]);
        s.end_frame();
        assert_eq!(s.focus, Some(c), "Shift+Tab wraps backwards");
        s.begin_frame(&[tab(Modifiers::NONE)], 1.0);
        s.focus_order.extend([a, b, c]);
        s.end_frame();
        assert_eq!(s.focus, Some(a), "and Tab wraps forwards");
        s.begin_frame(&[Event::Button { button: MouseButton::Left, pressed: true, pos: Vec2::ZERO, mods: Modifiers::NONE }], 1.0);
        assert!(!s.focus_visible, "a pointer press hides the rings");
        s.set_time(5.0);
        s.begin_frame(&[], 1.0);
        assert_eq!(s.now, 5.0);
        s.request_redraw_after(0.5);
        s.request_redraw_after(0.2);
        s.request_redraw_after(0.9);
        assert_eq!(s.wake_after, Some(0.2), "the soonest wins");
    }

    #[test]
    fn clipboard_files_and_ime() {
        let mut s = UiState::new();
        assert!(!s.take_clipboard_dirty());
        s.set_clipboard("hello");
        assert_eq!(s.clipboard, "hello");
        assert!(s.take_clipboard_dirty());
        assert!(!s.take_clipboard_dirty(), "reported once");

        let p = PathBuf::from("/tmp/x.png");
        s.begin_frame(&[Event::FileHovered(p.clone())], 1.0);
        assert!(s.hovering_files);
        s.begin_frame(&[Event::FileDropped(p.clone())], 1.0);
        assert!(!s.hovering_files, "the drop ends the hover");
        assert_eq!(s.dropped_files, vec![p.clone()]);
        assert_eq!(s.take_dropped_files(), vec![p]);
        assert!(s.dropped_files.is_empty());
        s.begin_frame(&[Event::FileHovered(PathBuf::new()), Event::FileHoverLeft], 1.0);
        assert!(!s.hovering_files);

        s.begin_frame(&[Event::ImePreedit { text: "ni".into(), cursor: Some((2, 2)) }], 1.0);
        assert_eq!(s.ime_preedit, Some(("ni".to_owned(), Some((2, 2)))));
        s.begin_frame(&[], 1.0);
        assert!(s.ime_preedit.is_some(), "a composition outlives the frame");
        s.begin_frame(&[Event::Text("你".into())], 1.0);
        assert_eq!(s.ime_preedit, None, "a commit ends it");
        s.begin_frame(&[Event::ImePreedit { text: "a".into(), cursor: None }, Event::ImePreedit { text: String::new(), cursor: None }], 1.0);
        assert_eq!(s.ime_preedit, None, "an empty preedit clears it");
    }
}
