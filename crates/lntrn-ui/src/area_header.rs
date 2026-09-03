//! One area's header: the editor dropdown, the tab strip with its `+`,
//! the host's own controls, the grip that swaps areas by dragging, and
//! the `⋮` menu (maximize, split, close a tab or the area). What the user
//! asked for comes back as [`AreaAction`]s for the shell to apply once
//! every area has drawn.

use lntrn_math::Vec2;

use crate::host::{AreaCx, Host};
use crate::screen::{AreaId, Axis};
use crate::state::CursorIcon;
use crate::ui::{Sense, Ui};

pub(crate) enum AreaAction<E> {
    Split(AreaId, Axis),
    Close(AreaId),
    Maximize(AreaId),
    SetEditor(AreaId, E),
    AddTab(AreaId, E),
    CloseTab(AreaId),
    SelectTab(AreaId, usize),
    /// Open the area's editor in a window of its own.
    Detach(AreaId),
}

/// What a header is drawn from.
pub(crate) struct HeaderIn<'a, E> {
    pub area: AreaId,
    /// The showing tab's editor.
    pub kind: E,
    pub editors: &'a [E],
    pub labels: &'a [&'a str],
    /// The labels of this area's tabs, in order.
    pub tabs: &'a [String],
    pub current: usize,
    pub maximized: bool,
}

pub(crate) struct HeaderOut<E> {
    pub actions: Vec<AreaAction<E>>,
    /// The grip was pressed: a drag to swap areas may be starting.
    pub grip_pressed: bool,
}

pub(crate) fn area_header<H: Host>(ui: &mut Ui, host: &mut H, cx: &mut AreaCx<H::AreaState>, h: HeaderIn<H::Editor>) -> HeaderOut<H::Editor> {
    let mut out = HeaderOut { actions: Vec::new(), grip_pressed: false };
    ui.row(|ui| {
        let mut idx = h.editors.iter().position(|&k| k == h.kind).unwrap_or(0);
        if ui.dropdown("editor", &mut idx, h.labels) {
            out.actions.push(AreaAction::SetEditor(h.area, h.editors[idx]));
        }
        // The tab strip: the showing tab lit, the others plain, then a `+`
        // that lists the editors to open in a new tab.
        if h.tabs.len() > 1 {
            let style = ui.text_style();
            for (i, label) in h.tabs.iter().enumerate() {
                let w = ui.measure(label, &style) + ui.m.pad * 2.0;
                let rect = ui.alloc(Vec2::new(w, ui.m.widget_h));
                let id = ui.id("tab").with_index(i);
                let mut r = ui.interact(id, rect, Sense::CLICK);
                ui.focusable(id, rect);
                ui.key_click(id, &mut r);
                if r.hovered {
                    ui.state.cursor_icon = CursorIcon::Pointer;
                }
                if i == h.current {
                    ui.raised(rect, ui.theme.accent, false);
                    ui.text_centered(label, &style, rect, ui.theme.accent_text);
                } else {
                    ui.button_face(rect, &r);
                    ui.text_centered(label, &style, rect, ui.theme.text);
                }
                ui.focus_ring(id, rect);
                if r.clicked && i != h.current {
                    out.actions.push(AreaAction::SelectTab(h.area, i));
                }
            }
        }
        if let Some(i) = ui.menu_button("+", h.labels) {
            out.actions.push(AreaAction::AddTab(h.area, h.editors[i]));
        }
        host.draw_header(h.kind, ui, cx);
        let style = ui.text_style();
        let menu_w = ui.measure("⋮", &style) + ui.m.pad * 2.0;
        let spacer = (ui.avail_width() - menu_w - ui.m.gap).max(0.0);
        // The empty stretch of the header is a grip: drag it onto another
        // area to swap the two.
        let grip = ui.alloc(Vec2::new(spacer, ui.m.widget_h));
        if ui.interact(ui.id("grip"), grip, Sense::DRAG).pressed {
            out.grip_pressed = true;
        }
        let mut rows = vec![if h.maximized { "Restore" } else { "Maximize" }, "Split Left | Right", "Split Top | Bottom"];
        rows.push("Open in New Window");
        if h.tabs.len() > 1 {
            rows.push("Close Tab");
        }
        rows.push("Close Area");
        if let Some(i) = ui.menu_button("⋮", &rows) {
            out.actions.push(match (i, rows[i]) {
                (0, _) => AreaAction::Maximize(h.area),
                (1, _) => AreaAction::Split(h.area, Axis::Horizontal),
                (2, _) => AreaAction::Split(h.area, Axis::Vertical),
                (_, "Open in New Window") => AreaAction::Detach(h.area),
                (_, "Close Tab") => AreaAction::CloseTab(h.area),
                _ => AreaAction::Close(h.area),
            });
        }
    });
    out
}
