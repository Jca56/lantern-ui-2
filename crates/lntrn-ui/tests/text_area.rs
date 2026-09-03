//! The multi-line text area, headless.

use std::cell::RefCell;

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, Modifiers, Ui, WidgetId};

fn area(h: &mut Harness, text: &RefCell<String>, f: impl Fn(&mut Ui, &mut String)) -> WidgetId {
    let id = WidgetId::ROOT.with("notes");
    h.frame(|ui| f(ui, &mut text.borrow_mut()));
    id
}

#[test]
fn typing_newlines_and_vertical_motion() {
    let mut h = Harness::new(900.0, 700.0);
    let text = RefCell::new(String::new());
    let draw = |ui: &mut Ui, s: &mut String| {
        ui.text_area("notes", s, Some(400.0));
    };
    let f = |ui: &mut Ui| draw(ui, &mut text.borrow_mut());
    let id = area(&mut h, &text, draw);
    h.click_on(id, f);
    h.type_text("hello");
    h.key(Key::Enter);
    h.type_text("big world");
    h.frame(f);
    assert_eq!(*text.borrow(), "hello\nbig world");
    // Up keeps the column: from the end of "big world" (col 9) to "hello" (clamped to 5).
    h.key(Key::ArrowUp);
    h.frame(f);
    assert_eq!(h.state.text_edit(id).cursor, 5, "end of the first line");
    h.key(Key::ArrowDown);
    h.frame(f);
    assert_eq!(h.state.text_edit(id).cursor, 15, "back to the remembered column, the end");
    // Home/End work on the row; Ctrl+Home is the top.
    h.key(Key::Home);
    h.frame(f);
    assert_eq!(h.state.text_edit(id).cursor, 6);
    h.key(Key::End);
    h.frame(f);
    assert_eq!(h.state.text_edit(id).cursor, 15);
    h.key_with(Key::Home, Modifiers::CTRL);
    h.frame(f);
    assert_eq!(h.state.text_edit(id).cursor, 0);
    // Shift+Down selects across the newline; copy takes both lines.
    h.key_with(Key::ArrowDown, Modifiers::SHIFT);
    h.key_with(Key::ArrowRight, Modifiers::SHIFT);
    h.frame(f);
    let (s0, s1) = h.state.text_edit(id).selection();
    assert_eq!((s0, s1), (0, 7), "column 0 of line 2, plus one: {:?}", &text.borrow()[s0..s1]);
    h.key_with(Key::Char('c'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(h.state.clipboard, "hello\nb");
    // Ctrl+Enter commits; Enter alone never does.
    let committed = RefCell::new(false);
    let g = |ui: &mut Ui| {
        if ui.text_area("notes", &mut text.borrow_mut(), Some(400.0)).committed {
            *committed.borrow_mut() = true;
        }
    };
    h.key(Key::Enter);
    h.frame(g);
    assert!(!*committed.borrow());
    h.key_with(Key::Enter, Modifiers::CTRL);
    h.frame(g);
    assert!(*committed.borrow());
}

#[test]
fn clicks_place_the_caret_and_double_click_picks_a_word() {
    let mut h = Harness::new(900.0, 700.0);
    let text = RefCell::new(String::from("alpha beta\ngamma delta"));
    let draw = |ui: &mut Ui, s: &mut String| {
        ui.text_area("notes", s, Some(400.0));
    };
    let f = |ui: &mut Ui| draw(ui, &mut text.borrow_mut());
    let id = area(&mut h, &text, draw);
    let r = h.rect_of(id).unwrap();
    let lh = h.theme.metrics(1.0).text_size as f64 * 1.2;
    // Second row, near its start.
    h.click_at(Vec2::new(r.min.x + 12.0, r.min.y + 10.0 + lh * 1.5), f);
    let c = h.state.text_edit(id).cursor;
    assert!((11..=12).contains(&c), "start of gamma: {c}");
    h.advance(1.0);
    // Double-click on "delta" selects it.
    let style = lntrn_text::TextStyle::new(h.theme.metrics(1.0).text_size);
    let x_delta = h.text.measure("gamma de", &style) as f64;
    let p = Vec2::new(r.min.x + 10.0 + x_delta, r.min.y + 10.0 + lh * 1.5);
    h.click_at(p, f);
    h.click_at(p, f);
    let (s0, s1) = h.state.text_edit(id).selection();
    assert_eq!(&text.borrow()[s0..s1], "delta");
    // Typing replaces the selection.
    h.type_text("D");
    h.frame(f);
    assert_eq!(*text.borrow(), "alpha beta\ngamma D");
}

#[test]
fn wraps_and_scrolls_to_the_caret() {
    let mut h = Harness::new(400.0, 300.0);
    let text = RefCell::new((0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"));
    let draw = |ui: &mut Ui, s: &mut String| {
        ui.text_area("notes", s, Some(200.0));
    };
    let f = |ui: &mut Ui| draw(ui, &mut text.borrow_mut());
    let id = area(&mut h, &text, draw);
    h.click_on(id, f);
    h.key_with(Key::End, Modifiers::CTRL);
    h.frame(f);
    assert!(h.state.scroll(id).offset.y > 0.0, "scrolled down to the caret");
    assert!(h.rect_of(id.with("thumb")).is_some(), "a scrollbar thumb exists");
    h.key_with(Key::Home, Modifiers::CTRL);
    h.frame(f);
    assert_eq!(h.state.scroll(id).offset.y, 0.0);
    // A long word wraps: the text is still one string, rows just multiply.
    *text.borrow_mut() = "word ".repeat(60);
    h.frame(f);
    assert!(h.state.scroll(id).content.y > 200.0, "wrapped content is taller than the box");
}

#[test]
fn undo_and_redo_step_through_edits() {
    let mut h = Harness::new(900.0, 700.0);
    let text = RefCell::new(String::new());
    let draw = |ui: &mut Ui, s: &mut String| {
        ui.text_area("notes", s, Some(400.0));
    };
    let f = |ui: &mut Ui| draw(ui, &mut text.borrow_mut());
    let id = area(&mut h, &text, draw);
    h.click_on(id, f);
    h.type_text("one");
    h.frame(f);
    h.advance(2.0);
    h.type_text(" two");
    h.frame(f);
    h.advance(2.0);
    h.key(Key::Backspace);
    h.frame(f);
    assert_eq!(*text.borrow(), "one tw");
    h.key_with(Key::Char('z'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(*text.borrow(), "one two", "undo the delete");
    h.key_with(Key::Char('z'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(*text.borrow(), "one", "undo the second burst of typing");
    h.key_with(Key::Char('z'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(*text.borrow(), "", "and the first");
    h.key_with(Key::Char('y'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(*text.borrow(), "one", "redo");
    h.key_with(Key::Char('z'), Modifiers::CTRL | Modifiers::SHIFT);
    h.frame(f);
    assert_eq!(*text.borrow(), "one two", "Ctrl+Shift+Z is redo too");
    h.advance(2.0);
    h.type_text("!");
    h.frame(f);
    h.key_with(Key::Char('y'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(*text.borrow(), "one two!", "a new edit drops the redo branch");
}
