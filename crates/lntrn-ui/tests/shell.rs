//! Shell features through a tiny host: dialogs, toasts, maximize.

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Action, AreaCx, Axis, Dialog, Host, HostCx, Key, KeyPress, Modifiers, Shell, ShellRequest, Ui, WidgetId, actions};

#[derive(Default)]
struct Tiny {
    ran: Vec<String>,
    /// What the Rename dialog's field holds.
    name: String,
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
    fn draw_body(&mut self, _: u8, ui: &mut Ui, _: &mut AreaCx<()>) -> bool {
        ui.label("body");
        false
    }
    fn run(&mut self, action: &Action, cx: &mut HostCx) {
        self.ran.push(action.id.clone());
        match action.id.as_str() {
            "ask" => cx.request(ShellRequest::Dialog(Dialog::confirm("Delete everything?", "This cannot be undone.", "Delete", Action::new("deleted")))),
            "notice" => cx.request(ShellRequest::Dialog(Dialog::notice("Hello", "Just so you know."))),
            "toast" => cx.toast("Saved"),
            "rename" => cx.request(ShellRequest::Dialog(Dialog::new("Rename", "").button("Cancel", None).button("Rename", Some(Action::new("renamed"))).default_button(1).content("name"))),
            _ => {}
        }
    }
    fn draw_item(&mut self, key: &str, ui: &mut Ui, cx: &mut HostCx) -> bool {
        if key == "name" {
            if ui.state.focus.is_none() {
                ui.state.focus = Some(ui.id("name"));
            }
            if ui.text_field("name", &mut self.name).committed {
                cx.request(ShellRequest::DialogDefault);
            }
        }
        false
    }
    fn key(&self, press: KeyPress, _: Option<u8>) -> Option<Action> {
        match press.key {
            Key::Char('d') => Some(Action::new("ask")),
            Key::Char('n') => Some(Action::new("notice")),
            Key::Char('t') => Some(Action::new("toast")),
            Key::Char('r') => Some(Action::new("rename")),
            Key::Space if press.mods.ctrl() => Some(Action::new(actions::MAXIMIZE)),
            _ => None,
        }
    }
}

#[test]
fn a_dialog_can_hold_the_hosts_widgets() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Tiny> = Shell::new(0);
    let mut host = Tiny::default();
    h.shell_frame(&mut shell, &mut host);
    h.key(Key::Char('r'));
    h.shell_settle(&mut shell, &mut host, 4);
    assert!(shell.popup_open());
    let field = h.rect_of(WidgetId::ROOT.with("popup").with("popup").with("content").with("name")).expect("the field inside the dialog");
    let ok = h.rect_of(WidgetId::ROOT.with("popup").with("popup").with_index(1).with("Rename")).expect("the Rename button");
    assert!(ok.min.y >= field.max.y, "buttons below the content: {ok:?} under {field:?}");
    h.type_text("Plans");
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.name, "Plans", "the field had focus from the start");
    // Enter in the field: the host asks for the default button.
    h.key(Key::Enter);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(!shell.popup_open());
    assert_eq!(host.ran, vec!["rename", "renamed"]);
    // Escape closes without running anything, field or no field.
    h.key(Key::Char('r'));
    h.shell_settle(&mut shell, &mut host, 4);
    assert!(shell.popup_open());
    h.key(Key::Escape);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(!shell.popup_open());
    assert_eq!(host.ran, vec!["rename", "renamed", "rename"]);
}

fn press_release<H: Host>(h: &mut Harness, shell: &mut Shell<H>, host: &mut H, at: Vec2) {
    h.move_to(at);
    h.press();
    h.shell_frame(shell, host);
    h.release();
    h.shell_settle(shell, host, 3);
}

#[test]
fn confirm_dialog_runs_its_action_on_enter_and_stays_modal() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Tiny> = Shell::new(0);
    let mut host = Tiny::default();
    h.shell_frame(&mut shell, &mut host);
    h.key(Key::Char('d'));
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(shell.popup_open(), "the dialog is up");
    // A press outside is swallowed; the dialog stays.
    press_release(&mut h, &mut shell, &mut host, Vec2::new(20.0, 600.0));
    assert!(shell.popup_open(), "modal");
    // Enter presses the default (Delete) button.
    h.key(Key::Enter);
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(host.ran, vec!["ask", "deleted"]);
    assert!(!shell.popup_open());

    // Escape closes without running anything; so does the Cancel button.
    h.key(Key::Char('d'));
    h.shell_settle(&mut shell, &mut host, 3);
    h.key(Key::Escape);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(!shell.popup_open());
    h.key(Key::Char('d'));
    h.shell_settle(&mut shell, &mut host, 3);
    let cancel = h.rect_of(WidgetId::ROOT.with("popup").with("popup").with_index(0).with("Cancel")).expect("Cancel button");
    press_release(&mut h, &mut shell, &mut host, cancel.center());
    assert!(!shell.popup_open());
    assert_eq!(host.ran, vec!["ask", "deleted", "ask", "ask"], "cancel ran nothing");

    // A notice: Tab reaches its OK button, Space presses it.
    h.key(Key::Char('n'));
    h.shell_settle(&mut shell, &mut host, 3);
    h.key(Key::Tab);
    h.shell_settle(&mut shell, &mut host, 3);
    h.key(Key::Space);
    h.shell_settle(&mut shell, &mut host, 3);
    assert!(!shell.popup_open());
}

#[test]
fn toasts_appear_then_fade_away() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Tiny> = Shell::new(0);
    let mut host = Tiny::default();
    h.shell_frame(&mut shell, &mut host);
    h.key(Key::Char('t'));
    let out = h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(shell.toasts().len(), 1);
    assert_eq!(shell.toasts()[0].text, "Saved");
    assert!(out.wake_after.is_some_and(|w| w > 1.0), "wakes up when the fade should start: {:?}", out.wake_after);
    h.advance(3.7);
    let out = h.shell_frame(&mut shell, &mut host);
    assert!(out.wake_after.is_some_and(|w| w < 0.1), "fading: frames every tick");
    h.advance(1.0);
    let out = h.shell_frame(&mut shell, &mut host);
    assert!(shell.toasts().is_empty(), "gone after four seconds");
    assert_eq!(out.wake_after, None, "and the app sleeps again");
}

#[test]
fn maximize_by_key_and_by_menu() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Tiny> = Shell::new(0);
    let right = shell.screen.split(0, Axis::Horizontal, 0.5, 1).unwrap();
    let mut host = Tiny::default();
    h.shell_frame(&mut shell, &mut host);
    assert_eq!(shell.screen.layouts().len(), 2);
    shell.screen.active = Some(right);
    h.key_with(Key::Space, Modifiers::CTRL);
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(shell.screen.maximized, Some(right));
    assert_eq!(shell.screen.layouts().len(), 1);
    assert_eq!(shell.screen.layouts()[0].area, right);
    // The header's ⋮ menu offers Restore first.
    let menu = h.rect_of(WidgetId::ROOT.with_u64(right as u64).with("header").with("⋮")).expect("area menu button");
    press_release(&mut h, &mut shell, &mut host, menu.center());
    let restore = h.rect_of(WidgetId::ROOT.with_u64(right as u64).with("header").with("⋮").with("item").with_index(0)).expect("Restore row");
    press_release(&mut h, &mut shell, &mut host, restore.center());
    assert_eq!(shell.screen.maximized, None);
    assert_eq!(shell.screen.layouts().len(), 2, "back to the split");
}

#[test]
fn dragging_a_header_onto_another_area_swaps_them() {
    let mut h = Harness::new(1000.0, 700.0);
    let mut shell: Shell<Tiny> = Shell::new(0);
    let right = shell.screen.split(0, Axis::Horizontal, 0.5, 1).unwrap();
    let mut host = Tiny::default();
    h.shell_frame(&mut shell, &mut host);
    let grip = h.rect_of(WidgetId::ROOT.with_u64(0).with("header").with("grip")).expect("header grip");
    let target = shell.screen.layout_of(right).unwrap().body.center();
    h.move_to(grip.center());
    h.press();
    h.shell_frame(&mut shell, &mut host);
    h.move_to(target);
    let out = h.shell_frame(&mut shell, &mut host);
    assert_eq!(out.cursor, lntrn_ui::CursorIcon::Grabbing, "a drag is under way");
    h.release();
    h.shell_settle(&mut shell, &mut host, 3);
    assert_eq!(shell.screen.area(0).unwrap().editor, 1, "the left area now hosts the right's editor");
    assert_eq!(shell.screen.area(right).unwrap().editor, 0);
    assert_eq!(shell.screen.active, Some(right));
    // A plain click on the grip (no movement) swaps nothing.
    h.advance(1.0);
    press_release(&mut h, &mut shell, &mut host, grip.center());
    assert_eq!(shell.screen.area(0).unwrap().editor, 1);
}
