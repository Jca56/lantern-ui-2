//! The smallest complete Lantern app. Copy this file into a new crate's
//! `src/main.rs`, give the crate `lntrn-app` and `lntrn-ui` as
//! dependencies, and grow from here. To see it run as it is:
//! `cargo run -p lntrn-app --example template`.
//!
//! A Lantern app is a [`Host`]: it names its editors, draws each one with
//! immediate-mode widgets, and carries out the actions its menus, keys
//! and palette produce. Everything around that (the title bar, areas you
//! can split and swap, popups, the preferences editor, the clipboard,
//! files dropped in, the debug overlay) comes from `lntrn-ui`; the window
//! comes from `lntrn-app`.

use lntrn_app::{AppConfig, AppHost, run};
use lntrn_ui::keymap::CTX_WINDOW;
use lntrn_ui::{Action, AreaCx, Host, HostCx, Key, KeyConfig, KeyItem, KeyPress, Menu, MenuItem, Modifiers, Shell, Trigger, Ui, actions, prefs};

/// The kinds of editor an area can show. Add yours here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Editor {
    Main,
    Preferences,
}

const EDITORS: [Editor; 2] = [Editor::Main, Editor::Preferences];

/// Your app's state. The shell never looks inside.
struct App {
    keys: KeyConfig,
    name: String,
    greetings: u32,
}

impl App {
    fn new() -> Self {
        let mut keys = KeyConfig::default();
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Key::Char('q'), Modifiers::CTRL), actions::QUIT));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Key::F(3), Modifiers::NONE), actions::PALETTE));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Key::Char('g'), Modifiers::CTRL), "app.greet"));
        Self { keys, name: "world".to_owned(), greetings: 0 }
    }

    fn greet(&mut self, cx: &mut HostCx) {
        self.greetings += 1;
        cx.toast(&format!("Hello, {}!", self.name));
    }
}

impl Host for App {
    type Editor = Editor;
    /// Per-area data (a camera, a scroll position) would go here.
    type AreaState = ();

    fn editors(&self) -> &[Editor] {
        &EDITORS
    }

    fn editor_label(&self, editor: Editor) -> &str {
        match editor {
            Editor::Main => "Main",
            Editor::Preferences => "Preferences",
        }
    }

    fn title(&self) -> String {
        "My Lantern App".to_owned()
    }

    /// Menus on the title bar: (label, name). Rows get their key hints
    /// and preference check marks filled in by the shell.
    fn title_menus(&self) -> &[(&str, &str)] {
        &[("File", "file")]
    }

    fn menu(&self, name: &str) -> Option<Menu> {
        (name == "file").then(|| {
            Menu::new(
                "File",
                vec![
                    MenuItem::new("Greet", Action::new("app.greet")),
                    MenuItem::separator(),
                    MenuItem::pref_toggle("Reduce Motion", "reduce_motion"),
                    MenuItem::pref_toggle("Debug Overlay", "debug_overlay"),
                    MenuItem::separator(),
                    MenuItem::new("Quit", Action::new(actions::QUIT)),
                ],
            )
        })
    }

    /// F3 opens the palette; these are its entries: (action id, label).
    fn palette(&self, query: &str) -> Vec<(String, String)> {
        let q = query.to_lowercase();
        [("app.greet", "Greet"), (actions::QUIT, "Quit")].iter().filter(|(_, label)| label.to_lowercase().contains(&q)).map(|(id, label)| (id.to_string(), label.to_string())).collect()
    }

    fn key_hint(&self, action: &Action) -> Option<String> {
        self.keys.hint_for(action)
    }

    /// The body of an area, re-declared on every rebuild. Return `true`
    /// when something changed that other areas show.
    fn draw_body(&mut self, editor: Editor, ui: &mut Ui, cx: &mut AreaCx<()>) -> bool {
        match editor {
            Editor::Main => {
                ui.heading("Hello");
                ui.labelled("Name", |ui| {
                    ui.text_field("name", &mut self.name);
                });
                ui.row(|ui| {
                    if ui.button("Greet").clicked {
                        self.greet(&mut cx.host());
                    }
                    ui.label_dim(&format!("{} greetings so far", self.greetings));
                });
                ui.paragraph("Right-click for nothing yet, Ctrl+G to greet, F3 for the palette, Ctrl+Q to quit. Split this area from the ⋮ menu in its header.");
                false
            }
            // The shell's own preferences editor, free of charge.
            Editor::Preferences => prefs::draw(ui, cx.prefs),
        }
    }

    /// Menu rows, palette entries and key bindings all land here. The
    /// `shell.*` ids (quit, palette, preference toggles) never do.
    fn run(&mut self, action: &Action, cx: &mut HostCx) {
        match action.id.as_str() {
            "app.greet" => self.greet(cx),
            other => cx.toast(&format!("unknown action {other}")),
        }
    }

    fn key(&self, press: KeyPress, _editor: Option<Editor>) -> Option<Action> {
        self.keys.resolve(&[CTX_WINDOW], &press.to_event(), |_| true).map(KeyItem::action)
    }
}

/// GPU hooks (pictures, a 3D view) are optional; a UI-only app needs none.
impl AppHost for App {}

fn main() {
    let config = AppConfig { title: "My Lantern App".into(), app_id: "lntrn-template".into(), ..AppConfig::default() };
    run(config, App::new(), Shell::new(Editor::Main));
}
