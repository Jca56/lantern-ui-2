//! The keymap editor: rebinding by pressing a key, editing ids, add, remove.

use std::cell::RefCell;

use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, KeyConfig, KeyItem, Modifiers, Trigger, Ui, WidgetId};

#[test]
fn rebind_edit_add_remove() {
    let mut h = Harness::new(1000.0, 700.0);
    let keys = RefCell::new(KeyConfig::default());
    keys.borrow_mut().bind("window", KeyItem::new(Trigger::key(Key::Char('s'), Modifiers::CTRL), "file.save"));
    keys.borrow_mut().bind("window", KeyItem::new(Trigger::key(Key::F(3), Modifiers::NONE), "palette"));
    let changed = RefCell::new(0);
    let f = |ui: &mut Ui| {
        if ui.keymap_editor("keys", &mut keys.borrow_mut()) {
            *changed.borrow_mut() += 1;
        }
    };
    // Ids: keys / map 0 / "window" (collapsing) / item i / trigger.
    let item = |i: usize| WidgetId::ROOT.with("keys").with_index(0).with("window").with_index(i);
    h.click_on(item(0).with("trigger"), f);
    h.key_with(Key::Char('k'), Modifiers::ALT);
    h.settle(3, f);
    assert_eq!(keys.borrow().maps[0].items[0].trigger.label(), "Alt+K");
    assert_eq!(*changed.borrow(), 1);
    // Escape keeps the old binding.
    h.advance(1.0);
    h.click_on(item(1).with("trigger"), f);
    h.key(Key::Escape);
    h.settle(3, f);
    assert_eq!(keys.borrow().maps[0].items[1].trigger.label(), "F3");
    // Modifier presses alone are ignored while listening.
    h.advance(1.0);
    h.click_on(item(1).with("trigger"), f);
    h.key(Key::Shift);
    h.settle(2, f);
    assert_eq!(keys.borrow().maps[0].items[1].trigger.label(), "F3", "still listening");
    h.key_with(Key::Char('p'), Modifiers::CTRL | Modifiers::SHIFT);
    h.settle(3, f);
    assert_eq!(keys.borrow().maps[0].items[1].trigger.label(), "Ctrl+Shift+P");
    // The action id is a text field.
    h.advance(1.0);
    h.click_on(item(1).with("action"), f);
    h.key(Key::End);
    h.type_text(".open");
    h.frame(f);
    assert_eq!(keys.borrow().maps[0].items[1].op, "palette.open");
    // Add, then remove the new one.
    h.advance(1.0);
    h.click_on(WidgetId::ROOT.with("keys").with_index(0).with("window").with("+ Add binding"), f);
    assert_eq!(keys.borrow().maps[0].items.len(), 3);
    h.advance(1.0);
    h.click_on(item(2).with("−"), f);
    assert_eq!(keys.borrow().maps[0].items.len(), 2);
}
