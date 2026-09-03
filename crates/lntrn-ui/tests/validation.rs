//! Validated text fields (U023): a failing check reddens the field and
//! keeps Enter from committing; a passing one commits as usual.

use std::cell::RefCell;

use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, TextResponse, Ui, WidgetId};

fn port(s: &str) -> Option<String> {
    match s.parse::<u32>() {
        Ok(1..=65535) => None,
        _ => Some("a port".to_owned()),
    }
}

#[test]
fn a_failing_field_blocks_enter() {
    let mut h = Harness::new(800.0, 600.0);
    let s = RefCell::new(String::new());
    let last = RefCell::new(TextResponse::default());
    let f = |ui: &mut Ui| {
        *last.borrow_mut() = ui.text_field_validated("port", &mut s.borrow_mut(), &port);
    };
    h.click_on(WidgetId::ROOT.with("port"), f);
    assert!(last.borrow().invalid, "empty fails the check");
    h.type_text("abc");
    h.key(Key::Enter);
    h.frame(f);
    let r = *last.borrow();
    assert!(r.invalid && !r.committed && r.focused, "Enter on a failing field commits nothing: {r:?}");
    assert_eq!(*s.borrow(), "abc", "the text stays for fixing");
    // Fixed: Enter commits.
    h.key(Key::End);
    for _ in 0..3 {
        h.key(Key::Backspace);
    }
    h.type_text("80");
    h.frame(f);
    assert!(!last.borrow().invalid);
    h.key(Key::Enter);
    h.frame(f);
    let r = *last.borrow();
    assert!(r.committed && !r.invalid, "{r:?}");
    assert_eq!(*s.borrow(), "80");
}
