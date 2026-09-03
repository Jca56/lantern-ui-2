//! Shell-level popups: named menus, the command palette, the path dialog,
//! and the right-click context menu. Drawn above everything; Escape or an
//! outside press closes them. Menus let that press fall through to whatever
//! is underneath (one click dismisses *and* selects); dialogs swallow it.

use lntrn_math::{Rect, Vec2};
use lntrn_props::Value;

use crate::context_menu::{self, ContextMenu};
use crate::event::Key;
use crate::file_browser::{self, FileBrowser, Verdict};
use crate::host::{Action, Dialog, Host, HostCx, MenuItem, ShellRequest, actions};
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Debug)]
pub enum Popup {
    Menu { title: String, items: Vec<MenuItem>, pos: Vec2 },
    Palette { query: String, selected: usize },
    /// A file browser that runs `action` with the chosen `path`.
    Path { action: Action, browser: FileBrowser },
    Context(Box<ContextMenu>),
    Dialog(Dialog),
}

/// What the shell does after a popup frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopupResult {
    pub close: bool,
    /// Refresh the context menu (a tool changed state it displays).
    pub refresh: bool,
}

/// Run an action: the `shell.*` ids become requests the shell carries out;
/// everything else goes to the host.
pub(crate) fn dispatch<H: Host>(host: &mut H, action: &Action, cx: &mut HostCx) {
    let str_arg = |name: &str| match action.arg(name) {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    };
    match action.id.as_str() {
        actions::MENU => cx.request(ShellRequest::Menu(str_arg("menu"))),
        actions::PALETTE => cx.request(ShellRequest::Palette),
        actions::PREF_TOGGLE => cx.request(ShellRequest::PrefToggle(str_arg("field"))),
        actions::CLOSE_POPUP => cx.request(ShellRequest::ClosePopup),
        actions::MAXIMIZE => cx.request(ShellRequest::Maximize(None)),
        actions::QUIT => cx.request(ShellRequest::Quit),
        _ => host.run(action, cx),
    }
}

/// Draw the popup and act on it. `palette_entries` are the host's matches
/// for the palette's current query.
pub(crate) fn draw<H: Host>(ui: &mut Ui, popup: &mut Popup, window: Rect, palette_entries: &[(String, String)], host: &mut H, cx: &mut HostCx) -> PopupResult {
    if let Popup::Context(menu) = popup {
        return context_menu::draw(ui, menu, window, host, cx);
    }
    let mut out = PopupResult::default();
    if ui.state.take_key(|k| k.key == Key::Escape).is_some() {
        out.close = true;
    }
    let layer = 1;
    let m = ui.m;
    let rect = match popup {
        Popup::Menu { items, pos, .. } => {
            let w = m.px(260.0);
            let h = m.widget_h * (items.len() as f64 + 1.0) + m.gap * 3.0;
            let x = pos.x.min(window.max.x - w).max(window.min.x);
            let y = pos.y.min(window.max.y - h).max(window.min.y);
            Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h))
        }
        Popup::Palette { .. } => {
            let w = m.px(600.0).min(window.width() - m.pad * 2.0);
            let h = m.widget_h * 13.0 + m.gap * 4.0;
            Rect::from_min_size(Vec2::new((window.center().x - w * 0.5).round(), window.min.y + m.px(80.0)), Vec2::new(w, h))
        }
        Popup::Path { .. } => {
            let w = m.px(1000.0).min(window.width() - m.pad * 2.0);
            let h = m.px(760.0).min(window.height() - m.pad * 2.0);
            Rect::from_min_size((window.center() - Vec2::new(w, h) * 0.5).round(), Vec2::new(w, h))
        }
        Popup::Dialog(d) => {
            let w = m.px(700.0).min(window.width() - m.pad * 2.0);
            let style = ui.text_style();
            let body_h = ui.text.measure_wrapped(&d.body, &style, (w - m.gap * 2.0 - m.pad * 2.0) as f32).height as f64;
            let heading_h = ui.heading_style().line_height() as f64 + m.gap;
            let h = m.gap * 2.0 + m.pad * 2.0 + heading_h + m.gap + body_h + m.gap * 2.0 + m.widget_h;
            Rect::from_min_size((window.center() - Vec2::new(w, h) * 0.5).round(), Vec2::new(w, h))
        }
        Popup::Context(_) => unreachable!(),
    };
    ui.state.keep_popup(rect, layer);
    if ui.state.pressed && !rect.contains(ui.state.press_pos) {
        if matches!(popup, Popup::Dialog(_)) {
            // Modal: the press goes nowhere and the dialog stays.
            ui.state.press_claimed = true;
        } else {
            out.close = true;
            if !matches!(popup, Popup::Menu { .. }) {
                ui.state.press_claimed = true;
            }
        }
    }

    let saved_layer = ui.layer();
    let saved_clip = ui.clip();
    ui.draw.set_layer(layer);
    ui.set_layer_internal(layer);
    ui.set_clip(rect);
    ui.draw.push_clip_absolute(rect.expand(m.px(20.0)));
    ui.floating_panel(rect, ui.theme.header);
    ui.draw.pop_clip();
    ui.draw.push_clip_absolute(rect);
    ui.push_id("popup");
    let content = rect.shrink(m.gap);
    ui.set_cursor(content.min);
    ui.set_avail_width(content.width());

    match popup {
        Popup::Menu { title, items, .. } => {
            ui.label_dim(title);
            for (i, item) in items.iter().enumerate() {
                ui.push_index(i);
                if ui.selectable(&item.label, item.checked.unwrap_or(false)).clicked {
                    dispatch(host, &item.action, cx);
                    out.close = true;
                }
                ui.pop_id();
            }
        }
        Popup::Palette { query, selected } => {
            let field = ui.id("query");
            if ui.state.focus.is_none() {
                ui.state.focus = Some(field);
            }
            let field_rect = ui.alloc(Vec2::new(FILL, m.widget_h));
            let r = ui.text_edit_core(field, field_rect, query);
            let n = palette_entries.len();
            if ui.state.take_key(|k| k.key == Key::ArrowDown).is_some() && n > 0 {
                *selected = (*selected + 1).min(n - 1);
            }
            if ui.state.take_key(|k| k.key == Key::ArrowUp).is_some() {
                *selected = selected.saturating_sub(1);
            }
            if r.changed {
                *selected = 0;
            }
            if r.committed
                && let Some((id, _)) = palette_entries.get(*selected)
            {
                dispatch(host, &Action::new(id), cx);
                out.close = true;
            }
            ui.space(m.gap);
            for (i, (id, label)) in palette_entries.iter().enumerate().take(11) {
                ui.push_index(i);
                let rect = ui.alloc(Vec2::new(FILL, m.widget_h));
                let rr = ui.interact(ui.id("entry"), rect, Sense::CLICK);
                if rr.hovered {
                    ui.state.cursor_icon = CursorIcon::Pointer;
                }
                let style = ui.text_style();
                let inner = Rect::new(Vec2::new(rect.min.x + m.pad, rect.min.y), Vec2::new(rect.max.x - m.pad, rect.max.y));
                if i == *selected {
                    ui.fill_shaded(rect, ui.theme.selection);
                    ui.text_in_rect(label, &style, inner, ui.theme.selection_text);
                    ui.text_right(id, &style, inner, ui.theme.selection_text);
                } else {
                    if rr.hovered {
                        let bg = ui.theme.hover(ui.theme.header);
                        ui.fill(rect, bg);
                    }
                    ui.text_in_rect(label, &style, inner, ui.theme.text);
                    ui.text_right(id, &style, inner, ui.theme.text_dim);
                }
                if rr.clicked {
                    dispatch(host, &Action::new(id), cx);
                    out.close = true;
                }
                ui.pop_id();
            }
        }
        Popup::Path { action, browser } => match file_browser::draw(ui, browser, content) {
            Verdict::Confirm(path) => {
                let chosen = action.clone().with("path", Value::Str(path.display().to_string()));
                dispatch(host, &chosen, cx);
                out.close = true;
            }
            Verdict::Cancel => out.close = true,
            Verdict::Open => {}
        },
        Popup::Dialog(d) => {
            let inner = content.shrink(m.pad);
            ui.set_cursor(inner.min);
            ui.set_avail_width(inner.width());
            ui.heading(&d.title);
            ui.paragraph(&d.body);
            ui.space(m.gap);
            let enter = ui.state.take_key(|k| k.key == Key::Enter && k.mods.is_empty()).is_some();
            let style = ui.text_style();
            let widths: Vec<f64> = d.buttons.iter().map(|(l, _)| ui.measure(l, &style) + m.pad * 2.0).collect();
            let total: f64 = widths.iter().sum::<f64>() + m.gap * (widths.len().saturating_sub(1)) as f64;
            let spacer = (inner.width() - total).max(0.0);
            let mut pressed = None;
            ui.row(|ui| {
                ui.alloc(Vec2::new(spacer, 1.0));
                for (i, (label, _)) in d.buttons.iter().enumerate() {
                    ui.push_index(i);
                    if ui.button(label).clicked || (enter && i == d.default) {
                        pressed = Some(i);
                    }
                    ui.pop_id();
                }
            });
            if let Some(i) = pressed {
                if let Some(action) = d.buttons.get(i).and_then(|(_, a)| a.clone()) {
                    dispatch(host, &action, cx);
                }
                out.close = true;
            }
        }
        Popup::Context(_) => unreachable!(),
    }

    ui.pop_id();
    ui.draw.pop_clip();
    ui.set_clip(saved_clip);
    ui.set_layer_internal(saved_layer);
    ui.draw.set_layer(saved_layer);
    if out.close {
        ui.state.focus = None;
        ui.state.request_rebuild = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::AreaId;
    use crate::ui::Ui;

    struct Nothing(Vec<String>);

    impl Host for Nothing {
        type Editor = u8;
        type AreaState = ();
        fn editors(&self) -> &[u8] {
            &[0]
        }
        fn editor_label(&self, _: u8) -> &str {
            "Empty"
        }
        fn title(&self) -> String {
            String::new()
        }
        fn draw_body(&mut self, _: u8, _: &mut Ui, _: &mut crate::host::AreaCx<()>) -> bool {
            false
        }
        fn run(&mut self, action: &Action, _: &mut HostCx) {
            self.0.push(action.id.clone());
        }
    }

    #[test]
    fn shell_actions_become_requests() {
        let mut host = Nothing(Vec::new());
        let mut requests = Vec::new();
        let mut cx = HostCx { pointer: Vec2::ZERO, requests: &mut requests };
        dispatch(&mut host, &Action::new(actions::MENU).with("menu", Value::Str("file".into())), &mut cx);
        dispatch(&mut host, &Action::new(actions::PREF_TOGGLE).with("field", Value::Str("focus_follows_mouse".into())), &mut cx);
        dispatch(&mut host, &Action::new(actions::PALETTE), &mut cx);
        dispatch(&mut host, &Action::new(actions::QUIT), &mut cx);
        dispatch(&mut host, &Action::new("mine.thing"), &mut cx);
        let got: Vec<String> = requests.iter().map(|r| format!("{r:?}")).collect();
        assert_eq!(got, vec!["Menu(\"file\")", "PrefToggle(\"focus_follows_mouse\")", "Palette", "Quit"]);
        assert_eq!(host.0, vec!["mine.thing"], "only the host's own ids reach it");
        let _: AreaId = 0;
    }
}
