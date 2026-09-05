//! Scroll areas: down only, both ways, and a virtual list that lays out
//! only the rows in view. One core does the wheel, the bars, the clip and
//! keeping keyboard focus in view.

use lntrn_math::{Rect, Vec2};

use crate::id::WidgetId;
use crate::ui::{FILL, Sense, Ui};

/// What the content of a scroll area is told about where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollView {
    /// The visible part, in window space (inside the bars).
    pub viewport: Rect,
    /// How far the content is scrolled: `x` across, `y` down.
    pub offset: Vec2,
    /// Where the content's top-left is laid out: `viewport.min - offset`.
    pub origin: Vec2,
}

impl Ui<'_> {
    /// A vertically scrolling region. `height` of `None` takes the remaining
    /// height. Content height is measured as `f` lays out, so the scrollbar
    /// is right from the second frame on.
    pub fn scroll_area(&mut self, label: &str, height: Option<f64>, f: impl FnOnce(&mut Ui)) {
        let id = self.id(label);
        let h = height.unwrap_or_else(|| self.remaining_height()).max(0.0);
        let viewport = self.alloc(Vec2::new(FILL, h));
        self.scroll_core(id, label, viewport, None, None, |ui, _| f(ui));
    }

    /// A region that scrolls both ways over content `content_w` physical
    /// pixels wide (the width `f` lays out in). Shift+wheel or a sideways
    /// wheel scrolls across. `f` gets the [`ScrollView`] so it can skip
    /// what is out of sight.
    pub fn scroll_area_2d(&mut self, label: &str, height: Option<f64>, content_w: f64, f: impl FnOnce(&mut Ui, ScrollView)) {
        let id = self.id(label);
        let h = height.unwrap_or_else(|| self.remaining_height()).max(0.0);
        let viewport = self.alloc(Vec2::new(FILL, h));
        self.scroll_core(id, label, viewport, Some(content_w.max(0.0)), None, f);
    }

    /// `count` rows of `row_h` physical pixels each, of which only those in
    /// view are laid out: `f(ui, i)` declares row `i` with the cursor at its
    /// top-left. Ten thousand rows cost the same as ten.
    pub fn virtual_list(&mut self, label: &str, count: usize, row_h: f64, height: Option<f64>, mut f: impl FnMut(&mut Ui, usize)) {
        let id = self.id(label);
        let h = height.unwrap_or_else(|| self.remaining_height()).max(0.0);
        let viewport = self.alloc(Vec2::new(FILL, h));
        let row_h = row_h.max(1.0);
        self.scroll_core(id, label, viewport, None, Some(count as f64 * row_h), |ui, view| {
            let (first, last) = visible_rows(view, row_h, count);
            for i in first..last {
                ui.set_cursor(Vec2::new(view.origin.x, view.origin.y + i as f64 * row_h));
                ui.push_index(i);
                f(ui, i);
                ui.pop_id();
            }
        });
    }

    /// The scrolling machinery behind the areas above and the table.
    /// `content_w` turns sideways scrolling on (with a bar along the
    /// bottom); `known_h` is the content height when it is known up front,
    /// else it is measured as `f` lays out. Returns where the view ended up.
    pub(crate) fn scroll_core(&mut self, id: WidgetId, label: &str, viewport: Rect, content_w: Option<f64>, known_h: Option<f64>, f: impl FnOnce(&mut Ui, ScrollView)) -> ScrollView {
        let m = self.m;
        let bar = m.scrollbar_w;
        let two_way = content_w.is_some();
        let mem = *self.state.scroll(id);
        // The bars keep their room whether or not they are needed, so the
        // layout does not jump when content grows.
        let inner_w = (viewport.width() - bar - m.gap).max(0.0);
        let inner_h = if two_way { (viewport.height() - bar - m.gap).max(0.0) } else { viewport.height() };
        let inner = Rect::from_min_size(viewport.min, Vec2::new(inner_w, inner_h));
        let content_w = content_w.unwrap_or(inner_w);
        let content_h = known_h.unwrap_or(mem.content.y);
        let max = Vec2::new((content_w - inner_w).max(0.0), (content_h - inner_h).max(0.0));
        let mut offset = Vec2::new(mem.offset.x.clamp(0.0, max.x), mem.offset.y.clamp(0.0, max.y));

        // Wheel over the viewport: down, or across with Shift or a sideways wheel.
        let over = self.state.pointer_in_window && viewport.intersection(&self.clip()).contains(self.state.pointer);
        let popup_blocks = self.state.popup.is_some_and(|(r, layer)| layer > self.layer() && r.contains(self.state.pointer));
        if over && !popup_blocks && self.state.wheel != Vec2::ZERO {
            let w = self.state.wheel;
            if two_way {
                let (dx, dy) = if self.state.mods.shift() { (w.y, w.x) } else { (w.x, w.y) };
                offset.x = (offset.x - dx).clamp(0.0, max.x);
                offset.y = (offset.y - dy).clamp(0.0, max.y);
            } else {
                offset.y = (offset.y - w.y).clamp(0.0, max.y);
            }
            self.state.wheel = Vec2::ZERO;
        }

        // Bars: down the right edge, and along the bottom when two-way.
        let track_v = Rect::new(Vec2::new(viewport.max.x - bar, viewport.min.y), Vec2::new(viewport.max.x, viewport.min.y + inner_h));
        if max.y > 0.0 {
            offset.y = self.scrollbar(id.with("thumb"), track_v, false, inner_h, content_h, offset.y);
        }
        if two_way && max.x > 0.0 {
            let track_h = Rect::new(Vec2::new(viewport.min.x, viewport.max.y - bar), Vec2::new(viewport.min.x + inner_w, viewport.max.y));
            offset.x = self.scrollbar(id.with("hthumb"), track_h, true, inner_w, content_w, offset.x);
        }

        // Lay the content out shifted by the offset, clipped to the viewport.
        let saved_clip = self.clip();
        let saved_avail = self.avail_width();
        let after_cursor = self.cursor();
        let saved_bottom = self.bottom;
        let content_clip = inner.intersection(&saved_clip);
        self.set_clip(content_clip);
        self.bottom = inner.max.y;
        self.draw.push_clip(content_clip);
        let origin = inner.min - offset;
        self.set_cursor(origin);
        self.set_avail_width(content_w);
        self.push_id(label);
        let view = ScrollView { viewport: inner, offset, origin };
        f(self, view);
        self.pop_id();
        let content_h = known_h.unwrap_or((self.cursor().y - origin.y - m.gap).max(0.0));
        let max = Vec2::new((content_w - inner_w).max(0.0), (content_h - inner_h).max(0.0));

        // Keyboard focus that just landed on something of ours outside the
        // viewport scrolls to it.
        if self.state.focus_moved
            && let Some(fr) = self.state.focus_rect
            && fr.min.x >= origin.x - 1.0
            && fr.max.x <= origin.x + content_w + 1.0
            && fr.min.y >= origin.y - 1.0
            && fr.max.y <= origin.y + content_h + 1.0
        {
            if fr.min.y < inner.min.y {
                offset.y -= inner.min.y - fr.min.y + m.gap;
            } else if fr.max.y > inner.max.y {
                offset.y += fr.max.y - inner.max.y + m.gap;
            }
            if two_way {
                if fr.min.x < inner.min.x {
                    offset.x -= inner.min.x - fr.min.x + m.gap;
                } else if fr.max.x > inner.max.x {
                    offset.x += fr.max.x - inner.max.x + m.gap;
                }
            }
            let clamped = Vec2::new(offset.x.clamp(0.0, max.x), offset.y.clamp(0.0, max.y));
            if clamped != mem.offset {
                self.state.request_rebuild = true;
            }
            offset = clamped;
        }
        self.draw.pop_clip();
        self.set_clip(saved_clip);
        self.bottom = saved_bottom;
        self.set_avail_width(saved_avail);
        self.set_cursor(after_cursor);

        let mem = self.state.scroll(id);
        mem.offset = offset;
        mem.content = Vec2::new(content_w, content_h);
        ScrollView { viewport: inner, offset, origin }
    }

    /// A bar along `track` for a viewport `view_len` long over `content_len`
    /// of content; returns the offset after any drag of the thumb.
    fn scrollbar(&mut self, id: WidgetId, track: Rect, across: bool, view_len: f64, content_len: f64, offset: f64) -> f64 {
        let m = self.m;
        let bar = m.scrollbar_w;
        let max = (content_len - view_len).max(0.0);
        let len = if across { track.width() } else { track.height() };
        let thumb_len = (len * view_len / content_len.max(1.0)).max(m.widget_h).min(len);
        let travel = len - thumb_len;
        let thumb_at = |off: f64| {
            let t = if max > 0.0 { off / max } else { 0.0 };
            if across {
                Rect::from_min_size(Vec2::new(track.min.x + travel * t, track.min.y), Vec2::new(thumb_len, bar))
            } else {
                Rect::from_min_size(Vec2::new(track.min.x, track.min.y + travel * t), Vec2::new(bar, thumb_len))
            }
        };
        let r = self.interact(id, thumb_at(offset), Sense::DRAG);
        let mut offset = offset;
        if r.dragging && travel > 0.0 && !r.pressed {
            let d = if across { r.drag_delta.x } else { r.drag_delta.y };
            offset = (offset + d / travel * max).clamp(0.0, max);
        }
        self.draw.rounded_rect(track, bar * 0.5, self.theme.field);
        let thumb = thumb_at(offset);
        let base = if r.held { self.theme.shaded(self.theme.accent) } else if r.hovered { self.theme.hover_g(self.theme.widget) } else { self.theme.widget };
        self.draw.rounded_rect_gradient(thumb.shrink(m.border), bar * 0.5, base.top, base.bottom);
        self.draw.stroke_rect(thumb, m.border, bar * 0.5, self.theme.border_dark);
        offset
    }
}

/// The rows of `row_h` each that a view over `count` rows shows: `first..last`.
pub(crate) fn visible_rows(view: ScrollView, row_h: f64, count: usize) -> (usize, usize) {
    let first = ((view.offset.y / row_h).floor().max(0.0) as usize).min(count);
    let last = (((view.offset.y + view.viewport.height()) / row_h).ceil().max(0.0) as usize).min(count);
    (first, last.max(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_in_view() {
        let view = |off: f64, h: f64| ScrollView { viewport: Rect::from_xywh(0.0, 0.0, 100.0, h), offset: Vec2::new(0.0, off), origin: Vec2::new(0.0, -off) };
        assert_eq!(visible_rows(view(0.0, 100.0), 30.0, 1000), (0, 4), "partly visible rows count");
        assert_eq!(visible_rows(view(95.0, 100.0), 30.0, 1000), (3, 7));
        assert_eq!(visible_rows(view(0.0, 100.0), 30.0, 2), (0, 2), "clamped to the rows there are");
        assert_eq!(visible_rows(view(9999.0, 100.0), 30.0, 10), (10, 10), "past the end: nothing");
    }
}
