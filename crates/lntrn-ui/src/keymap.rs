//! Keymaps are data: a trigger, an action id, argument overrides. Grouped
//! into named maps; resolution walks the maps for the current context chain
//! and fires the first item whose action `poll`s true. The host owns its
//! [`KeyConfig`] and resolves the keys the shell hands it in [`crate::Host::key`].

use lntrn_props::Value;

use crate::event::{Event, Key, Modifiers, MouseButton};
use crate::host::Action;

#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub key: Option<Key>,
    pub button: Option<MouseButton>,
    pub mods: Modifiers,
    /// Fire on press (else on release).
    pub press: bool,
    /// Only on a double click (buttons).
    pub double: bool,
}

impl Trigger {
    pub const fn key(key: Key, mods: Modifiers) -> Self {
        Self { key: Some(key), button: None, mods, press: true, double: false }
    }

    pub const fn button(button: MouseButton, mods: Modifiers) -> Self {
        Self { key: None, button: Some(button), mods, press: true, double: false }
    }

    /// `Ctrl+S`, `Right Click`, `Double Left Click`, `F3 (release)`.
    pub fn label(&self) -> String {
        let mut s = self.mods.label();
        match (self.key, self.button) {
            (Some(k), _) => s.push_str(&k.label()),
            (None, Some(b)) => {
                if self.double {
                    s.push_str("Double ");
                }
                s.push_str(match b {
                    MouseButton::Left => "Left Click",
                    MouseButton::Right => "Right Click",
                    MouseButton::Middle => "Middle Click",
                    MouseButton::Back => "Back Button",
                    MouseButton::Forward => "Forward Button",
                    MouseButton::Other(_) => "Mouse Button",
                });
            }
            (None, None) => s.push_str("(unbound)"),
        }
        if !self.press {
            s.push_str(" (release)");
        }
        s
    }

    pub fn matches(&self, ev: &Event) -> bool {
        match ev {
            Event::Key { key, pressed, mods, .. } => {
                self.key.is_some_and(|k| key_eq(k, *key)) && *pressed == self.press && *mods == self.mods
            }
            Event::Button { button, pressed, mods, .. } => {
                self.button == Some(*button) && *pressed == self.press && *mods == self.mods
            }
            _ => false,
        }
    }
}

/// Letters match case-insensitively so Shift+A binds without knowing
/// whether the platform reports `a` or `A`.
fn key_eq(a: Key, b: Key) -> bool {
    match (a, b) {
        (Key::Char(x), Key::Char(y)) => x.eq_ignore_ascii_case(&y),
        _ => a == b,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyItem {
    pub trigger: Trigger,
    pub op: String,
    pub overrides: Vec<(String, Value)>,
}

impl KeyItem {
    pub fn new(trigger: Trigger, op: &str) -> Self {
        Self { trigger, op: op.to_owned(), overrides: Vec::new() }
    }

    pub fn with(mut self, field: &str, v: Value) -> Self {
        self.overrides.push((field.to_owned(), v));
        self
    }

    /// `(name, value)` pairs, borrowed.
    pub fn overrides(&self) -> Vec<(&str, Value)> {
        self.overrides.iter().map(|(n, v)| (n.as_str(), v.clone())).collect()
    }

    /// The action this binding fires.
    pub fn action(&self) -> Action {
        Action { id: self.op.clone(), args: self.overrides.clone() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyMap {
    pub name: String,
    pub items: Vec<KeyItem>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyConfig {
    pub maps: Vec<KeyMap>,
}

/// The conventional name of the map that applies everywhere; a host passes
/// it last in its context chain.
pub const CTX_WINDOW: &str = "window";

impl KeyConfig {
    pub fn map(&self, name: &str) -> Option<&KeyMap> {
        self.maps.iter().find(|m| m.name == name)
    }

    pub fn map_mut(&mut self, name: &str) -> &mut KeyMap {
        if let Some(i) = self.maps.iter().position(|m| m.name == name) {
            &mut self.maps[i]
        } else {
            self.maps.push(KeyMap { name: name.to_owned(), items: Vec::new() });
            self.maps.last_mut().expect("just pushed")
        }
    }

    pub fn bind(&mut self, map: &str, item: KeyItem) {
        self.map_mut(map).items.push(item);
    }

    /// First item in `contexts` order whose trigger matches `ev` and whose
    /// action `poll`s true.
    pub fn resolve<'a>(&'a self, contexts: &[&str], ev: &Event, mut poll: impl FnMut(&str) -> bool) -> Option<&'a KeyItem> {
        for name in contexts {
            let Some(map) = self.map(name) else {
                continue;
            };
            for item in &map.items {
                if item.trigger.matches(ev) && poll(&item.op) {
                    return Some(item);
                }
            }
        }
        None
    }

    /// The label of the first binding that fires `action` (`Ctrl+O`), for
    /// showing beside a menu row. A binding matches when its action id is
    /// the same and every override it sets is set the same on `action`.
    pub fn hint_for(&self, action: &Action) -> Option<String> {
        self.maps
            .iter()
            .flat_map(|m| m.items.iter())
            .find(|item| item.op == action.id && item.overrides.iter().all(|(k, v)| action.arg(k) == Some(v)))
            .map(|item| item.trigger.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lntrn_math::Vec2;

    fn key(k: Key, mods: Modifiers) -> Event {
        Event::Key { key: k, pressed: true, repeat: false, mods }
    }

    fn sample() -> KeyConfig {
        use Key::*;
        let ctrl = Modifiers::CTRL;
        let shift = Modifiers::SHIFT;
        let none = Modifiers::NONE;
        let mut k = KeyConfig::default();
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('z'), ctrl), "ed.undo"));
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('z'), ctrl | shift), "ed.redo"));
        k.bind("object", KeyItem::new(Trigger::key(Char('a'), shift), "wm.call_menu").with("menu", Value::Str("add".into())));
        k.bind("object", KeyItem::new(Trigger::key(Tab, none), "mode_set").with("mode", Value::Enum(1)));
        k.bind("mesh", KeyItem::new(Trigger::key(Tab, none), "mode_set").with("mode", Value::Enum(0)));
        k
    }

    #[test]
    fn resolution_order_and_poll() {
        let k = sample();
        let undo = k.resolve(&["mesh", CTX_WINDOW], &key(Key::Char('z'), Modifiers::CTRL), |_| true).unwrap();
        assert_eq!(undo.op, "ed.undo");
        // Uppercase from a shifted key still matches.
        let add = k.resolve(&["object", CTX_WINDOW], &key(Key::Char('A'), Modifiers::SHIFT), |_| true).unwrap();
        assert_eq!(add.op, "wm.call_menu");
        assert_eq!(add.overrides()[0].0, "menu");
        // Tab means different things per context; the first context wins.
        let tab = key(Key::Tab, Modifiers::NONE);
        assert_eq!(k.resolve(&["mesh", "object"], &tab, |_| true).unwrap().overrides[0].1, Value::Enum(0));
        assert_eq!(k.resolve(&["object", "mesh"], &tab, |_| true).unwrap().overrides[0].1, Value::Enum(1));
        // Poll rejects: falls through to nothing.
        assert!(k.resolve(&["object"], &tab, |_| false).is_none());
        // Modifiers must match exactly; releases do not fire.
        assert!(k.resolve(&[CTX_WINDOW], &key(Key::Char('z'), Modifiers::NONE), |_| true).is_none());
        let release = Event::Key { key: Key::Char('z'), pressed: false, repeat: false, mods: Modifiers::CTRL };
        assert!(k.resolve(&[CTX_WINDOW], &release, |_| true).is_none());
        let click = Event::Button { button: MouseButton::Left, pressed: true, pos: Vec2::ZERO, mods: Modifiers::NONE };
        assert!(Trigger::button(MouseButton::Left, Modifiers::NONE).matches(&click));
        assert_eq!(Trigger::key(Key::Char('s'), Modifiers::CTRL | Modifiers::SHIFT).label(), "Ctrl+Shift+S");
        assert_eq!(Trigger::button(MouseButton::Right, Modifiers::NONE).label(), "Right Click");
    }

    #[test]
    fn hints_for_menu_rows() {
        let k = sample();
        assert_eq!(k.hint_for(&Action::new("ed.undo")).as_deref(), Some("Ctrl+Z"));
        assert_eq!(k.hint_for(&Action::new("ed.redo")).as_deref(), Some("Ctrl+Shift+Z"));
        assert_eq!(k.hint_for(&Action::new("nothing")), None);
        // Overrides must agree: the add menu's binding is not a hint for another menu.
        let add = Action::new("wm.call_menu").with("menu", Value::Str("add".into()));
        assert_eq!(k.hint_for(&add).as_deref(), Some("Shift+A"));
        let file = Action::new("wm.call_menu").with("menu", Value::Str("file".into()));
        assert_eq!(k.hint_for(&file), None);
        // The first map with a match wins (object's Tab before mesh's).
        assert_eq!(k.hint_for(&Action::new("mode_set").with("mode", Value::Enum(0))).as_deref(), Some("Tab"));
    }
}
