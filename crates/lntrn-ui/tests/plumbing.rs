//! The outside world, headless: the clipboard, input methods, and files
//! dropped on the window.

use std::cell::RefCell;
use std::path::PathBuf;

use lntrn_math::{Rect, Vec2};
use lntrn_ui::testing::Harness;
use lntrn_ui::{Action, AreaCx, AreaId, Axis, Event, Host, HostCx, Key, Modifiers, Shell, Ui, WidgetId};

#[test]
fn copy_marks_the_clipboard_and_paste_reads_it() {
    let mut h = Harness::new(800.0, 400.0);
    let text = RefCell::new(String::from("hello"));
    let f = |ui: &mut Ui| {
        ui.text_field("name", &mut text.borrow_mut());
    };
    h.click_on(WidgetId::ROOT.with("name"), f);
    h.key_with(Key::Char('a'), Modifiers::CTRL);
    h.key_with(Key::Char('c'), Modifiers::CTRL);
    h.settle(2, f);
    assert_eq!(h.state.clipboard, "hello");
    assert!(h.state.take_clipboard_dirty(), "the harness pushes it to the system");
    assert!(!h.state.take_clipboard_dirty(), "once");
    // The harness pulled "world" from the system before this paste.
    h.state.clipboard = "world".into();
    h.key_with(Key::Char('a'), Modifiers::CTRL);
    h.key_with(Key::Char('v'), Modifiers::CTRL);
    h.settle(2, f);
    assert_eq!(*text.borrow(), "world");
    assert!(!h.state.take_clipboard_dirty(), "pasting is not a copy");
    // A cut marks it too.
    h.key_with(Key::Char('a'), Modifiers::CTRL);
    h.key_with(Key::Char('x'), Modifiers::CTRL);
    h.settle(2, f);
    assert_eq!(*text.borrow(), "");
    assert_eq!(h.state.clipboard, "world");
    assert!(h.state.take_clipboard_dirty());
}

#[test]
fn a_composition_shows_at_the_caret_and_commits_as_text() {
    let mut h = Harness::new(800.0, 400.0);
    let text = RefCell::new(String::from("ab"));
    let f = |ui: &mut Ui| {
        ui.text_field("name", &mut text.borrow_mut());
    };
    h.click_on(WidgetId::ROOT.with("name"), f);
    h.key(Key::End);
    let plain = h.settle(2, f);
    let caret_before = h.state.ime_rect.expect("focused: the caret is reported for input methods");
    h.event(Event::ImePreedit { text: "ni".into(), cursor: Some((2, 2)) });
    let composing = h.settle(2, f);
    assert_eq!(*text.borrow(), "ab", "composing changes nothing yet");
    assert!(h.state.ime_preedit.is_some());
    assert!(composing.vertices > plain.vertices, "the composition is drawn (and underlined)");
    let caret = h.state.ime_rect.unwrap();
    assert!(caret.min.x > caret_before.min.x, "the caret sits after the composition: {caret:?} vs {caret_before:?}");
    h.event(Event::Text("你".into()));
    h.settle(2, f);
    assert_eq!(*text.borrow(), "ab你");
    assert_eq!(h.state.ime_preedit, None);
    // Focus elsewhere: no caret to report.
    h.move_to(Vec2::new(700.0, 350.0));
    h.press();
    h.frame(f);
    h.release();
    h.settle(2, f);
    assert_eq!(h.state.ime_rect, None);
}

#[test]
fn a_text_area_composes_too() {
    let mut h = Harness::new(800.0, 400.0);
    let text = RefCell::new(String::from("one\ntwo"));
    let f = |ui: &mut Ui| {
        ui.text_area("notes", &mut text.borrow_mut(), Some(200.0));
    };
    h.click_on(WidgetId::ROOT.with("notes"), f);
    h.key_with(Key::End, Modifiers::CTRL);
    let plain = h.settle(2, f);
    h.event(Event::ImePreedit { text: "san".into(), cursor: None });
    let composing = h.settle(2, f);
    assert_eq!(*text.borrow(), "one\ntwo");
    assert!(composing.vertices > plain.vertices);
    assert!(h.state.ime_rect.is_some());
    h.event(Event::Text("三".into()));
    h.settle(2, f);
    assert_eq!(*text.borrow(), "one\ntwo三");
}

/// Two editors: one with a text field and a drop zone under it, one plain.
#[derive(Default)]
struct Plumb {
    text: String,
    zone: Vec<PathBuf>,
    zone_hover: bool,
    drops: Vec<(Vec<PathBuf>, Option<AreaId>, Option<u8>)>,
}

impl Host for Plumb {
    type Editor = u8;
    type AreaState = ();
    fn editors(&self) -> &[u8] {
        &[0, 1]
    }
    fn editor_label(&self, e: u8) -> &str {
        if e == 0 { "Field" } else { "Plain" }
    }
    fn title(&self) -> String {
        "Plumb".into()
    }
    fn draw_body(&mut self, editor: u8, ui: &mut Ui, _: &mut AreaCx<()>) -> bool {
        if editor == 0 {
            ui.text_field("field", &mut self.text);
            let rest = Rect::new(ui.cursor(), ui.clip().max);
            let zone = ui.drop_zone(rest);
            self.zone_hover = zone.hovering;
            if !zone.files.is_empty() {
                self.zone = zone.files;
            }
        } else {
            ui.label("plain");
        }
        false
    }
    fn run(&mut self, _: &Action, _: &mut HostCx) {}
    fn dropped(&mut self, paths: &[PathBuf], area: Option<AreaId>, editor: Option<u8>, _: &mut HostCx) {
        self.drops.push((paths.to_vec(), area, editor));
    }
}

#[test]
fn dropped_files_go_to_a_zone_under_the_pointer_or_else_to_the_host() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Plumb> = Shell::new(0);
    let right = shell.screen.split(0, Axis::Horizontal, 0.5, 1).unwrap();
    let mut host = Plumb::default();
    h.shell_frame(&mut shell, &mut host);
    let p = PathBuf::from("/tmp/picture.png");

    // Over the right area, where there is no zone: the host gets it, with the area.
    let right_body = shell.screen.layout_of(right).unwrap().body.center();
    h.move_to(right_body);
    h.event(Event::FileHovered(p.clone()));
    h.shell_frame(&mut shell, &mut host);
    assert!(!host.zone_hover, "the zone is in the other area");
    h.event(Event::FileDropped(p.clone()));
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.drops, vec![(vec![p.clone()], Some(right), Some(1))]);
    assert!(host.zone.is_empty());

    // Over the zone: it takes the drop and the host hook stays quiet.
    let left_body = shell.screen.layout_of(0).unwrap().body;
    h.move_to(Vec2::new(left_body.center().x, left_body.max.y - 20.0));
    h.event(Event::FileHovered(p.clone()));
    h.shell_frame(&mut shell, &mut host);
    assert!(host.zone_hover, "lit while a file hovers");
    h.event(Event::FileDropped(p.clone()));
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.zone, vec![p]);
    assert!(!host.zone_hover, "the drop ends the hover");
    assert_eq!(host.drops.len(), 1, "nothing new for the host");
}

#[test]
fn the_shell_reports_the_caret_for_input_methods() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Plumb> = Shell::new(0);
    let mut host = Plumb::default();
    let out = h.shell_frame(&mut shell, &mut host);
    assert_eq!(out.ime, None, "nothing focused");
    let field = h.rect_of(WidgetId::ROOT.with_u64(0).with("body").with("field")).expect("the field");
    h.move_to(field.center());
    h.press();
    h.shell_frame(&mut shell, &mut host);
    h.release();
    let out = h.shell_settle(&mut shell, &mut host, 3);
    let caret = out.ime.expect("focused: the caret is reported");
    assert!(field.contains(caret.center()), "inside the field: {caret:?} in {field:?}");
    h.key(Key::Escape);
    h.move_to(Vec2::new(900.0, 650.0));
    h.press();
    h.shell_frame(&mut shell, &mut host);
    h.release();
    let out = h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(out.ime, None, "a press elsewhere takes focus away");
}
