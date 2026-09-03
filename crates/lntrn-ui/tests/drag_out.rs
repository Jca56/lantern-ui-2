//! Dragging out of the window (U020): a press on a text selection waits,
//! a drag from it hands the text to the harness, the window system's end
//! of the drag puts the button up without a click, a plain click on the
//! selection collapses it, and a drag from outside it still selects.

use std::cell::RefCell;

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{DragPayload, Event, Key, Modifiers, Ui, WidgetId};

/// Select everything, and let the clock pass so the next press is no
/// double click. Returns a point inside the first word.
fn select_all(h: &mut Harness, id: WidgetId, f: &mut impl FnMut(&mut Ui)) -> Vec2 {
    h.click_on(id, &mut *f);
    h.key(Key::Home);
    h.key_with(Key::End, Modifiers::SHIFT);
    h.frame(&mut *f);
    h.advance(1.0);
    let rect = h.rect_of(id).unwrap();
    Vec2::new(rect.min.x + h.metrics().pad + 20.0, rect.center().y)
}

#[test]
fn a_selection_dragged_leaves_the_window() {
    let mut h = Harness::new(800.0, 600.0);
    let s = RefCell::new(String::from("hello world"));
    let mut f = |ui: &mut Ui| {
        ui.text_field("name", &mut s.borrow_mut());
    };
    let id = WidgetId::ROOT.with("name");
    let mid = select_all(&mut h, id, &mut f);
    // A press on the selection, then a move well past the threshold.
    h.move_to(mid);
    h.press();
    h.frame(f);
    assert!(h.state.take_drag_out().is_none(), "a press alone drags nothing");
    h.move_to(mid + Vec2::new(60.0, 0.0));
    h.frame(f);
    assert_eq!(h.state.take_drag_out(), Some(DragPayload::Text("hello world".into())), "the selection, whole");
    assert!(h.state.down && h.state.active == Some(id), "the widget still holds the press");
    // The harness started the drag; the compositor ends it.
    h.state.dragging_out = true;
    h.event(Event::DragEnded { dropped: true });
    h.frame(f);
    assert!(!h.state.down && h.state.active.is_none() && !h.state.dragging_out, "the button is up");
    assert_eq!(h.state.drag_ended, Some(true));
    assert_eq!(*s.borrow(), "hello world", "a drag copies");
    // Typing still replaces the selection: nothing collapsed it.
    h.type_text("z");
    h.frame(f);
    assert_eq!(*s.borrow(), "z");
}

#[test]
fn a_click_on_the_selection_collapses_it_on_release() {
    let mut h = Harness::new(800.0, 600.0);
    let s = RefCell::new(String::from("hello world"));
    let mut f = |ui: &mut Ui| {
        ui.text_field("name", &mut s.borrow_mut());
    };
    let id = WidgetId::ROOT.with("name");
    let mid = select_all(&mut h, id, &mut f);
    h.move_to(mid);
    h.press();
    h.frame(f);
    h.release();
    h.frame(f);
    h.type_text("x");
    h.frame(f);
    let got = s.borrow().clone();
    assert_eq!(got.len(), 12, "nothing was replaced: {got}");
    assert!(got.contains('x') && got != "xhello world" && got != "hello worldx", "the caret went where the click was: {got}");
}

#[test]
fn a_drag_from_outside_the_selection_still_selects() {
    let mut h = Harness::new(800.0, 600.0);
    let s = RefCell::new(String::from("hello world"));
    let f = |ui: &mut Ui| {
        ui.text_field("name", &mut s.borrow_mut());
    };
    let id = WidgetId::ROOT.with("name");
    h.click_on(id, f);
    let rect = h.rect_of(id).unwrap();
    let pad = h.metrics().pad;
    // From before the first glyph to past the last: everything.
    h.drag(Vec2::new(rect.min.x + pad + 1.0, rect.center().y), Vec2::new(rect.max.x - 1.0, rect.center().y), 4, f);
    assert!(h.state.take_drag_out().is_none(), "no selection under the press: no drag out");
    h.type_text("z");
    h.frame(f);
    assert_eq!(*s.borrow(), "z", "the drag selected it all");
}

#[test]
fn the_text_area_drags_its_selection_too() {
    let mut h = Harness::new(800.0, 600.0);
    let s = RefCell::new(String::from("one two three"));
    let f = |ui: &mut Ui| {
        ui.text_area("notes", &mut s.borrow_mut(), Some(200.0));
    };
    let id = WidgetId::ROOT.with("notes");
    h.click_on(id, f);
    h.key_with(Key::Home, Modifiers::CTRL);
    h.key_with(Key::End, Modifiers::CTRL | Modifiers::SHIFT);
    h.frame(f);
    let rect = h.rect_of(id).unwrap();
    let pad = h.metrics().pad;
    let style_h = h.metrics().widget_h;
    // On the first line, a little way in.
    let at = Vec2::new(rect.min.x + pad + style_h, rect.min.y + pad + style_h * 0.4);
    h.move_to(at);
    h.press();
    h.frame(f);
    h.move_to(at + Vec2::new(80.0, 0.0));
    h.frame(f);
    assert_eq!(h.state.take_drag_out(), Some(DragPayload::Text("one two three".into())));
}
