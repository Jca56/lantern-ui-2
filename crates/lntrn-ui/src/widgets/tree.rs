//! Tree rows: a disclosure triangle, a label, an indented body of children.
//! Selection is the caller's: pass `selected` and act on `clicked`.

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

/// What a tree row reported this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TreeResponse {
    /// The row was clicked (or Enter/Space pressed on it): select it.
    pub clicked: bool,
    pub double_clicked: bool,
    /// Whether the children are shown (after this frame's clicks).
    pub open: bool,
    pub hovered: bool,
    pub rect: Rect,
}

impl Ui<'_> {
    /// A row with children under it. The triangle (or a double click, or
    /// Left/Right when focused) opens and closes it; it starts open.
    pub fn tree_node(&mut self, label: &str, selected: bool, children: impl FnOnce(&mut Ui)) -> TreeResponse {
        let out = self.tree_row(label, selected, true);
        if out.open {
            self.push_id(label);
            self.indent(self.m.widget_h * 0.6, children);
            self.pop_id();
        }
        out
    }

    /// A row without children, indented like its siblings.
    pub fn tree_leaf(&mut self, label: &str, selected: bool) -> TreeResponse {
        self.tree_row(label, selected, false)
    }

    fn tree_row(&mut self, label: &str, selected: bool, branch: bool) -> TreeResponse {
        let id = self.id(label);
        let rect = self.alloc(Vec2::new(FILL, self.m.widget_h));
        let disc = Rect::from_min_size(rect.min, Vec2::splat(rect.height()));
        let mut r = self.interact(id, rect, Sense::CLICK);
        let focused = self.focusable(id);
        self.key_click(id, &mut r);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let mut open = branch && self.state.open_default(id, true);
        if branch {
            let toggled_by_key = focused
                && self.state.take_key(|k| matches!(k.key, Key::ArrowLeft | Key::ArrowRight)).map(|k| k.key == Key::ArrowRight).is_some_and(|want_open| want_open != open);
            let on_disc = r.clicked && disc.contains(self.state.pointer);
            if on_disc || r.double_clicked || toggled_by_key {
                open = !open;
                *self.state.open(id) = open;
                self.state.request_rebuild = true;
                if on_disc {
                    r.clicked = false;
                }
            }
        }

        // ---- draw ----
        let style = self.text_style();
        if selected {
            self.fill_shaded(rect, self.theme.selection);
        } else if r.hovered || r.held {
            let bg = self.theme.hover(self.theme.panel);
            self.fill(rect, bg);
        }
        let ink = if selected { self.theme.selection_text } else { self.theme.text };
        let dim = if selected { self.theme.selection_text } else { self.theme.text_dim };
        if branch {
            let s = self.m.px(6.0);
            let c = disc.center();
            let w = self.m.px(2.0);
            if open {
                self.draw.line(Vec2::new(c.x - s, c.y - s * 0.5), Vec2::new(c.x, c.y + s * 0.5), w, dim);
                self.draw.line(Vec2::new(c.x, c.y + s * 0.5), Vec2::new(c.x + s, c.y - s * 0.5), w, dim);
            } else {
                self.draw.line(Vec2::new(c.x - s * 0.5, c.y - s), Vec2::new(c.x + s * 0.5, c.y), w, dim);
                self.draw.line(Vec2::new(c.x + s * 0.5, c.y), Vec2::new(c.x - s * 0.5, c.y + s), w, dim);
            }
        }
        let text_rect = Rect::new(Vec2::new(disc.max.x, rect.min.y), Vec2::new(rect.max.x - self.m.pad, rect.max.y));
        self.text_in_rect(label, &style, text_rect, ink);
        self.focus_ring(id, rect);
        TreeResponse { clicked: r.clicked, double_clicked: r.double_clicked, open, hovered: r.hovered, rect }
    }
}
