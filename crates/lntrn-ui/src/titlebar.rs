//! Client-side window frame: a title bar with the host's menus, drag,
//! double-click to maximize, minimize / maximize / close, and resize grabs
//! along the edges. The shell draws it; the app carries out the resulting
//! [`WindowCommand`].

use lntrn_math::{Rect, Vec2};
use lntrn_render::DrawList;
use lntrn_text::{TextEngine, TextStyle};

use crate::id::WidgetId;
use crate::host::Host;
use crate::shell::{Shell, WindowState};
use crate::state::CursorIcon;
use crate::theme::{Metrics, Theme};
use crate::ui::{Sense, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    /// Start an interactive move (the pointer button is down on the title).
    Drag,
    Minimize,
    ToggleMaximize,
    Close,
    /// Start an interactive resize from this edge or corner.
    Resize(ResizeEdge),
}

/// What the title bar reported after drawing.
pub(crate) struct TitleBar {
    /// The rect left for the areas.
    pub rest: Rect,
    pub command: Option<WindowCommand>,
    /// A menu label was clicked: the menu's name and where its popup goes.
    pub open_menu: Option<(String, Vec2)>,
    /// The pointer is on a menu label (the same pair), so an open menu can
    /// switch to it.
    pub hovered_menu: Option<(String, Vec2)>,
}

/// Grab zone along the window edges, in logical pixels.
const EDGE: f64 = 5.0;
/// Corner zone: this far from both edges.
const CORNER: f64 = 15.0;

fn edge_at(p: Vec2, window: Rect, edge: f64, corner: f64) -> Option<ResizeEdge> {
    let n = p.y < window.min.y + edge;
    let s = p.y >= window.max.y - edge;
    let w = p.x < window.min.x + edge;
    let e = p.x >= window.max.x - edge;
    let near_n = p.y < window.min.y + corner;
    let near_s = p.y >= window.max.y - corner;
    let near_w = p.x < window.min.x + corner;
    let near_e = p.x >= window.max.x - corner;
    match () {
        _ if (n && near_w) || (w && near_n) => Some(ResizeEdge::NorthWest),
        _ if (n && near_e) || (e && near_n) => Some(ResizeEdge::NorthEast),
        _ if (s && near_w) || (w && near_s) => Some(ResizeEdge::SouthWest),
        _ if (s && near_e) || (e && near_s) => Some(ResizeEdge::SouthEast),
        _ if n => Some(ResizeEdge::North),
        _ if s => Some(ResizeEdge::South),
        _ if w => Some(ResizeEdge::West),
        _ if e => Some(ResizeEdge::East),
        _ => None,
    }
}

fn cursor_for(edge: ResizeEdge) -> CursorIcon {
    match edge {
        ResizeEdge::North | ResizeEdge::South => CursorIcon::NsResize,
        ResizeEdge::East | ResizeEdge::West => CursorIcon::EwResize,
        ResizeEdge::NorthEast | ResizeEdge::SouthWest => CursorIcon::NeswResize,
        ResizeEdge::NorthWest | ResizeEdge::SouthEast => CursorIcon::NwseResize,
    }
}

impl<H: Host> Shell<H> {
    /// Hover cursor and press handling for the resize zones. Nothing when
    /// maximized (the compositor owns the edges then).
    pub(crate) fn resize_edges(&mut self, window: Rect, m: Metrics, ws: WindowState) -> Option<WindowCommand> {
        if ws.maximized || !self.state.pointer_in_window || self.state.active.is_some() {
            return None;
        }
        let (edge, corner) = (m.px(EDGE), m.px(CORNER));
        let hovered = edge_at(self.state.pointer, window, edge, corner);
        if let Some(e) = hovered {
            self.state.cursor_icon = cursor_for(e);
        }
        if self.state.pressed && !self.state.press_claimed
            && let Some(e) = edge_at(self.state.press_pos, window, edge, corner)
        {
            self.state.press_claimed = true;
            return Some(WindowCommand::Resize(e));
        }
        None
    }

    /// Draw the title bar across the top of `window`. `menus` are the host's
    /// (label, menu name) pairs on the left.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn title_bar(
        &mut self,
        draw: &mut DrawList,
        text: &mut TextEngine,
        theme: &Theme,
        m: Metrics,
        window: Rect,
        ws: WindowState,
        title: &str,
        status: &str,
        menus: &[(&str, &str)],
    ) -> TitleBar {
        let (bar, rest) = window.take_top(m.header_h);
        let mut cmd = None;
        let mut open_menu = None;
        let mut hovered_menu = None;
        // An unfocused window's bar sinks halfway into the background.
        let g = if ws.focused { theme.title } else { theme.title.map(|c| c.lerp(theme.bg, 0.5)) };
        let bg = g.mid();
        draw.set_layer(0);
        draw.push_clip_absolute(bar);
        draw.rect_gradient(bar, g.top, g.bottom);
        draw.hline(bar.min.x, bar.max.x, bar.min.y, m.border, theme.highlight(bg));
        draw.hline(bar.min.x, bar.max.x, bar.max.y - m.border, m.border, theme.border_dark);
        draw.pop_clip();

        let mut ui = Ui::new(draw, text, theme, m, &mut self.state, bar, bar, WidgetId::ROOT.with("titlebar"), 0);
        ui.set_window_rect(window);

        // Window buttons, right to left: close, maximize, minimize.
        let side = m.header_h;
        let mut x = bar.max.x;
        let buttons = [("close", 2usize), ("maximize", 1), ("minimize", 0)];
        for (name, kind) in buttons {
            x -= side;
            let r = Rect::from_min_size(Vec2::new(x, bar.min.y), Vec2::new(side, side));
            let resp = ui.interact(ui.id(name), r, Sense::CLICK);
            if resp.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
                let hover = if kind == 2 { theme.close } else { theme.hover(bg) };
                let shown = if resp.held { theme.shade(hover) } else { hover };
                ui.draw.rect_gradient(r, theme.top(shown), theme.bottom(shown));
            }
            let c = r.center();
            let s = m.px(6.0);
            let w = m.px(2.0);
            let ink = theme.text;
            match kind {
                0 => ui.draw.hline(c.x - s, c.x + s, (c.y - w * 0.5).round(), w, ink),
                1 => {
                    if ws.maximized {
                        let a = Rect::from_center_size(c + Vec2::new(-w, w), Vec2::splat(s * 1.6));
                        let b = Rect::from_center_size(c + Vec2::new(w, -w), Vec2::splat(s * 1.6));
                        ui.draw.stroke_rect(b, w, 0.0, ink);
                        ui.draw.rect(a, if resp.hovered { theme.hover(bg) } else { bg });
                        ui.draw.stroke_rect(a, w, 0.0, ink);
                    } else {
                        ui.draw.stroke_rect(Rect::from_center_size(c, Vec2::splat(s * 2.0)), w, 0.0, ink);
                    }
                }
                _ => {
                    ui.draw.line(c + Vec2::new(-s, -s), c + Vec2::new(s, s), w, ink);
                    ui.draw.line(c + Vec2::new(-s, s), c + Vec2::new(s, -s), w, ink);
                }
            }
            if resp.clicked {
                cmd = Some(match kind {
                    0 => WindowCommand::Minimize,
                    1 => WindowCommand::ToggleMaximize,
                    _ => WindowCommand::Close,
                });
            }
        }

        // Menus on the left: a click opens the popup just below the label.
        let style = ui.text_style();
        let mut menu_x = bar.min.x + m.pad;
        for &(label, name) in menus {
            let w = ui.measure(label, &style) + m.pad * 2.0;
            let r = Rect::from_min_size(Vec2::new(menu_x, bar.min.y), Vec2::new(w, side));
            let resp = ui.interact(ui.id(name), r, Sense::CLICK);
            if resp.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
                let shown = if resp.held { theme.shade(theme.hover(bg)) } else { theme.hover(bg) };
                ui.draw.rect_gradient(r, theme.top(shown), theme.bottom(shown));
                hovered_menu = Some((name.to_owned(), Vec2::new(r.min.x, r.max.y)));
            }
            let inner = Rect::new(Vec2::new(r.min.x + m.pad, r.min.y), Vec2::new(r.max.x - m.pad, r.max.y));
            ui.text_in_rect(label, &style, inner, if ws.focused { theme.text } else { theme.text_dim });
            if resp.clicked {
                open_menu = Some((name.to_owned(), Vec2::new(r.min.x, r.max.y)));
            }
            menu_x += w;
        }

        // Everything between the menus and the buttons drags the window.
        let drag_rect = Rect::new(Vec2::new(menu_x, bar.min.y), Vec2::new(x, bar.max.y));
        let resp = ui.interact(ui.id("drag"), drag_rect, Sense::DRAG);
        if resp.pressed {
            cmd = Some(if resp.double_clicked { WindowCommand::ToggleMaximize } else { WindowCommand::Drag });
            // The window system takes over; forget the press so nothing else reacts.
            ui.state.active = None;
        }

        let title_style = ui.text_style().bold();
        let title_rect = Rect::new(Vec2::new(menu_x + m.pad * 2.0, bar.min.y), Vec2::new(x - m.pad, bar.max.y));
        ui.text_in_rect(title, &title_style, title_rect, if ws.focused { theme.text } else { theme.text_dim });
        if !status.is_empty() {
            let title_w = ui.measure(title, &title_style);
            let status_rect = Rect::new(Vec2::new(title_rect.min.x + title_w + m.pad * 3.0, bar.min.y), title_rect.max);
            if status_rect.width() > m.px(100.0) {
                let style = ui.text_style();
                ui.text_right(status, &style, status_rect, theme.text_dim);
            }
        }
        ui.finish();

        TitleBar { rest, command: cmd, open_menu, hovered_menu }
    }
}

impl<H: Host> Shell<H> {
    /// The status bar's height: shorter than a widget row.
    pub(crate) fn status_bar_h(m: Metrics) -> f64 {
        (m.widget_h * 0.7).round()
    }

    /// A row along the bottom of the window with the host's status in
    /// it, dim, left-aligned, in the title bar's colors and a smaller
    /// text (U036).
    pub(crate) fn status_bar(&mut self, draw: &mut DrawList, text: &mut TextEngine, theme: &Theme, m: Metrics, bar: Rect, status: &str) {
        let g = theme.title;
        draw.set_layer(0);
        draw.push_clip_absolute(bar);
        draw.rect_gradient(bar, g.top, g.bottom);
        draw.hline(bar.min.x, bar.max.x, bar.min.y, m.border, theme.border_dark);
        draw.hline(bar.min.x, bar.max.x, bar.min.y + m.border, m.border, theme.highlight(g.mid()));
        draw.pop_clip();
        if status.is_empty() {
            return;
        }
        let mut ui = Ui::new(draw, text, theme, m, &mut self.state, bar, bar, WidgetId::ROOT.with("status"), 0);
        let style = TextStyle::new((m.text_size * 0.85).round().max(10.0));
        let rect = Rect::new(Vec2::new(bar.min.x + m.pad * 2.0, bar.min.y), Vec2::new(bar.max.x - m.pad * 2.0, bar.max.y));
        ui.text_in_rect(status, &style, rect, theme.text_dim);
        ui.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_corners() {
        let w = Rect::from_xywh(0.0, 0.0, 1000.0, 500.0);
        assert_eq!(edge_at(Vec2::new(2.0, 250.0), w, 5.0, 15.0), Some(ResizeEdge::West));
        assert_eq!(edge_at(Vec2::new(998.0, 250.0), w, 5.0, 15.0), Some(ResizeEdge::East));
        assert_eq!(edge_at(Vec2::new(500.0, 2.0), w, 5.0, 15.0), Some(ResizeEdge::North));
        assert_eq!(edge_at(Vec2::new(500.0, 497.0), w, 5.0, 15.0), Some(ResizeEdge::South));
        assert_eq!(edge_at(Vec2::new(2.0, 10.0), w, 5.0, 15.0), Some(ResizeEdge::NorthWest));
        assert_eq!(edge_at(Vec2::new(990.0, 498.0), w, 5.0, 15.0), Some(ResizeEdge::SouthEast));
        assert_eq!(edge_at(Vec2::new(500.0, 250.0), w, 5.0, 15.0), None);
        assert_eq!(cursor_for(ResizeEdge::NorthEast), CursorIcon::NeswResize);
    }
}
