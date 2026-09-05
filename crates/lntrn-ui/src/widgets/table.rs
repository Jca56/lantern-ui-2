//! A table: a header of labelled columns that drag wider and click to
//! sort, over rows that scroll both ways and are laid out only while in
//! view. The rows are the caller's: it says which are selected and fills
//! each cell with any widget, so a cell can be a label, a value to drag or
//! a toggle. Sorting is the caller's too; the header only says how.
//!
//! ```ignore
//! let resp = ui.table("objects", &cols, objects.len(), None, |t| {
//!     for i in t.visible() {
//!         t.row(i, selected == Some(i), |c| match c.col {
//!             0 => c.label(&objects[i].name),
//!             _ => { c.drag_value("", &mut objects[i].size, 0.1, None, 1); }
//!         });
//!     }
//! });
//! ```

use std::ops::{Deref, DerefMut, Range};

use lntrn_math::{Rect, Vec2};

use crate::icons::{self, Icon};
use crate::id::WidgetId;
use crate::state::CursorIcon;
use crate::ui::{FILL, KeyStep, Sense, Ui};
use crate::widgets::scroll::{ScrollView, visible_rows};

/// Where a column's text lines up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// One column of a table.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub label: String,
    /// Starting width in logical pixels, or [`FILL`] to take what is
    /// left. The user may drag it; the table remembers.
    pub width: f64,
    pub align: Align,
    /// Clicking the header sorts by it (see [`TableResponse::sort`]).
    pub sortable: bool,
}

impl Column {
    pub fn new(label: &str, width: f64) -> Self {
        Self { label: label.to_owned(), width, align: Align::Left, sortable: false }
    }

    /// A column that takes whatever width the others leave.
    pub fn fill(label: &str) -> Self {
        Self::new(label, FILL)
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Numbers line up on the right.
    pub fn right(self) -> Self {
        self.align(Align::Right)
    }

    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }
}

/// Arrow keys while the table has keyboard focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowStep {
    #[default]
    None,
    /// Rows down (negative: up).
    By(i32),
    First,
    Last,
}

/// What a table reported this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableResponse {
    /// A row was clicked: select it.
    pub clicked: Option<usize>,
    pub double_clicked: Option<usize>,
    pub hovered: Option<usize>,
    /// The column the header says to sort by, and whether ascending.
    pub sort: Option<(usize, bool)>,
    /// The header was clicked this frame: sort the rows again.
    pub sort_changed: bool,
    /// Move the selection by this.
    pub step: RowStep,
    /// A column was dragged wider or narrower.
    pub resized: bool,
}

/// One cell while its row is laid out: a [`Ui`] positioned inside the
/// cell (every widget works), plus [`Cell::label`], which lines text up
/// the way the column asks.
pub struct Cell<'u, 'a> {
    ui: &'u mut Ui<'a>,
    pub col: usize,
    pub rect: Rect,
    pub align: Align,
}

impl<'a> Deref for Cell<'_, 'a> {
    type Target = Ui<'a>;
    fn deref(&self) -> &Ui<'a> {
        self.ui
    }
}

impl<'a> DerefMut for Cell<'_, 'a> {
    fn deref_mut(&mut self) -> &mut Ui<'a> {
        self.ui
    }
}

impl Cell<'_, '_> {
    /// One line of text filling the cell, aligned as the column asks.
    pub fn label(&mut self, s: &str) {
        let ink = self.ui.theme.text;
        self.aligned(s, ink);
    }

    pub fn label_dim(&mut self, s: &str) {
        let ink = self.ui.theme.text_dim;
        self.aligned(s, ink);
    }

    fn aligned(&mut self, s: &str, ink: lntrn_math::Color) {
        let style = self.ui.text_style();
        match self.align {
            Align::Left => self.ui.text_in_rect(s, &style, self.rect, ink),
            Align::Center => self.ui.text_centered(s, &style, self.rect, ink),
            Align::Right => self.ui.text_right(s, &style, self.rect, ink),
        }
    }
}

/// The body of a table while it is laid out (see [`Ui::table`]).
pub struct Table<'u, 'a> {
    ui: &'u mut Ui<'a>,
    id: WidgetId,
    /// Each column's left edge in window space and width, physical pixels.
    cols: Vec<(f64, f64)>,
    aligns: Vec<Align>,
    width: f64,
    row_h: f64,
    view: ScrollView,
    visible: Range<usize>,
    resp: TableResponse,
}

impl Table<'_, '_> {
    /// The rows in view: lay these out, and only these.
    pub fn visible(&self) -> Range<usize> {
        self.visible.clone()
    }

    /// Lay out row `i`: `cell(c)` fills each cell. A `selected` row is lit.
    /// Returns whether the row was clicked.
    pub fn row(&mut self, i: usize, selected: bool, mut cell: impl FnMut(&mut Cell<'_, '_>)) -> bool {
        let top = self.view.origin.y + i as f64 * self.row_h;
        let rect = Rect::from_min_size(Vec2::new(self.view.origin.x, top), Vec2::new(self.width, self.row_h));
        let id = self.id.with("row").with_index(i);
        let ui = &mut *self.ui;
        let m = ui.m;
        // Hover first, without the press, so the background goes under the cells.
        let hover = ui.interact(id, rect, Sense::NONE);
        if selected {
            ui.fill_shaded(rect, ui.theme.shaded(ui.theme.selection));
        } else if hover.hovered {
            ui.fill(rect, ui.theme.hover(ui.theme.panel.mid()));
        } else if i % 2 == 1 {
            ui.draw.rect(rect, ui.theme.panel.bottom);
        }
        if hover.hovered {
            self.resp.hovered = Some(i);
        }
        let saved = (ui.cursor(), ui.avail_width(), ui.clip());
        ui.push_index(i);
        for (c, &(x, w)) in self.cols.iter().enumerate() {
            let cell_rect = Rect::from_min_size(Vec2::new(x, top), Vec2::new(w, self.row_h));
            let inner = Rect::new(Vec2::new(cell_rect.min.x + m.pad, top + ((self.row_h - m.widget_h) * 0.5).round()), Vec2::new(cell_rect.max.x - m.pad, top + ((self.row_h + m.widget_h) * 0.5).round()));
            let clip = cell_rect.intersection(&saved.2);
            ui.set_clip(clip);
            ui.draw.push_clip(clip);
            ui.set_cursor(inner.min);
            ui.set_avail_width(inner.width());
            ui.push_index(c);
            let align = self.aligns[c];
            ui.row(|ui| cell(&mut Cell { ui, col: c, rect: inner, align }));
            ui.pop_id();
            ui.draw.pop_clip();
        }
        ui.pop_id();
        ui.set_clip(saved.2);
        ui.set_cursor(saved.0);
        ui.set_avail_width(saved.1);
        // The press, after the cells had their chance at it.
        let r = ui.interact(id, rect, Sense::CLICK);
        if r.pressed {
            // The table takes keyboard focus, so arrows move the selection.
            ui.state.focus = Some(self.id);
        }
        if r.clicked {
            self.resp.clicked = Some(i);
        }
        if r.double_clicked {
            self.resp.double_clicked = Some(i);
        }
        r.clicked
    }
}

impl Ui<'_> {
    /// A table `height` tall (the remaining height when `None`) with
    /// `rows` rows under `columns`. `body(t)` lays the rows out:
    /// `for i in t.visible() { t.row(i, selected, |c| ...) }`. One Tab stop;
    /// Up, Down, Home and End step the selection through
    /// [`TableResponse::step`].
    pub fn table(&mut self, label: &str, columns: &[Column], rows: usize, height: Option<f64>, body: impl FnOnce(&mut Table<'_, '_>)) -> TableResponse {
        let id = self.id(label);
        let m = self.m;
        let row_h = m.widget_h + m.gap;
        let h = height.unwrap_or_else(|| self.remaining_height()).max(m.widget_h * 2.0);
        let outer = self.alloc(Vec2::new(FILL, h));
        let (header, viewport) = outer.take_top(m.widget_h);

        // Column widths: remembered, reset when the columns change.
        let mem = self.state.table_mem(id);
        if mem.widths.len() != columns.len() {
            mem.widths = columns.iter().map(|c| c.width).collect();
            mem.sort = None;
        }
        let logical = mem.widths.clone();
        let sort = mem.sort;
        let inner_w = (viewport.width() - m.scrollbar_w - m.gap).max(0.0);
        let fixed: f64 = logical.iter().filter(|w| **w != FILL).map(|w| m.px(*w).max(m.widget_h)).sum();
        let fills = logical.iter().filter(|w| **w == FILL).count();
        let fill_w = if fills > 0 { ((inner_w - fixed) / fills as f64).max(m.widget_h).round() } else { 0.0 };
        let widths: Vec<f64> = logical.iter().map(|w| if *w == FILL { fill_w } else { m.px(*w).max(m.widget_h) }).collect();
        let total_w: f64 = widths.iter().sum();
        let content_w = total_w.max(inner_w);
        let cols_at = |left: f64| -> Vec<(f64, f64)> {
            let mut x = left;
            widths
                .iter()
                .map(|w| {
                    let c = (x, *w);
                    x += w;
                    c
                })
                .collect()
        };

        // Keyboard: one Tab stop; arrows step the selection.
        let mut resp = TableResponse { sort, ..TableResponse::default() };
        if self.focusable(id, outer) {
            resp.step = match self.key_step(id) {
                KeyStep::None => RowStep::None,
                KeyStep::By(n) => RowStep::By(-n),
                KeyStep::Min => RowStep::First,
                KeyStep::Max => RowStep::Last,
            };
        }

        // ---- body (its widgets' ids live under the table's) ----------------------
        let aligns: Vec<Align> = columns.iter().map(|c| c.align).collect();
        let mut body_resp = TableResponse::default();
        self.push_id(label);
        let view = self.scroll_core(id.with("body"), "body", viewport, Some(content_w), Some(rows as f64 * row_h), |ui, view| {
            let (first, last) = visible_rows(view, row_h, rows);
            let mut t = Table { ui, id, cols: cols_at(view.origin.x), aligns, width: content_w, row_h, view, visible: first..last, resp: TableResponse::default() };
            body(&mut t);
            body_resp = t.resp;
        });
        self.pop_id();
        resp.clicked = body_resp.clicked;
        resp.double_clicked = body_resp.double_clicked;
        resp.hovered = body_resp.hovered;

        // ---- header: grips first (they win the press), then the labels --------
        let clip = header.intersection(&self.clip());
        self.draw.push_clip(clip);
        self.draw.rect_gradient(header, self.theme.header.top, self.theme.header.bottom);
        self.draw.hline(header.min.x, header.max.x, header.max.y - m.border, m.border, self.theme.border_dark);
        let cols = cols_at(header.min.x - view.offset.x);
        let style = self.text_style();
        for (c, &(x, w)) in cols.iter().enumerate() {
            let edge = x + w;
            let grip = Rect::from_center_size(Vec2::new(edge, header.center().y), Vec2::new(m.px(10.0), header.height()));
            let g = self.interact(id.with("col").with_index(c).with("grip"), grip, Sense::DRAG);
            if g.hovered || g.dragging {
                self.state.cursor_icon = CursorIcon::EwResize;
            }
            if g.dragging && g.drag_delta.x != 0.0 {
                let new_w = (w + g.drag_delta.x).max(m.widget_h);
                self.state.table_mem(id).widths[c] = new_w / m.scale;
                resp.resized = true;
                self.state.request_rebuild = true;
            }
            self.draw.vline(edge, header.min.y + m.pad, header.max.y - m.pad, m.border, self.theme.border_light.fade(0.5));
        }
        for (c, &(x, w)) in cols.iter().enumerate() {
            let cell = Rect::from_min_size(Vec2::new(x, header.min.y), Vec2::new(w, header.height()));
            let col = &columns[c];
            let r = self.interact(id.with("col").with_index(c), cell, if col.sortable { Sense::CLICK } else { Sense::NONE });
            if col.sortable && r.hovered {
                self.state.cursor_icon = CursorIcon::Pointer;
                self.fill(cell, self.theme.hover(self.theme.header.mid()));
            }
            let sorted = sort.filter(|(sc, _)| *sc == c);
            let arrow_w = if sorted.is_some() { m.widget_h * 0.5 } else { 0.0 };
            let inner = Rect::new(Vec2::new(cell.min.x + m.pad, cell.min.y), Vec2::new(cell.max.x - m.pad - arrow_w, cell.max.y));
            let ink = if sorted.is_some() { self.theme.accent } else { self.theme.text };
            match col.align {
                Align::Left => self.text_in_rect(&col.label, &style, inner, ink),
                Align::Center => self.text_centered(&col.label, &style, inner, ink),
                Align::Right => self.text_right(&col.label, &style, inner, ink),
            }
            if let Some((_, asc)) = sorted {
                let arrow = Rect::from_center_size(Vec2::new(cell.max.x - m.pad - arrow_w * 0.5, cell.center().y), Vec2::splat(arrow_w));
                icons::draw(self.draw, arrow, if asc { Icon::Up } else { Icon::Down }, ink, m.px(2.0));
            }
            if r.clicked && col.sortable {
                let next = match sort {
                    Some((sc, asc)) if sc == c => (c, !asc),
                    _ => (c, true),
                };
                self.state.table_mem(id).sort = Some(next);
                resp.sort = Some(next);
                resp.sort_changed = true;
                self.state.request_rebuild = true;
            }
        }
        self.draw.pop_clip();
        self.draw.stroke_rect(outer, m.border, 0.0, self.theme.border_dark);
        self.focus_ring(id, outer);
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_build() {
        let c = Column::new("Size", 150.0).right().sortable();
        assert_eq!((c.width, c.align, c.sortable), (150.0, Align::Right, true));
        let f = Column::fill("On");
        assert_eq!(f.width, FILL);
        assert_eq!(f.align, Align::Left);
        assert!(!f.sortable);
        assert_eq!(RowStep::default(), RowStep::None);
    }
}
