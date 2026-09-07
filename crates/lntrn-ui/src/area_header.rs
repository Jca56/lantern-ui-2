//! One area's header: the tab strip (a tab per editor stacked here; the
//! lit one names the editor and, clicked, lists the editors to switch it
//! to, U034), its `+`, the host's own controls, the grip that moves the area
//! by dragging (onto another area to swap or dock beside it, to a window
//! edge to span it), and the `⋮` menu (maximize, split, dock at an edge,
//! close a tab or the area). A tab has a right-click menu of its own
//! (rename in place, close) and drags along the strip to another place.
//! What the user asked for comes back as [`AreaAction`]s for the shell
//! to apply once every area has drawn.

use lntrn_math::{Color, Rect, Vec2};

use crate::host::{AreaCx, Host};
use crate::screen::{AreaId, Axis};
use crate::screen_dock::Side;
use crate::state::CursorIcon;
use crate::ui::{Sense, Ui};
use crate::widgets::TextOpts;

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
    /// Move the area to the window's side, spanning it.
    Dock(AreaId, Side),
    /// Start typing a name for a tab.
    StartRename(AreaId, usize),
    /// The name typed (`None`: back to the editor's label); ends the rename.
    RenameTab(AreaId, usize, Option<String>),
    RenameCancel,
    MoveTab(AreaId, usize, usize),
    CloseTabAt(AreaId, usize),
}

/// The pointer has to move this far from the press before a tab drags.
const DRAG_START: f64 = 8.0;

/// What a header is drawn from.
pub(crate) struct HeaderIn<'a, E> {
    pub area: AreaId,
    /// The showing tab's editor.
    pub kind: E,
    pub editors: &'a [E],
    pub labels: &'a [&'a str],
    /// The labels of this area's tabs, in order (a given name, or the
    /// editor's), and the editors' own labels for the rename field's hint.
    pub tabs: &'a [String],
    pub defaults: &'a [String],
    pub current: usize,
    /// A tab of this area being renamed: which, the text so far, whether
    /// the field takes focus this draw.
    pub rename: Option<&'a mut (usize, String, bool)>,
    pub maximized: bool,
    /// The only area on screen: nothing to dock beside.
    pub alone: bool,
}

pub(crate) struct HeaderOut<E> {
    pub actions: Vec<AreaAction<E>>,
    /// The grip was pressed: a drag to move the area may be starting.
    pub grip_pressed: bool,
}

pub(crate) fn area_header<H: Host>(ui: &mut Ui, host: &mut H, cx: &mut AreaCx<H::AreaState>, mut h: HeaderIn<H::Editor>) -> HeaderOut<H::Editor> {
    let mut out = HeaderOut { actions: Vec::new(), grip_pressed: false };
    ui.row(|ui| {
        // The tab strip: every editor stacked here, the showing one lit.
        // The lit tab is the editor picker too: clicked, it lists the
        // editors to switch this tab to (U034). The others switch tabs.
        let style = ui.text_style();
        let chevron_w = ui.m.px(16.0);
        let renaming = h.rename.as_ref().map(|r| r.0);
        // Where the tabs will sit, worked out first so a drag along the
        // strip knows which tab was pressed before the tabs draw.
        let field_w = ui.m.px(160.0);
        let widths: Vec<f64> = h.tabs.iter().enumerate().map(|(i, label)| if renaming == Some(i) { field_w } else { ui.measure(label, &style) + ui.m.pad * 2.0 + if i == h.current { chevron_w } else { 0.0 } }).collect();
        let mut rects = Vec::with_capacity(widths.len());
        let mut x = ui.cursor().x;
        for w in &widths {
            rects.push(Rect::from_min_size(Vec2::new(x, ui.cursor().y), Vec2::new(*w, ui.m.widget_h)));
            x += w + ui.m.gap;
        }
        let pointer = ui.state.pointer;
        let press = ui.state.press_pos;
        let drag_from = rects.iter().position(|r| r.contains(press)).filter(|_| (ui.state.down || ui.state.released) && renaming.is_none() && (pointer - press).length() > ui.m.px(DRAG_START));
        for (i, label) in h.tabs.iter().enumerate() {
            let lit = i == h.current;
            let rect = ui.alloc(Vec2::new(widths[i], ui.m.widget_h));
            let id = ui.id("tab").with_index(i);
            // ---- a name being typed in the tab's place ----
            if renaming == Some(i) {
                let field_id = ui.id("rename");
                let (_, buf, focus) = h.rename.as_mut().expect("renaming");
                if std::mem::take(focus) {
                    ui.state.focus = Some(field_id);
                    ui.state.focus_visible = false;
                }
                let fr = ui.text_edit_core_with(field_id, rect, buf, TextOpts { placeholder: &h.defaults[i], ..TextOpts::default() });
                if fr.committed {
                    let name = buf.trim().to_owned();
                    out.actions.push(AreaAction::RenameTab(h.area, i, (name != h.defaults[i]).then_some(name)));
                } else if fr.cancelled || (!fr.focused && ui.state.focus != Some(field_id)) {
                    out.actions.push(AreaAction::RenameCancel);
                }
                continue;
            }
            let mut r = ui.interact(id, rect, Sense::CLICK);
            ui.focusable(id, rect);
            ui.key_click(id, &mut r);
            if drag_from.is_some() {
                // A drag along the strip is not a click on the tab it ends over.
                r.clicked = false;
            }
            if r.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            // ---- the tab's own menu on a right click ----
            let menu_id = id.with("menu");
            if ui.state.right_pressed && rect.contains(pointer) {
                *ui.state.open(menu_id) = true;
                *ui.state.floats(menu_id, [0.0; 4]) = [pointer.x, pointer.y, 0.0, 0.0];
                ui.state.request_rebuild = true;
            }
            if *ui.state.open(menu_id) {
                let at = *ui.state.floats(menu_id, [0.0; 4]);
                let anchor = Rect::from_min_size(Vec2::new(at[0], at[1]), Vec2::ZERO);
                let items: Vec<&str> = if h.tabs.len() > 1 { vec!["Rename…", "Close Tab"] } else { vec!["Rename…"] };
                let res = ui.popup_list(menu_id, anchor, &items, None);
                match res.picked {
                    Some(0) => out.actions.push(AreaAction::StartRename(h.area, i)),
                    Some(_) => out.actions.push(AreaAction::CloseTabAt(h.area, i)),
                    None => {}
                }
                if res.picked.is_some() || res.closed {
                    *ui.state.open(menu_id) = false;
                }
            }
            if lit {
                let open = *ui.state.open(id);
                ui.raised(rect, ui.theme.shaded(ui.theme.accent), open);
                let text_rect = Rect::new(rect.min, Vec2::new(rect.max.x - chevron_w, rect.max.y));
                ui.text_centered(label, &style, text_rect, ui.theme.accent_text);
                chevron(ui, Vec2::new(rect.max.x - chevron_w * 0.5 - ui.m.px(2.0), rect.center().y), ui.theme.accent_text);
                ui.focus_ring(id, rect);
                if r.clicked {
                    *ui.state.open(id) = !open;
                    ui.state.request_rebuild = true;
                }
                if *ui.state.open(id) {
                    let idx = h.editors.iter().position(|&k| k == h.kind);
                    let res = ui.popup_list(id, rect, h.labels, idx);
                    if let Some(j) = res.picked {
                        out.actions.push(AreaAction::SetEditor(h.area, h.editors[j]));
                    }
                    if res.picked.is_some() || res.closed {
                        *ui.state.open(id) = false;
                    }
                }
            } else {
                ui.button_face(rect, &r);
                ui.text_centered(label, &style, rect, ui.theme.text);
                ui.focus_ring(id, rect);
                if r.clicked {
                    out.actions.push(AreaAction::SelectTab(h.area, i));
                }
            }
        }
        // ---- a tab dragged along the strip: a ghost follows the pointer,
        // a bar shows where it lands ----
        if let Some(from) = drag_from {
            let to = rects.iter().filter(|r| r.center().x < pointer.x).count();
            let to = if to > from { to - 1 } else { to };
            if ui.state.released {
                if to != from {
                    out.actions.push(AreaAction::MoveTab(h.area, from, to));
                }
                ui.state.request_rebuild = true;
            } else {
                let saved = ui.draw.layer();
                ui.draw.set_layer(saved + 2);
                if to != from {
                    let slot_x = if to >= from { rects[to].max.x + ui.m.gap * 0.5 } else { rects[to].min.x - ui.m.gap * 0.5 };
                    ui.draw.vline(slot_x.round(), rects[from].min.y, rects[from].max.y, ui.m.px(3.0), ui.theme.accent);
                }
                let label = &h.tabs[from];
                let w = ui.measure(label, &style) + ui.m.pad * 2.0;
                let ghost = Rect::from_min_size(Vec2::new(pointer.x - w * 0.5, rects[from].min.y), Vec2::new(w, ui.m.widget_h));
                ui.floating_panel(ghost, ui.theme.header);
                ui.text_centered(label, &style, ghost, ui.theme.text);
                ui.draw.set_layer(saved);
                ui.state.cursor_icon = CursorIcon::Grabbing;
                ui.state.request_rebuild = true;
            }
        }
        if let Some(i) = ui.menu_button("+", h.labels) {
            out.actions.push(AreaAction::AddTab(h.area, h.editors[i]));
        }
        host.draw_header(h.kind, ui, cx);
        let menu_w = ui.measure("⋮", &style) + ui.m.pad * 2.0;
        let spacer = (ui.avail_width() - menu_w - ui.m.gap).max(0.0);
        // The empty stretch of the header is a grip: drag it onto another
        // area to swap the two or dock beside it, or to a window edge to
        // span it (see [`crate::screen_dock`]).
        let grip = ui.alloc(Vec2::new(spacer, ui.m.widget_h));
        if ui.interact(ui.id("grip"), grip, Sense::DRAG).pressed {
            out.grip_pressed = true;
        }
        let mut rows = vec![if h.maximized { "Restore" } else { "Maximize" }, "Split Left | Right", "Split Top | Bottom"];
        if !h.alone {
            rows.extend(["Dock Left Edge", "Dock Right Edge", "Dock Top Edge", "Dock Bottom Edge"]);
        }
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
                (_, "Dock Left Edge") => AreaAction::Dock(h.area, Side::Left),
                (_, "Dock Right Edge") => AreaAction::Dock(h.area, Side::Right),
                (_, "Dock Top Edge") => AreaAction::Dock(h.area, Side::Top),
                (_, "Dock Bottom Edge") => AreaAction::Dock(h.area, Side::Bottom),
                (_, "Open in New Window") => AreaAction::Detach(h.area),
                (_, "Close Tab") => AreaAction::CloseTab(h.area),
                _ => AreaAction::Close(h.area),
            });
        }
    });
    out
}

/// A small "⌄" centred at `c`: the lit tab opens a list.
fn chevron(ui: &mut Ui, c: Vec2, color: Color) {
    let s = ui.m.px(5.0);
    let w = ui.m.px(2.0);
    ui.draw.line(Vec2::new(c.x - s, c.y - s * 0.5), Vec2::new(c.x, c.y + s * 0.5), w, color);
    ui.draw.line(Vec2::new(c.x, c.y + s * 0.5), Vec2::new(c.x + s, c.y - s * 0.5), w, color);
}
