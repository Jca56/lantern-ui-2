//! The right-click context menu (D023): a titled panel with tabs, rows,
//! submenus and live property panels, a tool strip floating down its left
//! edge, and an optional bar of buttons above it. The host builds one from
//! whatever was under the pointer and hands it over with
//! [`crate::ShellRequest::ContextMenu`]; the shell draws it and routes
//! its actions back through [`crate::Host::run`].

use lntrn_math::{Color, Rect, Vec2};
use lntrn_props::Reflect;

use crate::event::Key;
use crate::host::{Action, Host, HostCx};
use crate::icons::Icon;
use crate::popups::{PopupResult, dispatch};
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Debug)]
pub enum Item {
    Header(String),
    Separator,
    Action { label: String, action: Action },
    Sub { label: String, items: Vec<Item> },
    /// Editable properties with an Apply button; afterwards edits re-apply
    /// live through [`Host::apply`] with `adjust` set.
    Panel { label: String, action: String, props: Box<dyn Reflect>, applied: bool },
    /// A row the host draws itself ([`Host::draw_item`]), by key.
    Custom(String),
}

impl Item {
    pub fn header(label: &str) -> Item {
        Item::Header(label.to_owned())
    }

    pub fn action(label: &str, action: Action) -> Item {
        Item::Action { label: label.to_owned(), action }
    }

    pub fn sub(label: &str, items: Vec<Item>) -> Item {
        Item::Sub { label: label.to_owned(), items }
    }

    pub fn panel(label: &str, action: &str, props: Box<dyn Reflect>) -> Item {
        Item::Panel { label: label.to_owned(), action: action.to_owned(), props, applied: false }
    }

    pub fn custom(key: &str) -> Item {
        Item::Custom(key.to_owned())
    }
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub label: String,
    pub items: Vec<Item>,
}

/// An icon button on the strip or the bar.
#[derive(Clone, Debug)]
pub struct Tool {
    pub icon: Icon,
    pub active: bool,
    /// Tooltip.
    pub tip: String,
    pub action: Action,
}

impl Tool {
    pub fn new(icon: Icon, tip: &str, action: Action) -> Self {
        Self { icon, active: false, tip: tip.to_owned(), action }
    }

    pub fn active(mut self, on: bool) -> Self {
        self.active = on;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Narrow,
    Wide,
}

#[derive(Clone, Debug)]
pub struct ContextMenu {
    pub title: String,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    /// Buttons down the left edge; an active one is lit in the accent.
    pub tools: Vec<Tool>,
    /// Buttons in a row above the panel's left edge (a mode switch); an
    /// active one is lit in the outline colour.
    pub bar: Vec<Tool>,
    /// Colour of the panel's outline and the bar's lit buttons; the theme's
    /// focus colour when `None`.
    pub outline: Option<Color>,
    pub pos: Vec2,
    pub width: Width,
    /// Submenu open in the current tab, by item index.
    pub open_sub: Option<usize>,
    /// Height measured last frame, for placement.
    pub height: f64,
}

impl ContextMenu {
    pub fn new(title: &str, pos: Vec2) -> Self {
        Self { title: title.to_owned(), tabs: Vec::new(), tab: 0, tools: Vec::new(), bar: Vec::new(), outline: None, pos, width: Width::Narrow, open_sub: None, height: 0.0 }
    }

    pub fn tab(mut self, label: &str, items: Vec<Item>) -> Self {
        self.tabs.push(Tab { label: label.to_owned(), items });
        self
    }

    pub fn tool(mut self, tool: Tool) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools.extend(tools);
        self
    }

    pub fn bar(mut self, tools: Vec<Tool>) -> Self {
        self.bar = tools;
        self
    }

    pub fn wide(mut self) -> Self {
        self.width = Width::Wide;
        self
    }

    pub fn outline(mut self, color: Color) -> Self {
        self.outline = Some(color);
        self
    }

    /// Keep the tab, open submenu and measured height of `old` when the
    /// host rebuilds a menu in place.
    pub fn keep_view_of(mut self, old: &ContextMenu) -> Self {
        self.tab = old.tab.min(self.tabs.len().saturating_sub(1));
        self.open_sub = old.open_sub;
        self.height = old.height;
        self.pos = old.pos;
        self
    }
}

/// The content on layer 2 measured as it lays out, then its panel painted
/// on layer 1 underneath; the bar above it, the tool strip floating down
/// the left, one open submenu to the side.
pub(crate) fn draw<H: Host>(ui: &mut Ui, menu: &mut ContextMenu, window: Rect, host: &mut H, cx: &mut HostCx) -> PopupResult {
    let mut out = PopupResult::default();
    if ui.state.take_key(|k| k.key == Key::Escape).is_some() {
        out.close = true;
    }
    let m = ui.m;
    let outline = menu.outline.unwrap_or(ui.theme.focus);
    let width = match menu.width {
        Width::Narrow => m.px(300.0),
        Width::Wide => m.px(440.0),
    };
    let strip_w = m.widget_h;
    let bar_h = if menu.bar.is_empty() { 0.0 } else { m.widget_h + m.gap };
    let strip_h = menu.tools.len() as f64 * (m.widget_h + m.gap);
    let est_h = if menu.height > 0.0 { menu.height } else { m.widget_h * 8.0 };
    let min_x = window.min.x + if menu.tools.is_empty() { 0.0 } else { strip_w + m.gap * 2.0 };
    let min_y = window.min.y + bar_h;
    let x = menu.pos.x.clamp(min_x, (window.max.x - width).max(min_x));
    let y = menu.pos.y.clamp(min_y, (window.max.y - est_h).max(min_y));
    let panel = Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, est_h));
    // The bar sits in a row above the panel's left edge; the strip runs down
    // the left from the panel's top. The corner between them stays empty.
    let bar = Rect::from_min_size(Vec2::new(panel.min.x, panel.min.y - bar_h), Vec2::new(menu.bar.len() as f64 * (strip_w + m.gap), m.widget_h));
    let strip = Rect::from_min_size(Vec2::new(panel.min.x - m.gap - strip_w, panel.min.y), Vec2::new(strip_w, strip_h.max(1.0)));
    let sub_w = m.px(300.0);
    // The submenu opens beside the panel, on the left when the right is full.
    let sub_x = if panel.max.x + m.gap + sub_w <= window.max.x { panel.max.x + m.gap } else { strip.min.x - m.gap - sub_w };
    let sub_area = Rect::from_min_size(Vec2::new(sub_x, panel.min.y), Vec2::new(sub_w, est_h));
    let mut hit = panel;
    if !menu.tools.is_empty() {
        hit = hit.union(&strip);
    }
    if !menu.bar.is_empty() {
        hit = hit.union(&bar);
    }
    if menu.open_sub.is_some() {
        hit = hit.union(&sub_area);
    }
    ui.state.keep_popup(hit, 1);
    // A press outside closes the menu but is left unclaimed, so the editor
    // underneath still gets it: one click selects, one right-click opens the
    // menu for the new thing.
    let outside_left = ui.state.pressed && !hit.contains(ui.state.press_pos);
    let outside_right = ui.state.right_pressed && !hit.contains(ui.state.pointer);
    if outside_left || outside_right {
        out.close = true;
    }

    let saved_layer = ui.layer();
    let saved_clip = ui.clip();
    ui.draw.set_layer(2);
    ui.set_layer_internal(2);
    ui.set_clip(window);
    ui.draw.push_clip_absolute(window);
    ui.push_id("context");

    // ---- content -------------------------------------------------------------
    let content = panel.shrink(m.pad);
    ui.set_cursor(content.min);
    ui.set_avail_width(content.width());
    let title = menu.title.clone();
    ui.heading(&title);
    if menu.tabs.len() > 1 {
        let labels: Vec<&str> = menu.tabs.iter().map(|t| t.label.as_str()).collect();
        let mut tab = menu.tab;
        if ui.tabs(&mut tab, &labels) {
            menu.tab = tab;
            menu.open_sub = None;
        }
        ui.space(m.gap);
    }
    let tab = menu.tab.min(menu.tabs.len().saturating_sub(1));
    let mut sub_anchor: Option<(usize, f64)> = None;
    let mut new_open_sub = menu.open_sub;
    if let Some(t) = menu.tabs.get_mut(tab) {
        for (i, item) in t.items.iter_mut().enumerate() {
            ui.push_index(i);
            match item {
                Item::Header(h) => {
                    ui.label_dim(h);
                }
                Item::Separator => ui.separator(),
                Item::Action { label, action } => {
                    if ui.selectable(label, false).clicked {
                        dispatch(host, action, cx);
                        out.close = true;
                    }
                }
                Item::Sub { label, .. } => {
                    let rect = ui.alloc(Vec2::new(FILL, m.widget_h));
                    let r = ui.interact(ui.id("sub"), rect, Sense::CLICK);
                    let open = menu.open_sub == Some(i);
                    if r.hovered || open {
                        let bg = if open { ui.theme.hover(ui.theme.header) } else { ui.theme.hover(ui.theme.panel) };
                        ui.fill(rect, bg);
                    }
                    if r.hovered {
                        ui.state.cursor_icon = CursorIcon::Pointer;
                        new_open_sub = Some(i);
                    }
                    if r.clicked {
                        new_open_sub = if open { None } else { Some(i) };
                    }
                    let style = ui.text_style();
                    let inner = Rect::new(Vec2::new(rect.min.x + m.pad, rect.min.y), Vec2::new(rect.max.x - m.pad, rect.max.y));
                    ui.text_in_rect(label, &style, inner, ui.theme.text);
                    ui.text_right("▸", &style, inner, ui.theme.text_dim);
                    if open {
                        sub_anchor = Some((i, rect.min.y));
                    }
                }
                Item::Panel { label, action, props, applied } => {
                    ui.label_dim(label);
                    let changed = ui.props_panel(&mut **props);
                    let mut apply = false;
                    ui.row(|ui| {
                        if ui.button(if *applied { "Apply Again" } else { "Apply" }).clicked {
                            apply = true;
                        }
                    });
                    if apply || (changed && *applied) {
                        if host.apply(action, &**props, *applied && !apply, cx) {
                            *applied = true;
                        }
                        ui.state.request_rebuild = true;
                    }
                    ui.separator();
                }
                Item::Custom(key) => {
                    if host.draw_item(key, ui, cx) {
                        ui.state.request_rebuild = true;
                    }
                }
            }
            ui.pop_id();
        }
    }
    let bottom = ui.cursor().y + m.pad;
    let measured = (bottom - panel.min.y).max(m.widget_h * 2.0);
    if (measured - menu.height).abs() > 0.5 {
        menu.height = measured;
        ui.state.request_rebuild = true;
    }
    let panel_rect = Rect::from_min_size(panel.min, Vec2::new(width, measured));
    menu.open_sub = new_open_sub;

    // ---- submenu ---------------------------------------------------------------
    if let (Some(si), Some((_, anchor_y))) = (menu.open_sub, sub_anchor)
        && let Some(Item::Sub { items, .. }) = menu.tabs.get(tab).and_then(|t| t.items.get(si)).cloned()
    {
        let sub_y = anchor_y.min(window.max.y - m.widget_h * (items.len() as f64 + 0.5) - m.pad * 2.0).max(window.min.y);
        let sub_rect = Rect::from_min_size(Vec2::new(sub_x, sub_y), Vec2::new(sub_w, m.widget_h * items.len() as f64 + m.gap * (items.len() as f64 - 1.0).max(0.0) + m.pad * 2.0));
        ui.set_cursor(sub_rect.min + Vec2::splat(m.pad));
        ui.set_avail_width(sub_rect.width() - m.pad * 2.0);
        ui.push_id("sub");
        for (j, item) in items.iter().enumerate() {
            ui.push_index(j);
            if let Item::Action { label, action } = item
                && ui.selectable(label, false).clicked
            {
                dispatch(host, action, cx);
                out.close = true;
            }
            ui.pop_id();
        }
        ui.pop_id();
        ui.draw.set_layer(1);
        ui.floating_panel(sub_rect, ui.theme.header);
        ui.draw.set_layer(2);
    }

    // ---- bar: a row of icon buttons above the panel, lit in the outline colour --
    for (i, t) in menu.bar.iter().enumerate() {
        let rect = Rect::from_min_size(Vec2::new(bar.min.x + i as f64 * (strip_w + m.gap), bar.min.y), Vec2::splat(strip_w));
        let lit = t.active.then_some((outline, ui.theme.selection_text));
        let r = ui.icon_button_in(ui.id("bar").with_index(i), rect, t.icon, lit, &t.tip);
        if r.clicked && !t.active {
            dispatch(host, &t.action, cx);
            out.refresh = true;
            ui.state.request_rebuild = true;
        }
    }

    // ---- tool strip -------------------------------------------------------------
    let mut ty = strip.min.y;
    for (i, t) in menu.tools.iter().enumerate() {
        let rect = Rect::from_min_size(Vec2::new(strip.min.x, ty), Vec2::splat(strip_w));
        let lit = t.active.then_some((ui.theme.accent, ui.theme.accent_text));
        let r = ui.icon_button_in(ui.id("tool").with_index(i), rect, t.icon, lit, &t.tip);
        if r.clicked {
            dispatch(host, &t.action, cx);
            out.refresh = true;
            ui.state.request_rebuild = true;
        }
        ty += strip_w + m.gap;
    }

    // ---- panel underneath, outlined ----------------------------------------------
    ui.draw.set_layer(1);
    ui.floating_panel(panel_rect, ui.theme.header);
    ui.draw.stroke_rect(panel_rect, m.px(2.0), m.radius, outline);
    ui.draw.set_layer(saved_layer);

    ui.pop_id();
    ui.draw.pop_clip();
    ui.set_clip(saved_clip);
    ui.set_layer_internal(saved_layer);
    if out.close {
        ui.state.focus = None;
        ui.state.request_rebuild = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lntrn_props::Value;

    #[test]
    fn builder_and_keep_view() {
        let menu = ContextMenu::new("Cube", Vec2::new(10.0, 20.0))
            .tab("Object", vec![Item::action("Delete", Action::new("object.delete")), Item::Separator, Item::sub("Add", vec![Item::action("Cube", Action::new("add").with("kind", Value::Enum(1)))])])
            .tab("Transform", vec![Item::custom("transform")])
            .tool(Tool::new(Icon::Move, "Move", Action::new("gizmo").with("mode", Value::Enum(0))).active(true))
            .bar(vec![Tool::new(Icon::Object, "Object mode", Action::new("mode").with("edit", Value::Bool(false)))])
            .wide();
        assert_eq!(menu.tabs.len(), 2);
        assert_eq!(menu.width, Width::Wide);
        assert!(menu.tools[0].active);
        assert_eq!(menu.bar.len(), 1);
        assert!(matches!(&menu.tabs[0].items[2], Item::Sub { items, .. } if items.len() == 1));

        let mut old = menu.clone();
        old.tab = 1;
        old.open_sub = Some(2);
        old.height = 300.0;
        let fresh = ContextMenu::new("Cube", Vec2::ZERO).tab("Only", vec![]).keep_view_of(&old);
        assert_eq!(fresh.tab, 0, "clamped to the tabs that exist");
        assert_eq!(fresh.open_sub, Some(2));
        assert_eq!(fresh.height, 300.0);
        assert_eq!(fresh.pos, Vec2::new(10.0, 20.0));
    }
}
