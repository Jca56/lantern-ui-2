//! Widgets driven by the headless harness: clicks, drags, keys, and a
//! whole shell with a tiny host.

use std::cell::RefCell;

use lntrn_math::Vec2;
use lntrn_props::Value;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Action, AreaCx, Host, HostCx, Key, KeyPress, Menu, MenuItem, Modifiers, Shell, Ui, WidgetId, actions};

#[test]
fn button_click_and_toggle() {
    let mut h = Harness::new(800.0, 600.0);
    let mut clicks = 0;
    let mut on = false;
    let mut ui_fn = |ui: &mut Ui| {
        if ui.button("Click me").clicked {
            clicks += 1;
        }
        ui.toggle("Snap", &mut on);
    };
    h.click_on(WidgetId::ROOT.with("Click me"), &mut ui_fn);
    h.click_on(WidgetId::ROOT.with("Snap"), &mut ui_fn);
    // A press that starts off the button and releases on it is not a click.
    h.move_to(Vec2::new(700.0, 500.0));
    h.press();
    h.frame(&mut ui_fn);
    let r = h.rect_of_label("Click me").unwrap();
    h.move_to(r.center());
    h.release();
    h.settle(2, &mut ui_fn);
    assert_eq!(clicks, 1);
    assert!(on);
}

#[test]
fn slider_drags_and_types() {
    let mut h = Harness::new(800.0, 600.0);
    let v = RefCell::new(0.0);
    let mut f = |ui: &mut Ui| {
        ui.slider("Opacity", &mut v.borrow_mut(), 0.0, 1.0, 0.0);
    };
    h.frame(f);
    let r = h.rect_of_label("Opacity").unwrap();
    h.drag(Vec2::new(r.min.x + 1.0, r.center().y), Vec2::new(r.min.x + r.width() * 0.5, r.center().y), 4, &mut f);
    assert!((*v.borrow() - 0.5).abs() < 0.02, "dragged to the middle: {}", v.borrow());
    // Double-click opens a number editor; typing a value and Enter commits.
    let c = Vec2::new(r.min.x + r.width() * 0.5, r.center().y);
    h.click_at(c, &mut f);
    h.click_at(c, &mut f);
    h.set_mods(Modifiers::CTRL);
    h.key(Key::Char('a'));
    h.set_mods(Modifiers::NONE);
    h.type_text("0.25");
    h.key(Key::Enter);
    h.settle(3, &mut f);
    assert!((*v.borrow() - 0.25).abs() < 1e-9, "typed 0.25: {}", v.borrow());
}

#[test]
fn text_field_edits() {
    let mut h = Harness::new(800.0, 600.0);
    let s = RefCell::new(String::from("ab"));
    let mut f = |ui: &mut Ui| {
        ui.text_field("name", &mut s.borrow_mut());
    };
    h.click_on(WidgetId::ROOT.with("name"), &mut f);
    h.key(Key::End);
    h.type_text("cd");
    h.frame(f);
    assert_eq!(*s.borrow(), "abcd");
    h.key(Key::Backspace);
    h.key(Key::Home);
    h.type_text("x");
    h.frame(f);
    assert_eq!(*s.borrow(), "xabc");
    // Shift+End selects to the end; typing replaces the selection.
    h.key_with(Key::Home, Modifiers::NONE);
    h.key_with(Key::End, Modifiers::SHIFT);
    h.type_text("z");
    h.frame(f);
    assert_eq!(*s.borrow(), "z");
}

#[test]
fn dropdown_opens_and_picks() {
    let mut h = Harness::new(800.0, 600.0);
    let mut choice = 0;
    let options = ["Solid", "Wire", "Material"];
    let mut f = |ui: &mut Ui| {
        ui.dropdown("shading", &mut choice, &options);
    };
    let id = WidgetId::ROOT.with("shading");
    h.click_on(id, &mut f);
    let item = h.rect_of(id.with("item").with_index(2)).expect("the list is open");
    h.click_at(item.center(), &mut f);
    assert_eq!(choice, 2);
    assert!(h.rect_of(id.with("item").with_index(0)).is_none(), "the list closed");
}

#[test]
fn scroll_area_scrolls_with_the_wheel() {
    let mut h = Harness::new(800.0, 300.0);
    let mut picked = None;
    let mut f = |ui: &mut Ui| {
        ui.scroll_area("list", None, |ui| {
            for i in 0..40 {
                ui.push_index(i);
                if ui.selectable(&format!("Row {i}"), false).clicked {
                    picked = Some(i);
                }
                ui.pop_id();
            }
        });
    };
    h.frame(&mut f);
    let row0 = h.rect_of(WidgetId::ROOT.with("list").with_index(0).with("Row 0")).unwrap();
    h.move_to(Vec2::new(200.0, 150.0));
    h.wheel(-5.0);
    h.frame(&mut f);
    let row0_after = h.rect_of(WidgetId::ROOT.with("list").with_index(0).with("Row 0")).unwrap();
    assert!(row0_after.min.y < row0.min.y, "content moved up");
    // 5 notches of 45px scrolled 225px: rows 5..=9 are in the 280px viewport.
    let row7 = h.rect_of(WidgetId::ROOT.with("list").with_index(7).with("Row 7")).unwrap();
    h.click_at(row7.center(), &mut f);
    assert_eq!(picked, Some(7));
}

#[test]
fn tabs_switch() {
    let mut h = Harness::new(800.0, 600.0);
    let mut tab = 0;
    let mut f = |ui: &mut Ui| {
        ui.tabs(&mut tab, &["A", "B", "C"]);
    };
    h.click_on(WidgetId::ROOT.with("C").with_index(2), &mut f);
    assert_eq!(tab, 2);
}

// ---- a whole shell -----------------------------------------------------------

struct Tiny {
    ran: Vec<String>,
    bodies: usize,
}

impl Host for Tiny {
    type Editor = u8;
    type AreaState = ();
    fn editors(&self) -> &[u8] {
        &[0, 1]
    }
    fn editor_label(&self, e: u8) -> &str {
        if e == 0 { "Main" } else { "Other" }
    }
    fn title(&self) -> String {
        "Tiny".into()
    }
    fn title_menus(&self) -> &[(&str, &str)] {
        &[("File", "file")]
    }
    fn menu(&self, name: &str) -> Option<Menu> {
        (name == "file").then(|| Menu::new("File", vec![MenuItem::new("Do Thing", Action::new("tiny.thing")), MenuItem::new("Quit", Action::new(actions::QUIT))]))
    }
    fn palette(&self, query: &str) -> Vec<(String, String)> {
        vec![("tiny.thing".to_owned(), "Do Thing".to_owned())].into_iter().filter(|(id, _)| id.contains(query)).collect()
    }
    fn draw_body(&mut self, _: u8, ui: &mut Ui, _: &mut AreaCx<()>) -> bool {
        self.bodies += 1;
        ui.label("body");
        false
    }
    fn run(&mut self, action: &Action, _: &mut HostCx) {
        self.ran.push(action.id.clone());
    }
    fn key(&self, press: KeyPress, _: Option<u8>) -> Option<Action> {
        match press.key {
            Key::F(3) => Some(Action::new(actions::PALETTE)),
            Key::Char('q') if press.mods.ctrl() => Some(Action::new(actions::QUIT)),
            Key::Char('t') => Some(Action::new("tiny.thing").with("n", Value::I64(1))),
            _ => None,
        }
    }
}

#[test]
fn shell_menus_keys_and_palette() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Tiny> = Shell::new(0);
    let mut host = Tiny { ran: Vec::new(), bodies: 0 };
    h.shell_frame(&mut shell, &mut host);
    assert_eq!(host.bodies, 1);

    // A key the widgets ignore reaches the host and becomes an action.
    h.key(Key::Char('t'));
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.ran, vec!["tiny.thing"]);

    // The title bar's File menu opens and its row runs.
    let file = h.rect_of(WidgetId::ROOT.with("titlebar").with("file")).expect("File menu button");
    h.move_to(file.center());
    h.press();
    h.shell_frame(&mut shell, &mut host);
    h.release();
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(shell.popup_open(), "menu open");
    let row = h.rect_of(WidgetId::ROOT.with("popup").with("menu").with("item").with_index(0)).expect("menu row");
    h.move_to(row.center());
    h.press();
    h.shell_frame(&mut shell, &mut host);
    h.release();
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.ran, vec!["tiny.thing", "tiny.thing"]);
    assert!(!shell.popup_open(), "menu closed after the pick");

    // F3 opens the palette; Enter runs the first match.
    h.key(Key::F(3));
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(shell.popup_open());
    h.key(Key::Enter);
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.ran.len(), 3);
    assert!(!shell.popup_open());

    // Ctrl+Q asks to quit.
    h.key_with(Key::Char('q'), Modifiers::CTRL);
    let out = h.shell_settle(&mut shell, &mut host, 3);
    assert!(out.quit);
}
