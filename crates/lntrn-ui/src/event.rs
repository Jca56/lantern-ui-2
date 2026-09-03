//! Window-system-independent input vocabulary: events, keys, modifiers,
//! buttons. `lntrn-app` translates winit into these; the shell, keymaps and
//! every host speak this and nothing else.

use std::path::PathBuf;

use lntrn_math::Vec2;

/// Keyboard modifier state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);
    pub const SHIFT: Modifiers = Modifiers(1);
    pub const CTRL: Modifiers = Modifiers(2);
    pub const ALT: Modifiers = Modifiers(4);
    pub const SUPER: Modifiers = Modifiers(8);

    pub const fn contains(self, o: Modifiers) -> bool {
        self.0 & o.0 == o.0
    }
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }
    pub const fn ctrl(self) -> bool {
        self.contains(Self::CTRL)
    }
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }
    pub const fn super_key(self) -> bool {
        self.contains(Self::SUPER)
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for Modifiers {
    type Output = Modifiers;
    fn bitor(self, o: Modifiers) -> Modifiers {
        Modifiers(self.0 | o.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelDelta {
    /// Notches; `y > 0` scrolls content up (wheel away from the user).
    Lines(Vec2),
    /// Physical pixels from a touchpad or high-resolution wheel.
    Pixels(Vec2),
}

impl WheelDelta {
    /// Approximate pixel distance, treating one line as `line_px`.
    pub fn to_pixels(self, line_px: f64) -> Vec2 {
        match self {
            WheelDelta::Lines(l) => l * line_px,
            WheelDelta::Pixels(p) => p,
        }
    }
}

/// Logical key identity, layout-aware (what the key *means*).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Space,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Shift,
    Control,
    Alt,
    Super,
    CapsLock,
    Unknown,
}

impl Key {
    /// How the key is written in a keymap editor: `S`, `F3`, `Left`, `Space`.
    pub fn label(self) -> String {
        match self {
            Key::Char(' ') | Key::Space => "Space".to_owned(),
            Key::Char(c) => c.to_uppercase().collect(),
            Key::Escape => "Esc".to_owned(),
            Key::Enter => "Enter".to_owned(),
            Key::Tab => "Tab".to_owned(),
            Key::Backspace => "Backspace".to_owned(),
            Key::Delete => "Delete".to_owned(),
            Key::Insert => "Insert".to_owned(),
            Key::ArrowLeft => "Left".to_owned(),
            Key::ArrowRight => "Right".to_owned(),
            Key::ArrowUp => "Up".to_owned(),
            Key::ArrowDown => "Down".to_owned(),
            Key::Home => "Home".to_owned(),
            Key::End => "End".to_owned(),
            Key::PageUp => "Page Up".to_owned(),
            Key::PageDown => "Page Down".to_owned(),
            Key::F(n) => format!("F{n}"),
            Key::Shift => "Shift".to_owned(),
            Key::Control => "Ctrl".to_owned(),
            Key::Alt => "Alt".to_owned(),
            Key::Super => "Super".to_owned(),
            Key::CapsLock => "Caps Lock".to_owned(),
            Key::Unknown => "?".to_owned(),
        }
    }

    /// A modifier on its own, not something to bind.
    pub fn is_modifier(self) -> bool {
        matches!(self, Key::Shift | Key::Control | Key::Alt | Key::Super | Key::CapsLock)
    }
}

impl Modifiers {
    /// `Ctrl+Shift+` style prefix (empty with no modifiers).
    pub fn label(self) -> String {
        let mut s = String::new();
        for (m, name) in [(Modifiers::CTRL, "Ctrl+"), (Modifiers::ALT, "Alt+"), (Modifiers::SHIFT, "Shift+"), (Modifiers::SUPER, "Super+")] {
            if self.contains(m) {
                s.push_str(name);
            }
        }
        s
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// New surface size in physical pixels.
    Resized { width: u32, height: u32 },
    ScaleFactor(f64),
    Focus(bool),
    CloseRequested,
    /// Pointer position in physical pixels.
    PointerMoved(Vec2),
    PointerLeft,
    Button { button: MouseButton, pressed: bool, pos: Vec2, mods: Modifiers },
    Wheel { delta: WheelDelta, pos: Vec2, mods: Modifiers },
    Key { key: Key, pressed: bool, repeat: bool, mods: Modifiers },
    /// Committed text from a key press (already dead-key/IME resolved).
    Text(String),
    Modifiers(Modifiers),
    /// An input method is composing: show `text` at the caret, with the
    /// composition caret at `cursor` (byte range into `text`) or hidden.
    /// Empty `text` ends the composition; the committed result arrives as
    /// [`Event::Text`].
    ImePreedit { text: String, cursor: Option<(usize, usize)> },
    /// A file from outside is being dragged over the window.
    FileHovered(PathBuf),
    /// The dragged file left without dropping.
    FileHoverLeft,
    /// A file was dropped on the window (one event per file).
    FileDropped(PathBuf),
}

impl Event {
    /// Does this event mean "something may look different now"?
    pub fn wants_redraw(&self) -> bool {
        matches!(self, Event::Resized { .. } | Event::ScaleFactor(_) | Event::Focus(_) | Event::ImePreedit { .. } | Event::FileHovered(_) | Event::FileHoverLeft | Event::FileDropped(_))
    }

    /// A key press that means "paste" (Ctrl+V, Shift+Insert): the harness
    /// pulls the system clipboard in before the rebuild that sees it.
    pub fn is_paste(&self) -> bool {
        match self {
            Event::Key { key: Key::Char(c), pressed: true, mods, .. } => c.eq_ignore_ascii_case(&'v') && *mods == Modifiers::CTRL,
            Event::Key { key: Key::Insert, pressed: true, mods, .. } => *mods == Modifiers::SHIFT,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers() {
        let m = Modifiers::CTRL | Modifiers::SHIFT;
        assert!(m.ctrl() && m.shift() && !m.alt() && !m.super_key());
        assert!(Modifiers::NONE.is_empty());
        assert!(m.contains(Modifiers::CTRL));
        assert!(!m.contains(Modifiers::ALT));
    }

    #[test]
    fn labels() {
        assert_eq!(Key::Char('s').label(), "S");
        assert_eq!(Key::F(3).label(), "F3");
        assert_eq!(Key::ArrowLeft.label(), "Left");
        assert_eq!((Modifiers::CTRL | Modifiers::SHIFT).label(), "Ctrl+Shift+");
        assert_eq!(Modifiers::NONE.label(), "");
        assert!(Key::Shift.is_modifier() && !Key::Char('a').is_modifier());
    }

    #[test]
    fn wheel_to_pixels() {
        assert_eq!(WheelDelta::Lines(Vec2::new(0.0, 2.0)).to_pixels(40.0), Vec2::new(0.0, 80.0));
        assert_eq!(WheelDelta::Pixels(Vec2::new(3.0, 4.0)).to_pixels(40.0), Vec2::new(3.0, 4.0));
        assert!(Event::Resized { width: 1, height: 1 }.wants_redraw());
        assert!(!Event::PointerLeft.wants_redraw());
        assert!(Event::FileDropped(PathBuf::from("/tmp/a.png")).wants_redraw());
    }

    #[test]
    fn paste_keys() {
        let key = |key, mods| Event::Key { key, pressed: true, repeat: false, mods };
        assert!(key(Key::Char('v'), Modifiers::CTRL).is_paste());
        assert!(key(Key::Char('V'), Modifiers::CTRL).is_paste(), "shifted letter still counts");
        assert!(key(Key::Insert, Modifiers::SHIFT).is_paste());
        assert!(!key(Key::Char('v'), Modifiers::NONE).is_paste());
        assert!(!key(Key::Char('c'), Modifiers::CTRL).is_paste());
        assert!(!Event::Key { key: Key::Char('v'), pressed: false, repeat: false, mods: Modifiers::CTRL }.is_paste(), "releases do not paste");
    }
}
