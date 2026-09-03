//! Cross-frame UI state: pointer and keyboard input gathered for one rebuild,
//! which widget is hot/active/focused, open popups, and (in
//! [`crate::memory`]) per-widget memory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use lntrn_image::Image;
use lntrn_math::{Rect, Vec2};

use crate::event::{Event, Key, Modifiers, MouseButton};
use crate::id::WidgetId;
pub use crate::memory::{AnimMem, History, ScrollMem, Snapshot, TableMem, TextEdit};
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

/// What a drag out of the window carries (see [`UiState::start_drag_out`]).
/// The harness offers it to whatever window the pointer lets go over:
/// text as text, files as a file list, a picture as PNG and as a file.
#[derive(Clone, Debug, PartialEq)]
pub enum DragPayload {
    Text(String),
    Files(Vec<PathBuf>),
    /// `name` names the file other apps see (`.png` is added if missing).
    Image { image: Image, name: String },
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
    /// Set by a host or widget that wants the *system* clipboard's text
    /// in [`Self::clipboard`] on the next rebuild (a paste that no key
    /// started). The harness reads it in and rebuilds once more.
    pub clipboard_wanted: bool,
    /// A picture on the clipboard, beside the text: put there by
    /// [`Self::set_clipboard_image`] (the harness pushes it out as PNG),
    /// or brought in by the harness on the rebuild after
    /// [`Self::clipboard_image_wanted`] was set. Take it when you use it.
    pub clipboard_image: Option<Image>,
    clipboard_image_dirty: bool,
    /// Like [`Self::clipboard_wanted`], for a picture.
    pub clipboard_image_wanted: bool,
    /// A file from outside is being dragged over the window: drop zones
    /// light up.
    pub hovering_files: bool,
    /// Files dropped on the window this frame. A widget under the pointer
    /// may take them ([`crate::Ui::drop_zone`]); whatever is left goes to
    /// [`crate::Host::dropped`].
    pub dropped_files: Vec<PathBuf>,
    /// A drag out of the window asked for this frame; the harness takes
    /// it ([`Self::take_drag_out`]) and starts it.
    drag_out: Option<DragPayload>,
    /// A drag out of the window is under way: the window system has the
    /// pointer until it ends ([`Event::DragEnded`]).
    pub dragging_out: bool,
    /// The drag out ended this frame: `Some(true)` dropped somewhere,
    /// `Some(false)` let go nowhere. A widget moving what it dragged
    /// (rather than copying) finishes the move on `Some(true)`.
    pub drag_ended: Option<bool>,
    /// An input method's composition in progress: the text and the
    /// composition caret (byte range into it). Text widgets show it inline
    /// at their caret; [`Event::Text`] commits it.
    pub ime_preedit: Option<(String, Option<(usize, usize)>)>,
    /// Where the focused text widget's caret is, so the input method can
    /// place its candidate window. Set each frame by the widget.
    pub ime_rect: Option<Rect>,
    /// The `reduce_motion` preference: [`crate::Ui::animate`] snaps, toasts
    /// vanish instead of fading, busy bars hold still.
    pub reduce_motion: bool,
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
            clipboard_wanted: false,
            clipboard_image: None,
            clipboard_image_dirty: false,
            clipboard_image_wanted: false,
            hovering_files: false,
            dropped_files: Vec::new(),
            drag_out: None,
            dragging_out: false,
            drag_ended: None,
            ime_preedit: None,
            ime_rect: None,
            reduce_motion: false,
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

    /// Put a picture on the clipboard. The harness pushes it out as PNG
    /// after the frame.
    pub fn set_clipboard_image(&mut self, image: Image) {
        self.clipboard_image = Some(image);
        self.clipboard_image_dirty = true;
    }

    /// Whether a picture was put on the clipboard since the last call.
    pub fn take_clipboard_image_dirty(&mut self) -> bool {
        std::mem::take(&mut self.clipboard_image_dirty)
    }

    /// Take this frame's dropped files, leaving none for anyone after.
    pub fn take_dropped_files(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.dropped_files)
    }

    /// Start dragging `payload` out of the window: the pointer is down and
    /// moving (see [`crate::Ui::drag_out_starts`]), and the harness hands
    /// it to the window system after this frame. From then on the drag is
    /// the window system's until [`Event::DragEnded`] arrives.
    pub fn start_drag_out(&mut self, payload: DragPayload) {
        self.drag_out = Some(payload);
    }

    /// The drag out asked for this frame, for the harness to start.
    pub fn take_drag_out(&mut self) -> Option<DragPayload> {
        self.drag_out.take()
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
        self.drag_ended = None;
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
                    // A release that reached us means no drag out took the
                    // pointer after all.
                    self.dragging_out = false;
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
                Event::DragEnded { dropped } => {
                    // The button is up, but nothing was clicked.
                    self.down = false;
                    self.active = None;
                    self.dragging_out = false;
                    self.drag_ended = Some(*dropped);
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
