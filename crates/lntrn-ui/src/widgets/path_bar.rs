//! A path as crumbs in one field (U032): click a crumb to go there, click
//! the current folder's name or the empty end to type a path instead —
//! Enter takes it, Escape or a click elsewhere gives it up. Crumbs that
//! do not fit fall off the left behind a "…" that goes to the last of them.
//! One crumb may be marked (drawn in the accent): the project a file
//! tree belongs to, say.

use std::path::{Path, PathBuf};

use lntrn_math::{Color, Rect, Vec2};

use crate::id::WidgetId;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathBarResponse {
    /// A crumb was clicked: go to this folder.
    pub go: Option<PathBuf>,
    /// A path was typed and Enter pressed; what it means is the caller's.
    pub typed: Option<String>,
    /// The bar is a text field right now.
    pub editing: bool,
}

impl Ui<'_> {
    /// `text` is the caller's buffer for the typed path; it is filled
    /// with `path` when editing starts.
    pub fn path_bar(&mut self, label: &str, path: &Path, text: &mut String) -> PathBarResponse {
        self.path_bar_marked(label, path, None, text)
    }

    /// Put the bar named `label` into typing mode, as a click on its end
    /// does: for an Open… command that wants a path typed. Call it before
    /// drawing the bar in the same frame.
    pub fn path_bar_edit(&mut self, label: &str, path: &Path, text: &mut String) {
        let id = self.id(label);
        self.path_bar_start(id, path, text);
    }

    fn path_bar_start(&mut self, id: WidgetId, path: &Path, text: &mut String) {
        let edit_id = id.with("edit");
        *text = path.display().to_string();
        *self.state.open(id) = true;
        self.state.focus = Some(edit_id);
        self.state.focus_visible = false;
        let te = self.state.text_edit(edit_id);
        te.cursor = text.len();
        te.anchor = te.cursor;
        self.state.request_rebuild = true;
    }

    /// [`Self::path_bar`] with the crumb at `mark` drawn in the accent.
    pub fn path_bar_marked(&mut self, label: &str, path: &Path, mark: Option<&Path>, text: &mut String) -> PathBarResponse {
        let id = self.id(label);
        let edit_id = id.with("edit");
        let m = self.m;
        let rect = self.alloc(Vec2::new(FILL, m.widget_h));
        let mut out = PathBarResponse::default();
        if *self.state.open(id) {
            let r = self.text_edit_core(edit_id, rect, text);
            out.editing = true;
            if r.committed {
                out.typed = Some(text.clone());
            }
            if r.committed || r.cancelled || !r.focused {
                *self.state.open(id) = false;
                out.editing = false;
                self.state.request_rebuild = true;
            }
            return out;
        }

        let bar = self.interact(id, rect, Sense::NONE);
        let theme = self.theme;
        let well = if bar.hovered { theme.hover(theme.field) } else { theme.field };
        self.recessed(rect, well);
        let style = self.text_style();
        let parts: Vec<PathBuf> = path.ancestors().map(Path::to_path_buf).collect::<Vec<_>>().into_iter().rev().collect();
        let names: Vec<String> = parts.iter().map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "/".to_owned())).collect();
        let widths: Vec<f64> = names.iter().map(|n| self.measure(n, &style) + m.pad * 1.5).collect();
        let sep_w = (m.widget_h * 0.5).round();
        let ell_w = self.measure("…", &style) + m.pad * 1.5;
        // The last crumbs that fit, from the right; "…" stands for the rest.
        let avail = rect.width() - m.pad * 2.0;
        let mut first = parts.len();
        let mut used = 0.0;
        while first > 0 {
            let need = widths[first - 1] + if first < parts.len() { sep_w } else { 0.0 };
            let reserve = if first > 1 { ell_w + sep_w } else { 0.0 };
            if first < parts.len() && used + need + reserve > avail {
                break;
            }
            used += need;
            first -= 1;
        }
        let mut items: Vec<(String, f64, PathBuf)> = Vec::new();
        if first > 0 {
            items.push(("…".to_owned(), ell_w, parts[first - 1].clone()));
        }
        for i in first..parts.len() {
            items.push((names[i].clone(), widths[i], parts[i].clone()));
        }
        let last = items.len().saturating_sub(1);
        let inset = m.px(3.0);
        let mut x = rect.min.x + m.pad;
        let mut edit = false;
        for (i, (name, w, target)) in items.iter().enumerate() {
            let r = Rect::new(Vec2::new(x, rect.min.y + inset), Vec2::new((x + w).min(rect.max.x - m.pad), rect.max.y - inset));
            let cr = self.interact(id.with("crumb").with_index(i), r, Sense::CLICK);
            if cr.hovered {
                self.state.cursor_icon = if i == last { CursorIcon::Text } else { CursorIcon::Pointer };
                if i != last {
                    self.draw.rounded_rect(r, m.radius * 0.6, theme.selection.fade(0.35));
                }
            }
            let color = if mark == Some(target.as_path()) {
                theme.accent
            } else if i == last {
                theme.text
            } else {
                theme.text_dim
            };
            self.text_centered(name, &style, r, color);
            if cr.clicked {
                if i == last {
                    edit = true;
                } else {
                    out.go = Some(target.clone());
                }
            }
            x = r.max.x;
            if i != last {
                chevron(self, Vec2::new(x + sep_w * 0.5, rect.center().y), m.px(4.0), m.px(1.5), theme.text_dim);
                x += sep_w;
            }
        }
        // The empty end: a click there types the path.
        let tail = Rect::new(Vec2::new(x.min(rect.max.x), rect.min.y), rect.max);
        let tr = self.interact(id.with("tail"), tail, Sense::CLICK);
        if tr.hovered {
            self.state.cursor_icon = CursorIcon::Text;
        }
        if tr.clicked || edit {
            self.path_bar_start(id, path, text);
        }
        out
    }
}

/// A small "›" at `c`.
fn chevron(ui: &mut Ui, c: Vec2, s: f64, w: f64, color: Color) {
    ui.draw.line(Vec2::new(c.x - s * 0.5, c.y - s), Vec2::new(c.x + s * 0.5, c.y), w, color);
    ui.draw.line(Vec2::new(c.x + s * 0.5, c.y), Vec2::new(c.x - s * 0.5, c.y + s), w, color);
}
