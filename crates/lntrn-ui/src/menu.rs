//! Named menus (D035): the popup a title-bar label, a key binding or a
//! [`crate::ShellRequest::Menu`] opens. Rows run actions; a row may show a
//! check mark, be greyed out, carry its key binding at the right edge, or
//! open one submenu beside the panel. Arrows walk the rows, Enter chooses,
//! Right opens a submenu and Left closes it. Escape or a press outside
//! closes the menu, and the press falls through to what is underneath.

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::host::{Action, Host, HostCx, MenuItem};
use crate::popups::{PopupResult, dispatch};
use crate::state::CursorIcon;
use crate::theme::Metrics;
use crate::ui::{Sense, Ui};

/// An open named menu.
#[derive(Clone, Debug)]
pub struct MenuState {
    /// The host's name for it (`file`), so hovering another title-bar
    /// label knows whether to switch.
    pub name: String,
    pub title: String,
    pub items: Vec<MenuItem>,
    /// Top-left corner asked for; the panel stays inside the window.
    pub pos: Vec2,
    /// Row whose submenu is open.
    pub open_sub: Option<usize>,
    /// Row the keyboard is on in the panel, and in the open submenu.
    pub selected: Option<usize>,
    pub sub_selected: Option<usize>,
}

impl MenuState {
    pub fn new(name: &str, title: &str, items: Vec<MenuItem>, pos: Vec2) -> Self {
        Self { name: name.to_owned(), title: title.to_owned(), items, pos, open_sub: None, selected: None, sub_selected: None }
    }
}

/// Width a column of rows needs: check column, label, hint or arrow, padding.
fn panel_width(ui: &mut Ui, items: &[MenuItem]) -> f64 {
    let style = ui.text_style();
    let m = ui.m;
    let mut w = m.px(260.0);
    for it in items.iter().filter(|i| !i.separator) {
        let right = if it.sub.is_empty() { it.hint.as_deref().map_or(0.0, |h| ui.measure(h, &style)) } else { ui.measure("▸", &style) };
        let need = m.gap * 2.0 + check_w(m) + ui.measure(&it.label, &style) + m.pad * 3.0 + right;
        w = w.max(need);
    }
    w.round()
}

/// The column a check mark sits in, left of every label.
fn check_w(m: Metrics) -> f64 {
    (m.widget_h * 0.6).round()
}

fn separator_h(m: Metrics) -> f64 {
    m.gap * 2.0 + m.border * 2.0
}

fn rows_height(m: Metrics, items: &[MenuItem]) -> f64 {
    items.iter().map(|i| if i.separator { separator_h(m) } else { m.widget_h }).sum()
}

/// The next row the keyboard can land on from `from`, stepping `step`
/// (wrapping); the first or last such row when `from` is `None`.
fn next_row(items: &[MenuItem], from: Option<usize>, step: isize) -> Option<usize> {
    let n = items.len() as isize;
    if n == 0 {
        return None;
    }
    let mut i = match from {
        Some(f) => f as isize,
        None if step > 0 => -1,
        None => n,
    };
    for _ in 0..n {
        i = (i + step).rem_euclid(n);
        let it = &items[i as usize];
        if !it.separator && it.enabled {
            return Some(i as usize);
        }
    }
    None
}

/// What a column of rows reported.
#[derive(Default)]
struct Column {
    /// Enabled row under the pointer.
    hovered: Option<usize>,
    /// A submenu row was clicked.
    clicked_sub: Option<usize>,
    /// Top of the row whose submenu is open, to anchor it.
    sub_anchor_y: Option<f64>,
}

/// Lay `items` out from `top` inside `panel`, one under the other. Rows
/// light up under the pointer, on `keyboard`, and while their submenu
/// (`open_sub`) is open. An enabled action row that is clicked runs.
#[allow(clippy::too_many_arguments)]
fn column<H: Host>(ui: &mut Ui, key: &str, items: &[MenuItem], panel: Rect, top: f64, keyboard: Option<usize>, open_sub: Option<usize>, host: &mut H, cx: &mut HostCx, out: &mut PopupResult) -> Column {
    let m = ui.m;
    let style = ui.text_style();
    let mut col = Column::default();
    let mut y = top;
    for (i, it) in items.iter().enumerate() {
        if it.separator {
            let ly = (y + separator_h(m) * 0.5 - m.border).round();
            ui.etched_line(panel.min.x + m.pad, panel.max.x - m.pad, ly);
            y += separator_h(m);
            continue;
        }
        let rect = Rect::new(Vec2::new(panel.min.x + m.gap, y), Vec2::new(panel.max.x - m.gap, y + m.widget_h));
        let r = ui.interact(ui.id(key).with_index(i), rect, if it.enabled { Sense::CLICK } else { Sense::NONE });
        let lit = (it.enabled && r.hovered) || keyboard == Some(i) || open_sub == Some(i);
        if lit {
            ui.fill(rect, ui.theme.hover(ui.theme.header));
        }
        if r.hovered && it.enabled {
            ui.state.cursor_icon = CursorIcon::Pointer;
            col.hovered = Some(i);
        }
        let ink = if it.enabled { ui.theme.text } else { ui.theme.text_dim };
        let check = Rect::new(Vec2::new(rect.min.x + m.pad, rect.min.y), Vec2::new(rect.min.x + m.pad + check_w(m), rect.max.y));
        if it.checked == Some(true) {
            ui.text_centered("✓", &style, check, ui.theme.accent);
        }
        let inner = Rect::new(Vec2::new(check.max.x, rect.min.y), Vec2::new(rect.max.x - m.pad, rect.max.y));
        ui.text_in_rect(&it.label, &style, inner, ink);
        if !it.sub.is_empty() {
            ui.text_right("▸", &style, inner, ui.theme.text_dim);
        } else if let Some(h) = &it.hint {
            ui.text_right(h, &style, inner, ui.theme.text_dim);
        }
        if open_sub == Some(i) {
            col.sub_anchor_y = Some(y);
        }
        if r.clicked && it.enabled {
            if it.sub.is_empty() {
                dispatch(host, &it.action, cx);
                out.close = true;
            } else {
                col.clicked_sub = Some(i);
            }
        }
        y += m.widget_h;
    }
    col
}

/// Keys this frame: arrows, Home and End move; Right opens the selected
/// row's submenu; Left closes it; Enter chooses.
fn keyboard<H: Host>(ui: &mut Ui, menu: &mut MenuState, host: &mut H, cx: &mut HostCx, out: &mut PopupResult) {
    while let Some(k) = ui.state.take_key(|k| matches!(k.key, Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight | Key::Enter | Key::Home | Key::End)) {
        ui.state.request_rebuild = true;
        let sub = menu.open_sub;
        let items: &[MenuItem] = match sub {
            Some(s) => &menu.items[s].sub,
            None => &menu.items,
        };
        let cur = if sub.is_some() { menu.sub_selected } else { menu.selected };
        let mut next = cur;
        let mut run: Option<Action> = None;
        let mut open: Option<usize> = None;
        let mut close_sub = false;
        match k.key {
            Key::ArrowDown => next = next_row(items, cur, 1),
            Key::ArrowUp => next = next_row(items, cur, -1),
            Key::Home => next = next_row(items, None, 1),
            Key::End => next = next_row(items, None, -1),
            Key::ArrowRight if sub.is_none() => open = cur.filter(|&i| !items[i].sub.is_empty()),
            Key::ArrowLeft if sub.is_some() => close_sub = true,
            Key::Enter => {
                if let Some(i) = cur {
                    if !items[i].sub.is_empty() && sub.is_none() {
                        open = Some(i);
                    } else if items[i].enabled {
                        run = Some(items[i].action.clone());
                    }
                }
            }
            _ => {}
        }
        if sub.is_some() {
            menu.sub_selected = next;
        } else {
            menu.selected = next;
        }
        if let Some(i) = open {
            menu.open_sub = Some(i);
            menu.sub_selected = next_row(&menu.items[i].sub, None, 1);
        }
        if close_sub {
            menu.open_sub = None;
            menu.sub_selected = None;
        }
        if let Some(a) = run {
            dispatch(host, &a, cx);
            out.close = true;
        }
    }
}

/// Draw the menu on layer 1 and act on it.
pub(crate) fn draw<H: Host>(ui: &mut Ui, menu: &mut MenuState, window: Rect, host: &mut H, cx: &mut HostCx) -> PopupResult {
    let mut out = PopupResult::default();
    if ui.state.take_key(|k| k.key == Key::Escape).is_some() {
        out.close = true;
    }
    keyboard(ui, menu, host, cx, &mut out);
    let m = ui.m;

    // ---- geometry ---------------------------------------------------------
    let w = panel_width(ui, &menu.items);
    let h = m.gap * 2.0 + m.widget_h + rows_height(m, &menu.items) + m.gap;
    let x = menu.pos.x.min(window.max.x - w).max(window.min.x);
    let y = menu.pos.y.min(window.max.y - h).max(window.min.y);
    let panel = Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h));
    let sub_items: Vec<MenuItem> = menu.open_sub.and_then(|i| menu.items.get(i)).map(|it| it.sub.clone()).unwrap_or_default();
    let mut hit = panel;
    let sub_w = if sub_items.is_empty() { 0.0 } else { panel_width(ui, &sub_items) };
    let sub_h = m.gap * 2.0 + rows_height(m, &sub_items);
    // Beside the panel on the right, or on the left when the right is full.
    let sub_x = if panel.max.x + m.gap + sub_w <= window.max.x { panel.max.x + m.gap } else { (panel.min.x - m.gap - sub_w).max(window.min.x) };

    let saved_layer = ui.layer();
    let saved_clip = ui.clip();
    ui.draw.set_layer(1);
    ui.set_layer_internal(1);
    ui.set_clip(window);
    ui.draw.push_clip_absolute(window);
    ui.push_id("menu");

    // ---- the panel and its rows ---------------------------------------------
    ui.floating_panel(panel, ui.theme.header);
    let style = ui.text_style();
    let title_rect = Rect::new(Vec2::new(panel.min.x + m.gap + m.pad, panel.min.y + m.gap), Vec2::new(panel.max.x - m.pad, panel.min.y + m.gap + m.widget_h));
    ui.text_in_rect(&menu.title, &style, title_rect, ui.theme.text_dim);
    let rows_top = title_rect.max.y;
    let col = column(ui, "item", &menu.items, panel, rows_top, menu.selected, menu.open_sub, host, cx, &mut out);

    // Hovering a row moves the keyboard there and opens (or closes) submenus.
    if let Some(i) = col.hovered {
        menu.selected = Some(i);
        let want = (!menu.items[i].sub.is_empty()).then_some(i);
        if menu.open_sub != want {
            menu.open_sub = want;
            menu.sub_selected = None;
            ui.state.request_rebuild = true;
        }
    }
    if let Some(i) = col.clicked_sub {
        menu.open_sub = if menu.open_sub == Some(i) { None } else { Some(i) };
        menu.sub_selected = None;
        ui.state.request_rebuild = true;
    }

    // ---- the submenu beside it -------------------------------------------------
    if let (Some(anchor_y), false) = (col.sub_anchor_y, sub_items.is_empty()) {
        let sub_y = (anchor_y - m.gap).min(window.max.y - sub_h).max(window.min.y);
        let sub = Rect::from_min_size(Vec2::new(sub_x, sub_y), Vec2::new(sub_w, sub_h));
        hit = hit.union(&sub);
        ui.floating_panel(sub, ui.theme.header);
        let scol = column(ui, "sub", &sub_items, sub, sub.min.y + m.gap, menu.sub_selected, None, host, cx, &mut out);
        if let Some(j) = scol.hovered {
            menu.sub_selected = Some(j);
        }
    }

    ui.state.keep_popup(hit, 1);
    // A press outside closes the menu and is left unclaimed: one click
    // dismisses and lands on whatever was under it.
    if ui.state.pressed && !hit.contains(ui.state.press_pos) {
        out.close = true;
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

    #[test]
    fn keyboard_rows_skip_separators_and_disabled() {
        let items = vec![MenuItem::separator(), MenuItem::new("Open", Action::new("open")), MenuItem::separator(), MenuItem::new("Save", Action::new("save")).disabled(), MenuItem::new("Quit", Action::new("quit"))];
        assert_eq!(next_row(&items, None, 1), Some(1), "first choosable");
        assert_eq!(next_row(&items, None, -1), Some(4), "last choosable");
        assert_eq!(next_row(&items, Some(1), 1), Some(4), "over the rule and the greyed row");
        assert_eq!(next_row(&items, Some(4), 1), Some(1), "wraps");
        assert_eq!(next_row(&items, Some(1), -1), Some(4), "wraps backwards");
        assert_eq!(next_row(&[], None, 1), None);
        assert_eq!(next_row(&[MenuItem::separator()], None, 1), None, "nothing to land on");
    }

    #[test]
    fn menu_items_build() {
        let it = MenuItem::new("Open…", Action::new("open")).hint("Ctrl+O").checked(true);
        assert!(it.enabled && it.sub.is_empty() && !it.separator);
        assert_eq!(it.hint.as_deref(), Some("Ctrl+O"));
        assert!(MenuItem::separator().separator);
        let sub = MenuItem::sub("Recent", vec![it.clone()]);
        assert_eq!(sub.sub.len(), 1);
        assert!(!MenuItem::new("x", Action::new("x")).disabled().enabled);
        let state = MenuState::new("file", "File", vec![sub], Vec2::ZERO);
        assert_eq!(state.name, "file");
        assert_eq!(state.open_sub, None);
    }
}
