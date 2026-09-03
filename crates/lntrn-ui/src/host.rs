//! The seam between the shell and an app. The shell owns the window frame,
//! the area tree, popups and preferences; everything it cannot know on its
//! own — which editors exist, what each draws, what a menu row does — it
//! asks the [`Host`]. An app implements this one trait and hands itself to
//! [`crate::Shell::frame`] every rebuild.

use std::path::PathBuf;

use lntrn_math::Vec2;
use lntrn_props::{Reflect, Value};

use crate::context_menu::ContextMenu;
use crate::event::Event;
use crate::prefs::Prefs;
use crate::screen::AreaId;
use crate::state::KeyPress;
use crate::ui::Ui;

/// One thing the user asked for: a menu row, a palette entry, a tool, a
/// key binding. The host decides what an id means; `args` carry the row's
/// overrides (the primitive to add, the path that was chosen).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Action {
    pub id: String,
    pub args: Vec<(String, Value)>,
}

impl Action {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_owned(), args: Vec::new() }
    }

    pub fn with(mut self, name: &str, v: Value) -> Self {
        self.args.push((name.to_owned(), v));
        self
    }

    pub fn arg(&self, name: &str) -> Option<&Value> {
        self.args.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// `(name, value)` pairs, borrowed.
    pub fn args_ref(&self) -> Vec<(&str, Value)> {
        self.args.iter().map(|(n, v)| (n.as_str(), v.clone())).collect()
    }
}

/// Action ids the shell carries out itself, so menus and key bindings can
/// reach shell features without the host implementing them.
pub mod actions {
    /// Open the named menu (`menu` arg) at the pointer.
    pub const MENU: &str = "shell.menu";
    /// Open the command palette.
    pub const PALETTE: &str = "shell.palette";
    /// Flip the boolean shell preference named by the `field` arg.
    pub const PREF_TOGGLE: &str = "shell.pref_toggle";
    /// Close the open popup, if any.
    pub const CLOSE_POPUP: &str = "shell.close_popup";
    /// Give the focused area the whole window, or put the layout back.
    pub const MAXIMIZE: &str = "shell.maximize";
    pub const QUIT: &str = "shell.quit";
}

/// A modal question: a title, a body, widgets of the host's if it wants
/// them, and buttons that run actions or just close. Enter presses the
/// default button; Escape closes; a press outside is swallowed.
#[derive(Clone, Debug)]
pub struct Dialog {
    pub title: String,
    pub body: String,
    /// Buttons left to right: the label, and what it runs (`None` only
    /// closes the dialog).
    pub buttons: Vec<(String, Option<Action>)>,
    /// The button Enter presses.
    pub default: usize,
    /// Widgets between the body and the buttons: the host draws them in
    /// [`Host::draw_item`] under this key (a name field for a rename, the
    /// settings of an export).
    pub content: Option<String>,
    /// Height measured last frame, when there is content.
    pub height: f64,
}

impl Dialog {
    /// A notice with one OK button.
    pub fn notice(title: &str, body: &str) -> Self {
        Self { buttons: vec![("OK".to_owned(), None)], ..Self::new(title, body) }
    }

    /// Cancel on the left, `ok` on the right running `action`; Enter confirms.
    pub fn confirm(title: &str, body: &str, ok: &str, action: Action) -> Self {
        Self { buttons: vec![("Cancel".to_owned(), None), (ok.to_owned(), Some(action))], default: 1, ..Self::new(title, body) }
    }

    /// Start with no buttons and add them with [`Dialog::button`].
    pub fn new(title: &str, body: &str) -> Self {
        Self { title: title.to_owned(), body: body.to_owned(), buttons: Vec::new(), default: 0, content: None, height: 0.0 }
    }

    /// Widgets of the host's between the body and the buttons, drawn by
    /// [`Host::draw_item`] under `key`.
    pub fn content(mut self, key: &str) -> Self {
        self.content = Some(key.to_owned());
        self
    }

    pub fn button(mut self, label: &str, action: Option<Action>) -> Self {
        self.buttons.push((label.to_owned(), action));
        self
    }

    pub fn default_button(mut self, i: usize) -> Self {
        self.default = i;
        self
    }
}

/// A row of a named menu.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub action: Action,
    /// A setting's current state, shown as a check mark.
    pub checked: Option<bool>,
    /// A greyed row does nothing when chosen.
    pub enabled: bool,
    /// Text at the right edge, normally the key binding. The shell fills
    /// it in from [`Host::key_hint`] when `None`.
    pub hint: Option<String>,
    /// Rows of a submenu; when there are any, the row opens it beside the
    /// menu instead of running `action`.
    pub sub: Vec<MenuItem>,
    /// A thin rule instead of a row.
    pub separator: bool,
}

impl MenuItem {
    pub fn new(label: &str, action: Action) -> Self {
        Self { label: label.to_owned(), action, checked: None, enabled: true, hint: None, sub: Vec::new(), separator: false }
    }

    /// A thin rule between groups of rows.
    pub fn separator() -> Self {
        Self { separator: true, ..Self::new("", Action::default()) }
    }

    /// A row that opens `items` beside the menu.
    pub fn sub(label: &str, items: Vec<MenuItem>) -> Self {
        Self { sub: items, ..Self::new(label, Action::default()) }
    }

    /// A row that shows a setting and flips it.
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    /// Greyed out: shown, but does nothing.
    pub fn disabled(self) -> Self {
        self.enabled(false)
    }

    /// Text at the right edge, instead of the key binding the shell
    /// would look up.
    pub fn hint(mut self, text: &str) -> Self {
        self.hint = Some(text.to_owned());
        self
    }

    /// A row that flips the boolean shell preference `field`.
    pub fn pref_toggle(label: &str, field: &str, on: bool) -> Self {
        Self::new(label, Action::new(actions::PREF_TOGGLE).with("field", Value::Str(field.to_owned()))).checked(on)
    }
}

/// A named menu: opened from the title bar, a key, or a [`ShellRequest`].
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(title: &str, items: Vec<MenuItem>) -> Self {
        Self { title: title.to_owned(), items }
    }
}

/// Something the host asks the shell to do. Gathered during a rebuild and
/// applied at its end.
#[derive(Clone, Debug)]
pub enum ShellRequest {
    /// Open the named menu at the pointer.
    Menu(String),
    /// Open the named menu at a point (below a button, say).
    MenuAt(String, Vec2),
    /// Open the command palette.
    Palette,
    /// Ask for a path in the file browser, then run `action` with its
    /// `path` argument set to the choice. `suggest` pre-fills the field and
    /// its extension filters the listing.
    PathDialog { action: Action, save: bool, suggest: String },
    /// Open a context menu at its `pos`.
    ContextMenu(Box<ContextMenu>),
    /// Ask a modal question.
    Dialog(Dialog),
    /// Press the open dialog's default button: what a host asks for when
    /// a text field inside the dialog commits, so Enter confirms there
    /// too.
    DialogDefault,
    /// Show a short message in the corner for a few seconds.
    Toast(String),
    /// Toggle one area (the focused one when `None`) taking the whole window.
    Maximize(Option<AreaId>),
    ClosePopup,
    /// Flip a boolean shell preference by field name.
    PrefToggle(String),
    /// Rebuild once more right after this one.
    Rebuild,
    Quit,
}

/// Who owns the pointer and keyboard this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Capture {
    /// Widgets, as usual.
    #[default]
    None,
    /// A running tool (a modal operator) owns button presses and keys. The
    /// UI still sees motion, releases, the middle button and — unless
    /// `wheel` — the wheel, so a view can be navigated mid-tool.
    Tool { wheel: bool },
}

/// What the host gets while running an action or drawing a popup item.
pub struct HostCx<'a> {
    /// Pointer in window space, physical pixels.
    pub pointer: Vec2,
    pub requests: &'a mut Vec<ShellRequest>,
}

impl HostCx<'_> {
    pub fn request(&mut self, r: ShellRequest) {
        self.requests.push(r);
    }

    pub fn rebuild(&mut self) {
        self.requests.push(ShellRequest::Rebuild);
    }

    pub fn toast(&mut self, text: &str) {
        self.requests.push(ShellRequest::Toast(text.to_owned()));
    }

    pub fn quit(&mut self) {
        self.requests.push(ShellRequest::Quit);
    }
}

/// What the host gets while drawing one area.
pub struct AreaCx<'a, S> {
    pub area: AreaId,
    /// This area's own state (a camera, a scroll position, a selection).
    pub state: &'a mut S,
    /// Whether this is the focused area.
    pub active: bool,
    /// Pointer in window space, physical pixels.
    pub pointer: Vec2,
    /// The shell's preferences, for a Preferences editor
    /// (see [`crate::prefs::draw`]).
    pub prefs: &'a mut Prefs,
    pub requests: &'a mut Vec<ShellRequest>,
}

impl<S> AreaCx<'_, S> {
    pub fn request(&mut self, r: ShellRequest) {
        self.requests.push(r);
    }

    pub fn rebuild(&mut self) {
        self.requests.push(ShellRequest::Rebuild);
    }

    pub fn toast(&mut self, text: &str) {
        self.requests.push(ShellRequest::Toast(text.to_owned()));
    }

    /// The same context without the area, for [`Host::run`].
    pub fn host(&mut self) -> HostCx<'_> {
        HostCx { pointer: self.pointer, requests: self.requests }
    }
}

/// An app, as the shell sees it.
///
/// Only [`Host::draw_body`] and [`Host::run`] are required; the rest have
/// defaults that mean "nothing of the kind". See `lntrn-demo` for a small
/// complete host.
pub trait Host {
    /// The kinds of editor an area can host. Plain data.
    type Editor: Copy + PartialEq + core::fmt::Debug;
    /// State the shell keeps per area beside its editor kind, created with
    /// `Default` when an area is made. `()` when there is none.
    type AreaState: Default + Clone;

    /// Every editor kind, in the order the area header lists them.
    fn editors(&self) -> &[Self::Editor];
    fn editor_label(&self, editor: Self::Editor) -> &str;
    /// A stable name for an editor kind, written into a saved layout.
    /// The label by default; override if labels may change.
    fn editor_id(&self, editor: Self::Editor) -> String {
        self.editor_label(editor).to_owned()
    }
    /// The editor kind a saved layout named, if it still exists.
    fn editor_from_id(&self, id: &str) -> Option<Self::Editor> {
        self.editors().iter().copied().find(|&e| self.editor_id(e) == id)
    }

    /// Text in the middle of the title bar.
    fn title(&self) -> String;
    /// Dim text right of the title (the last report, a mode).
    fn status(&self) -> String {
        String::new()
    }
    /// Menus on the left of the title bar: (label, menu name).
    fn title_menus(&self) -> &[(&str, &str)] {
        &[]
    }
    /// The rows of a named menu.
    fn menu(&self, name: &str) -> Option<Menu> {
        let _ = name;
        None
    }
    /// Command palette entries matching `query`: (action id, label).
    fn palette(&self, query: &str) -> Vec<(String, String)> {
        let _ = query;
        Vec::new()
    }
    /// The key binding that fires `action`, as text for the right edge of
    /// a menu row (`Ctrl+O`). A host with a [`crate::keymap::KeyConfig`]
    /// returns `keys.hint_for(action)`.
    fn key_hint(&self, action: &Action) -> Option<String> {
        let _ = action;
        None
    }

    /// Does this editor paint its own body (a 3D view)? Then the shell
    /// leaves the body unfilled for it.
    fn paints_body(&self, editor: Self::Editor) -> bool {
        let _ = editor;
        false
    }
    /// Controls in an area's header, after the editor dropdown.
    fn draw_header(&mut self, editor: Self::Editor, ui: &mut Ui, cx: &mut AreaCx<Self::AreaState>) {
        let _ = (editor, ui, cx);
    }
    /// The body of an area. Return `true` when something changed that other
    /// areas show (a theme, a document).
    fn draw_body(&mut self, editor: Self::Editor, ui: &mut Ui, cx: &mut AreaCx<Self::AreaState>) -> bool;

    /// Carry out an action chosen from a menu, the palette, a context menu,
    /// a tool, or a key binding. The `shell.*` ids never reach here.
    fn run(&mut self, action: &Action, cx: &mut HostCx);
    /// Apply a context-menu panel: run `action` with `props`. `adjust` means
    /// it already ran and the user changed a knob since. Return `true` on
    /// success.
    fn apply(&mut self, action: &str, props: &dyn Reflect, adjust: bool, cx: &mut HostCx) -> bool {
        let _ = (action, props, adjust, cx);
        false
    }
    /// Draw a [`crate::context_menu::Item::Custom`] row. Return `true` if it
    /// changed something.
    fn draw_item(&mut self, key: &str, ui: &mut Ui, cx: &mut HostCx) -> bool {
        let _ = (key, ui, cx);
        false
    }
    /// A key press no widget consumed, with the focused area's editor. The
    /// host resolves it against its [`crate::keymap::KeyConfig`] and hands
    /// back the action to run; the shell dispatches it like a menu row (so
    /// `shell.*` ids work here too).
    fn key(&self, press: KeyPress, editor: Option<Self::Editor>) -> Option<Action> {
        let _ = (press, editor);
        None
    }

    /// Whether a running tool owns input this frame.
    fn capture(&self) -> Capture {
        Capture::None
    }
    /// Every event of a frame while [`Host::capture`] is not `None`, in
    /// order, for the running tool.
    fn captured(&mut self, events: &[Event], cx: &mut HostCx) {
        let _ = (events, cx);
    }
    /// A tool or bar button of the open context menu ran: bring the menu's
    /// title, tools and items up to date with what changed.
    fn refresh_context_menu(&mut self, menu: &mut ContextMenu) {
        let _ = menu;
    }
    /// Files dropped on the window from outside that no
    /// [`crate::Ui::drop_zone`] took. `area` and `editor` are what was
    /// under the pointer when the window system last reported it, which
    /// not every window system keeps current during an outside drag.
    fn dropped(&mut self, paths: &[PathBuf], area: Option<AreaId>, editor: Option<Self::Editor>, cx: &mut HostCx) {
        let _ = (paths, area, editor, cx);
    }
}
