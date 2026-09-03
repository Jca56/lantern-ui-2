//! The widget gallery, and a small complete [`Host`] to copy from.
//! `cargo run -p lntrn-demo`.
//!
//! Three editors (Gallery, Preferences, Notes), a File menu on the title
//! bar, a command palette (F3), a keymap, a right-click context menu with a
//! tool strip, a bar, a submenu, a live property panel and a custom row —
//! every door into the shell, opened once.

use lntrn_app::lntrn_render::{Gpu, Images};
use lntrn_app::{AppConfig, AppHost, run, wgpu};
use lntrn_image::Image;
use lntrn_props::{Reflect, Value, props};
use lntrn_ui::gallery::{self, GalleryState};
use lntrn_ui::keymap::CTX_WINDOW;
use lntrn_ui::{Action, AreaCx, Axis, ContextMenu, Dialog, Host, HostCx, Icon, Item, Key, KeyConfig, KeyItem, KeyPress, Menu, MenuItem, Modifiers, Shell, ShellRequest, Tool, Trigger, Ui, actions, prefs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Editor {
    Gallery,
    Preferences,
    Notes,
    Keys,
    Empty,
}

const EDITORS: [Editor; 5] = [Editor::Gallery, Editor::Preferences, Editor::Notes, Editor::Keys, Editor::Empty];

/// Palette entries: (action id, label).
const PALETTE: [(&str, &str); 8] = [
    ("demo.open", "Open File…"),
    ("demo.open_image", "Open Picture…"),
    ("demo.save_as", "Save As…"),
    ("demo.reset", "Reset Gallery"),
    ("demo.about", "About"),
    (actions::MAXIMIZE, "Maximize Area"),
    (actions::PALETTE, "Command Palette"),
    (actions::QUIT, "Quit"),
];

props! {
    /// A knob for the context menu's live panel.
    pub struct CountBy {
        /// How far the click counter jumps.
        pub amount: i64 = 10 => { id: 1, hard: 1..=100, soft: 1..=50 },
    }
}

struct Demo {
    gallery: GalleryState,
    keys: KeyConfig,
    notes: String,
    status: String,
    /// Mirror of the shell preference, for the Help menu's check mark.
    ffm: bool,
    /// A decoded picture waiting for the GPU (uploaded in `after_rebuild`).
    pending_image: Option<(String, Image)>,
}

/// A picture made from arithmetic: a sky gradient, a sun, rolling hills.
fn sample_picture() -> Image {
    let (w, h) = (640u32, 400u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64 / w as f64, y as f64 / h as f64);
            let hill = 0.62 + 0.08 * (fx * 9.0).sin() + 0.05 * (fx * 23.0 + 1.0).cos();
            let (r, g, b) = if fy > hill {
                let t = (fy - hill) * 3.0;
                (0.16 + 0.1 * t, 0.55 - 0.2 * t, 0.2)
            } else {
                let d = ((fx - 0.72) * 1.6).hypot(fy - 0.28);
                if d < 0.11 {
                    (1.0, 0.85, 0.35)
                } else {
                    (0.25 + 0.5 * fy, 0.45 + 0.4 * fy, 0.9)
                }
            };
            rgba.extend([(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 255]);
        }
    }
    Image::new(w, h, rgba)
}

impl Demo {
    fn new() -> Self {
        use Key::*;
        let ctrl = Modifiers::CTRL;
        let shift = Modifiers::SHIFT;
        let none = Modifiers::NONE;
        let mut keys = KeyConfig::default();
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('q'), ctrl), actions::QUIT));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(F(3), none), actions::PALETTE));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Space, ctrl), actions::MAXIMIZE));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('o'), ctrl), "demo.open"));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('s'), ctrl | shift), "demo.save_as"));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('r'), ctrl), "demo.reset"));
        keys.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Char('f'), ctrl), actions::MENU).with("menu", Value::Str("file".into())));
        Self {
            gallery: GalleryState::default(),
            ffm: false,
            pending_image: None,
            keys,
            notes: "Right-click the gallery for a context menu. F3 opens the palette. Tab walks the widgets.".to_owned(),
            status: "Ctrl+F: File · F3: palette · Ctrl+Space: maximize · Ctrl+Q: quit".to_owned(),
        }
    }

    /// The gallery's context menu, built from its live state.
    fn gallery_menu(&self, pos: lntrn_math::Vec2) -> ContextMenu {
        let g = &self.gallery;
        let count = |label: &str, by: i64| Item::action(label, Action::new("demo.count").with("by", Value::I64(by)));
        let tab_tool = |icon, tip: &str, tab: usize| Tool::new(icon, tip, Action::new("demo.tab").with("tab", Value::I64(tab as i64))).active(g.tab == tab);
        ContextMenu::new(&format!("Gallery · {} clicks", g.clicks), pos)
            .tab(
                "Counter",
                vec![
                    count("Count One", 1),
                    Item::sub("Count More", vec![count("Ten", 10), count("A Hundred", 100), count("Back One", -1)]),
                    Item::Separator,
                    Item::panel("Count By", "demo.count_by", Box::new(CountBy::default())),
                    Item::action("Reset", Action::new("demo.reset")),
                ],
            )
            .tab("Stats", vec![Item::header("Live state"), Item::custom("stats")])
            .tools([
                Tool::new(Icon::Plus, "Count ten", Action::new("demo.count").with("by", Value::I64(10))),
                Tool::new(Icon::Minus, "Reset the counter", Action::new("demo.reset")),
                Tool::new(Icon::Grid, "Snap to grid (a toggle)", Action::new("demo.toggle_b")).active(g.toggle_b),
            ])
            .bar(vec![tab_tool(Icon::Solid, "Controls tab", 0), tab_tool(Icon::Edge, "Text tab", 1), tab_tool(Icon::Face, "Lists tab", 2)])
            .wide()
    }
}

impl Host for Demo {
    type Editor = Editor;
    type AreaState = ();

    fn editors(&self) -> &[Editor] {
        &EDITORS
    }

    fn editor_label(&self, editor: Editor) -> &str {
        match editor {
            Editor::Gallery => "Widget Gallery",
            Editor::Preferences => "Preferences",
            Editor::Notes => "Notes",
            Editor::Keys => "Key Bindings",
            Editor::Empty => "Empty",
        }
    }

    fn title(&self) -> String {
        "Lantern UI".to_owned()
    }

    fn status(&self) -> String {
        self.status.clone()
    }

    fn title_menus(&self) -> &[(&str, &str)] {
        &[("File", "file"), ("Help", "help")]
    }

    fn menu(&self, name: &str) -> Option<Menu> {
        Some(match name {
            "file" => Menu::new(
                "File",
                vec![
                    MenuItem::new("Open…", Action::new("demo.open")),
                    MenuItem::new("Open Picture…", Action::new("demo.open_image")),
                    MenuItem::new("Save As…", Action::new("demo.save_as")),
                    MenuItem::new("Reset Gallery…", Action::new("demo.reset_ask")),
                    MenuItem::new("Quit", Action::new(actions::QUIT)),
                ],
            ),
            "help" => Menu::new(
                "Help",
                vec![
                    MenuItem::new("Command Palette", Action::new(actions::PALETTE)),
                    MenuItem::new("Maximize Area", Action::new(actions::MAXIMIZE)),
                    MenuItem::pref_toggle("Focus Follows Mouse", "focus_follows_mouse", self.ffm),
                    MenuItem::new("About", Action::new("demo.about")),
                ],
            ),
            _ => return None,
        })
    }

    fn palette(&self, query: &str) -> Vec<(String, String)> {
        let q = query.to_lowercase();
        PALETTE.iter().filter(|(id, label)| q.is_empty() || id.contains(&q) || label.to_lowercase().contains(&q)).map(|(id, label)| ((*id).to_owned(), (*label).to_owned())).collect()
    }

    fn draw_body(&mut self, editor: Editor, ui: &mut Ui, cx: &mut AreaCx<()>) -> bool {
        match editor {
            Editor::Gallery => {
                gallery::draw(ui, &mut self.gallery);
                let over_popup = ui.state.popup.is_some_and(|(r, _)| r.contains(ui.state.pointer));
                if ui.state.right_pressed && ui.clip().contains(ui.state.pointer) && !over_popup {
                    let menu = self.gallery_menu(ui.state.pointer);
                    cx.request(ShellRequest::ContextMenu(Box::new(menu)));
                }
                false
            }
            Editor::Preferences => {
                self.ffm = cx.prefs.focus_follows_mouse;
                prefs::draw(ui, cx.prefs)
            }
            Editor::Notes => {
                ui.heading("Notes");
                ui.label_dim(&format!("Area {} · {} · Ctrl+Enter saves", cx.area, if cx.active { "focused" } else { "not focused" }));
                if ui.text_area("notes", &mut self.notes, None).committed {
                    cx.request(ShellRequest::PathDialog { action: Action::new("demo.saved"), save: true, suggest: std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join("notes.txt").display().to_string() });
                }
                false
            }
            Editor::Keys => {
                ui.heading("Key Bindings");
                ui.label_dim("Click a key to rebind it; type an action id beside it.");
                ui.scroll_area("keys", None, |ui| {
                    ui.keymap_editor("keys", &mut self.keys);
                });
                false
            }
            Editor::Empty => {
                ui.label_dim("Pick an editor from the header.");
                false
            }
        }
    }

    fn run(&mut self, action: &Action, cx: &mut HostCx) {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
        let path = || action.arg("path").and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None }).unwrap_or_default();
        let int = |name: &str| action.arg(name).and_then(|v| if let Value::I64(n) = v { Some(*n) } else { None }).unwrap_or(0);
        match action.id.as_str() {
            "demo.open" => cx.request(ShellRequest::PathDialog { action: Action::new("demo.opened"), save: false, suggest: home.join("notes.txt").display().to_string() }),
            "demo.opened" => {
                self.status = format!("Opened {}", path());
                self.notes = std::fs::read_to_string(path()).unwrap_or_else(|e| format!("could not read: {e}"));
                cx.toast(&self.status.clone());
            }
            "demo.open_image" => cx.request(ShellRequest::PathDialog { action: Action::new("demo.image_opened"), save: false, suggest: home.join("Pictures").join("picture.png").display().to_string() }),
            "demo.image_opened" => {
                let p = path();
                match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|bytes| lntrn_image::decode(&bytes).map_err(|e| format!("{e:?}"))) {
                    Ok(img) => {
                        self.status = format!("Decoded {} ({}×{})", p, img.width, img.height);
                        self.pending_image = Some((p, img));
                        self.gallery.tab = 4;
                    }
                    Err(e) => cx.request(ShellRequest::Dialog(Dialog::notice("Could not open the picture", &format!("{p}\n{e}")))),
                }
            }
            "demo.save_as" => cx.request(ShellRequest::PathDialog { action: Action::new("demo.saved"), save: true, suggest: home.join("notes.txt").display().to_string() }),
            "demo.saved" => {
                self.status = match std::fs::write(path(), &self.notes) {
                    Ok(()) => format!("Saved {}", path()),
                    Err(e) => format!("could not save: {e}"),
                };
                cx.toast(&self.status.clone());
            }
            "demo.reset_ask" => cx.request(ShellRequest::Dialog(Dialog::confirm("Reset the gallery?", "Every knob, toggle and counter goes back to how it started.", "Reset", Action::new("demo.reset")))),
            "demo.reset" => {
                self.gallery = GalleryState::default();
                self.status = "Gallery reset".to_owned();
                cx.toast("Gallery reset");
            }
            "demo.count" => self.gallery.clicks = (self.gallery.clicks as i64 + int("by")).max(0) as u32,
            "demo.tab" => self.gallery.tab = int("tab") as usize,
            "demo.toggle_b" => self.gallery.toggle_b = !self.gallery.toggle_b,
            "demo.about" => cx.request(ShellRequest::Dialog(Dialog::notice("Lantern UI 0.2", "Rust, wgpu and winit. Everything else is ours: math, reflection, text, rendering, widgets."))),
            other => self.status = format!("unknown action {other}"),
        }
        cx.rebuild();
    }

    fn apply(&mut self, action: &str, props: &dyn Reflect, adjust: bool, _cx: &mut HostCx) -> bool {
        if action != "demo.count_by" {
            return false;
        }
        let Some(Value::I64(by)) = props.get_by_name("amount") else {
            return false;
        };
        self.gallery.clicks += by as u32;
        self.status = if adjust { format!("Adjusted: +{by}") } else { format!("Applied: +{by}") };
        true
    }

    fn draw_item(&mut self, key: &str, ui: &mut Ui, _cx: &mut HostCx) -> bool {
        if key == "stats" {
            let g = &self.gallery;
            ui.label(&format!("{} clicks", g.clicks));
            ui.label(&format!("slider {:.2} · number {:.2} · count {}", g.slider, g.number, g.count));
            ui.label(&format!("{} chars of text", g.text.chars().count()));
        }
        false
    }

    fn key(&self, press: KeyPress, _editor: Option<Editor>) -> Option<Action> {
        self.keys.resolve(&[CTX_WINDOW], &press.to_event(), |_| true).map(KeyItem::action)
    }

    fn refresh_context_menu(&mut self, menu: &mut ContextMenu) {
        *menu = self.gallery_menu(menu.pos).keep_view_of(menu);
    }
}

impl AppHost for Demo {
    fn init_gpu(&mut self, gpu: &Gpu, _format: wgpu::TextureFormat, images: &mut Images) {
        self.gallery.image = Some(images.add(gpu, &sample_picture()));
        self.gallery.image_name = "Made from arithmetic".to_owned();
    }

    fn after_rebuild(&mut self, gpu: &Gpu, images: &mut Images, _shell: &mut Shell<Self>) -> bool {
        let Some((name, img)) = self.pending_image.take() else {
            return false;
        };
        self.gallery.image = Some(match self.gallery.image {
            Some(old) => images.replace(gpu, old, &img),
            None => images.add(gpu, &img),
        });
        self.gallery.image_name = name;
        true
    }
}

fn main() {
    let mut shell = Shell::new(Editor::Gallery);
    if let Some(right) = shell.screen.split(0, Axis::Horizontal, 0.62, Editor::Preferences) {
        shell.screen.split(right, Axis::Vertical, 0.6, Editor::Notes);
    }
    run(AppConfig { title: "Lantern UI".into(), app_id: "lntrn-demo".into(), ..AppConfig::default() }, Demo::new(), shell);
}
