//! Named menus through a tiny host: rows, rules, greyed rows, submenus,
//! key hints, the keyboard, and switching menus along the title bar.

use std::cell::RefCell;

use lntrn_math::Vec2;
use lntrn_props::Value;
use lntrn_ui::keymap::CTX_WINDOW;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Action, AreaCx, Host, HostCx, Key, KeyConfig, KeyItem, KeyPress, Menu, MenuItem, Modifiers, Shell, Trigger, Ui, WidgetId, actions};

#[derive(Default)]
struct Menus {
    ran: Vec<String>,
    keys: KeyConfig,
    /// Actions the shell asked a key hint for.
    asked: RefCell<Vec<String>>,
}

impl Menus {
    fn new() -> Self {
        let mut keys = KeyConfig::default();
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Key::Char('o'), Modifiers::CTRL), "open"));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Key::F(1), Modifiers::NONE), actions::MENU).with("menu", Value::Str("file".into())));
        Self { keys, ..Self::default() }
    }
}

impl Host for Menus {
    type Editor = u8;
    type AreaState = ();
    fn editors(&self) -> &[u8] {
        &[0]
    }
    fn editor_label(&self, _: u8) -> &str {
        "Main"
    }
    fn title(&self) -> String {
        "Menus".into()
    }
    fn title_menus(&self) -> &[(&str, &str)] {
        &[("File", "file"), ("Edit", "edit")]
    }
    fn menu(&self, name: &str) -> Option<Menu> {
        Some(match name {
            "file" => Menu::new(
                "File",
                vec![
                    MenuItem::new("Open…", Action::new("open")),
                    MenuItem::separator(),
                    MenuItem::new("Save", Action::new("save")).disabled(),
                    MenuItem::sub("Recent", vec![MenuItem::new("a.txt", Action::new("recent_a")), MenuItem::new("b.txt", Action::new("recent_b"))]),
                    MenuItem::new("Quit", Action::new("quit")).hint("Ctrl+Q"),
                ],
            ),
            "edit" => Menu::new("Edit", vec![MenuItem::new("Undo", Action::new("undo")), MenuItem::new("Redo", Action::new("redo"))]),
            _ => return None,
        })
    }
    fn key_hint(&self, action: &Action) -> Option<String> {
        self.asked.borrow_mut().push(action.id.clone());
        self.keys.hint_for(action)
    }
    fn draw_body(&mut self, _: u8, ui: &mut Ui, _: &mut AreaCx<()>) -> bool {
        ui.label("body");
        false
    }
    fn run(&mut self, action: &Action, _: &mut HostCx) {
        self.ran.push(action.id.clone());
    }
    fn key(&self, press: KeyPress, _: Option<u8>) -> Option<Action> {
        self.keys.resolve(&[CTX_WINDOW], &press.to_event(), |_| true).map(KeyItem::action)
    }
}

fn row(i: usize) -> WidgetId {
    WidgetId::ROOT.with("popup").with("menu").with("item").with_index(i)
}

fn sub_row(j: usize) -> WidgetId {
    WidgetId::ROOT.with("popup").with("menu").with("sub").with_index(j)
}

fn label(name: &str) -> WidgetId {
    WidgetId::ROOT.with("titlebar").with(name)
}

fn click(h: &mut Harness, shell: &mut Shell<Menus>, host: &mut Menus, at: Vec2) {
    h.move_to(at);
    h.press();
    h.shell_frame(shell, host);
    h.release();
    h.shell_settle(shell, host, 3);
}

fn open_file_menu(h: &mut Harness, shell: &mut Shell<Menus>, host: &mut Menus) {
    let file = h.rect_of(label("file")).expect("the File label");
    click(h, shell, host, file.center());
    assert!(shell.popup_open(), "the menu is up");
}

fn setup() -> (Harness, Shell<Menus>, Menus) {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Menus> = Shell::new(0);
    let mut host = Menus::new();
    h.shell_frame(&mut shell, &mut host);
    (h, shell, host)
}

#[test]
fn rows_run_rules_are_not_rows_and_greyed_rows_do_nothing() {
    let (mut h, mut shell, mut host) = setup();
    open_file_menu(&mut h, &mut shell, &mut host);
    assert_eq!(host.asked.borrow().as_slice(), ["open", "save", "recent_a", "recent_b"], "hints are looked up for action rows (submenus included), not for a row with its own hint");
    assert!(h.rect_of(row(0)).is_some(), "Open");
    assert!(h.rect_of(row(1)).is_none(), "a rule is not a row");
    let save = h.rect_of(row(2)).expect("greyed rows are still laid out");
    click(&mut h, &mut shell, &mut host, save.center());
    assert!(host.ran.is_empty(), "greyed: nothing ran");
    assert!(shell.popup_open(), "and the menu stays");
    let open = h.rect_of(row(0)).unwrap();
    click(&mut h, &mut shell, &mut host, open.center());
    assert_eq!(host.ran, vec!["open"]);
    assert!(!shell.popup_open(), "choosing closes the menu");
    // A press outside closes it too.
    open_file_menu(&mut h, &mut shell, &mut host);
    click(&mut h, &mut shell, &mut host, Vec2::new(900.0, 600.0));
    assert!(!shell.popup_open());
    assert_eq!(host.ran, vec!["open"], "nothing else ran");
}

#[test]
fn submenus_open_on_hover_and_their_rows_run() {
    let (mut h, mut shell, mut host) = setup();
    open_file_menu(&mut h, &mut shell, &mut host);
    assert!(h.rect_of(sub_row(0)).is_none(), "closed to start with");
    let recent = h.rect_of(row(3)).expect("the Recent row");
    h.move_to(recent.center());
    h.shell_settle(&mut shell, &mut host, 3);
    let b = h.rect_of(sub_row(1)).expect("submenu rows are laid out once it is open");
    assert!(b.min.x >= recent.max.x, "beside the panel, on the right: {b:?} vs {recent:?}");
    click(&mut h, &mut shell, &mut host, b.center());
    assert_eq!(host.ran, vec!["recent_b"]);
    assert!(!shell.popup_open());
    // Hovering a plain row again closes the submenu.
    open_file_menu(&mut h, &mut shell, &mut host);
    h.move_to(recent.center());
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(h.rect_of(sub_row(0)).is_some());
    h.move_to(h.rect_of(row(0)).unwrap().center());
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(h.rect_of(sub_row(0)).is_none(), "gone");
}

#[test]
fn the_keyboard_walks_rows_opens_submenus_and_chooses() {
    let (mut h, mut shell, mut host) = setup();
    h.key(Key::F(1));
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(shell.popup_open(), "a key binding opened the menu");
    h.key(Key::ArrowDown); // Open
    h.key(Key::ArrowDown); // over the rule and the greyed Save: Recent
    h.key(Key::ArrowRight);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(h.rect_of(sub_row(0)).is_some(), "Right opened the submenu");
    h.key(Key::ArrowDown); // b.txt
    h.key(Key::Enter);
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.ran, vec!["recent_b"]);
    assert!(!shell.popup_open());

    // Up from nothing lands on the last row. (The menu opens at the end of
    // the frame that sees F1, so the keys for it come in the next one.)
    h.key(Key::F(1));
    h.shell_settle(&mut shell, &mut host, 3);
    h.key(Key::ArrowUp);
    h.key(Key::Enter);
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.ran, vec!["recent_b", "quit"]);

    // Left closes a submenu without closing the menu; Escape closes the menu.
    h.key(Key::F(1));
    h.shell_settle(&mut shell, &mut host, 3);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowRight);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(h.rect_of(sub_row(0)).is_some());
    h.key(Key::ArrowLeft);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(h.rect_of(sub_row(0)).is_none());
    assert!(shell.popup_open());
    h.key(Key::Escape);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(!shell.popup_open());
    assert_eq!(host.ran.len(), 2, "nothing more ran");
}

#[test]
fn hovering_another_title_label_switches_menus() {
    let (mut h, mut shell, mut host) = setup();
    open_file_menu(&mut h, &mut shell, &mut host);
    assert!(h.rect_of(row(1)).is_none(), "File's second entry is a rule");
    assert!(h.rect_of(row(4)).is_some(), "File has five entries");
    let edit = h.rect_of(label("edit")).expect("the Edit label");
    h.move_to(edit.center());
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(shell.popup_open());
    assert!(h.rect_of(row(1)).is_some(), "Edit's second row (Redo)");
    assert!(h.rect_of(row(2)).is_none(), "and Edit has only two");
    // Sliding back reopens File.
    h.move_to(h.rect_of(label("file")).unwrap().center());
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(h.rect_of(row(4)).is_some());
    // Without a menu open, hovering a label opens nothing.
    h.key(Key::Escape);
    h.shell_settle(&mut shell, &mut host, 3);
    h.move_to(edit.center());
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(!shell.popup_open());
}
