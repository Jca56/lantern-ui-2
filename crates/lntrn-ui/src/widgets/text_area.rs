//! Multi-line text editing: wrapped rows, a caret and selection across
//! lines, Up and Down with a remembered column, Enter for a new line,
//! Ctrl+Enter to commit, and scrolling that keeps the caret in view.

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};
use crate::widgets::TextResponse;
use crate::widgets::text_field::{Edit, byte_at_x, clipboard_key, insert_typed, next_boundary, prev_boundary, take_edits, x_at};

/// The row (index into `rows`) a caret at byte `b` is on: the last row that
/// starts at or before it, so a caret at a soft wrap shows on the next row.
fn row_of(rows: &[(u32, u32)], b: usize) -> usize {
    rows.iter().rposition(|&(s, _)| s as usize <= b).unwrap_or(0)
}

/// Word boundaries around `b`: alphanumerics run together, anything else is
/// its own word.
fn word_at(s: &str, b: usize) -> (usize, usize) {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = b.min(s.len());
    while start > 0 {
        let p = prev_boundary(s, start);
        if s[p..].chars().next().is_some_and(is_word) { start = p } else { break }
    }
    let mut end = b.min(s.len());
    while end < s.len() {
        if s[end..].chars().next().is_some_and(is_word) { end = next_boundary(s, end) } else { break }
    }
    (start, end)
}

impl Ui<'_> {
    /// Editable text over several lines, filling the width and `height`
    /// tall (the remaining height when `None`). Enter breaks the line;
    /// Ctrl+Enter reports `committed`.
    pub fn text_area(&mut self, label: &str, value: &mut String, height: Option<f64>) -> TextResponse {
        let id = self.id(label);
        let h = height.unwrap_or_else(|| self.remaining_height()).max(self.m.widget_h * 2.0);
        let rect = self.alloc(Vec2::new(FILL, h));
        let r = self.interact(id, rect, Sense::FOCUS);
        self.focusable(id);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Text;
        }
        let style = self.text_style();
        let lh = style.line_height() as f64;
        let bar_w = self.m.scrollbar_w;
        let inner = Rect::new(rect.min + Vec2::splat(self.m.pad), Vec2::new(rect.max.x - self.m.pad - bar_w - self.m.gap, rect.max.y - self.m.pad));
        let width = inner.width().max(1.0);
        let mut rows = Vec::new();
        self.text.line_ranges(value, &style, width as f32, &mut rows);
        let mut out = TextResponse { focused: r.focused, ..TextResponse::default() };
        let mut scroll = self.state.scroll(id).offset;

        // Wheel over the area scrolls it.
        let popup_blocks = self.state.popup.is_some_and(|(p, layer)| layer > self.layer() && p.contains(self.state.pointer));
        if self.state.pointer_in_window && !popup_blocks && rect.intersection(&self.clip()).contains(self.state.pointer) && self.state.wheel.y != 0.0 {
            scroll -= self.state.wheel.y;
            self.state.wheel = Vec2::ZERO;
        }

        let mut adv = Vec::new();
        if r.focused {
            // ---- pointer ----
            if r.pressed || r.dragging {
                let p = self.state.pointer;
                let row = (((p.y - inner.min.y + scroll) / lh).floor().max(0.0) as usize).min(rows.len() - 1);
                let (s, e) = (rows[row].0 as usize, rows[row].1 as usize);
                self.text.advances(&value[s..e], &style, &mut adv);
                let b = s + byte_at_x(&adv, p.x - inner.min.x);
                let shift = self.state.mods.shift();
                let te = self.state.text_edit(id);
                te.cursor = b;
                if r.pressed && !shift {
                    te.anchor = b;
                }
                if r.double_clicked {
                    let (ws, we) = word_at(value, b);
                    te.anchor = ws;
                    te.cursor = we;
                }
                *self.state.floats(id, [-1.0; 4]) = [-1.0; 4];
            }
            // ---- keys and typed text, in arrival order ----
            for edit in take_edits(self.state) {
                let k = match edit {
                    Edit::Text(t) => {
                        if insert_typed(self.state, id, value, &t) {
                            out.changed = true;
                            *self.state.floats(id, [-1.0; 4]) = [-1.0; 4];
                        }
                        continue;
                    }
                    Edit::Key(k) => k,
                };
                if k.mods.ctrl() && matches!(k.key, Key::Char('c' | 'x' | 'v')) {
                    out.changed |= clipboard_key(self.state, id, k.key, value, true);
                    continue;
                }
                let te = self.state.text_edit(id);
                let (s0, s1) = te.selection();
                let mut keep_column = false;
                match k.key {
                    Key::ArrowLeft => {
                        te.cursor = if te.has_selection() && !k.mods.shift() { s0 } else { prev_boundary(value, te.cursor) };
                        if k.mods.ctrl() {
                            te.cursor = word_at(value, prev_boundary(value, te.cursor)).0;
                        }
                    }
                    Key::ArrowRight => {
                        te.cursor = if te.has_selection() && !k.mods.shift() { s1 } else { next_boundary(value, te.cursor) };
                        if k.mods.ctrl() {
                            te.cursor = word_at(value, te.cursor).1.max(next_boundary(value, te.cursor)).min(value.len());
                        }
                    }
                    Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
                        keep_column = true;
                        let cursor = te.cursor;
                        let row = row_of(&rows, cursor);
                        let by = match k.key {
                            Key::ArrowUp | Key::ArrowDown => 1,
                            _ => ((inner.height() / lh).floor() as usize).max(1),
                        };
                        let target = if matches!(k.key, Key::ArrowUp | Key::PageUp) { row.saturating_sub(by) } else { (row + by).min(rows.len() - 1) };
                        if target != row {
                            let (s, e) = (rows[row].0 as usize, rows[row].1 as usize);
                            let remembered = self.state.floats(id, [-1.0; 4])[0];
                            let x = if remembered >= 0.0 {
                                remembered
                            } else {
                                self.text.advances(&value[s..e], &style, &mut adv);
                                x_at(&adv, cursor - s)
                            };
                            let (ts, te2) = (rows[target].0 as usize, rows[target].1 as usize);
                            self.text.advances(&value[ts..te2], &style, &mut adv);
                            let b = ts + byte_at_x(&adv, x);
                            self.state.text_edit(id).cursor = b;
                            self.state.floats(id, [-1.0; 4])[0] = x;
                        } else if matches!(k.key, Key::ArrowUp | Key::PageUp) {
                            self.state.text_edit(id).cursor = 0;
                        } else {
                            self.state.text_edit(id).cursor = value.len();
                        }
                    }
                    Key::Home => {
                        te.cursor = if k.mods.ctrl() { 0 } else { rows[row_of(&rows, te.cursor)].0 as usize };
                    }
                    Key::End => {
                        te.cursor = if k.mods.ctrl() { value.len() } else { rows[row_of(&rows, te.cursor)].1 as usize };
                    }
                    Key::Backspace => {
                        if te.has_selection() {
                            value.replace_range(s0..s1, "");
                            te.cursor = s0;
                        } else if te.cursor > 0 {
                            let p = prev_boundary(value, te.cursor);
                            value.replace_range(p..te.cursor, "");
                            te.cursor = p;
                        }
                        te.anchor = te.cursor;
                        out.changed = true;
                    }
                    Key::Delete => {
                        if te.has_selection() {
                            value.replace_range(s0..s1, "");
                            te.cursor = s0;
                        } else if te.cursor < value.len() {
                            let n = next_boundary(value, te.cursor);
                            value.replace_range(te.cursor..n, "");
                        }
                        te.anchor = te.cursor;
                        out.changed = true;
                    }
                    Key::Enter if k.mods.ctrl() => out.committed = true,
                    Key::Enter => {
                        value.replace_range(s0..s1, "\n");
                        te.cursor = s0 + 1;
                        te.anchor = te.cursor;
                        out.changed = true;
                    }
                    Key::Escape => out.cancelled = true,
                    Key::Char('a') if k.mods.ctrl() => {
                        te.anchor = 0;
                        te.cursor = value.len();
                    }
                    _ => {}
                }
                let te = self.state.text_edit(id);
                if !k.mods.shift() && matches!(k.key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::PageUp | Key::PageDown) {
                    te.anchor = te.cursor;
                }
                if !keep_column {
                    *self.state.floats(id, [-1.0; 4]) = [-1.0; 4];
                }
            }
            if out.changed {
                self.text.line_ranges(value, &style, width as f32, &mut rows);
                *self.state.floats(id, [-1.0; 4]) = [-1.0; 4];
            }
            // ---- keep the caret in view ----
            let te = self.state.text_edit(id);
            te.cursor = te.cursor.min(value.len());
            te.anchor = te.anchor.min(value.len());
            let y = row_of(&rows, te.cursor) as f64 * lh;
            if y < scroll {
                scroll = y;
            }
            if y + lh > scroll + inner.height() {
                scroll = y + lh - inner.height();
            }
        }
        let content_h = rows.len() as f64 * lh;
        let max_scroll = (content_h - inner.height()).max(0.0);
        scroll = scroll.clamp(0.0, max_scroll);

        // ---- draw ----
        let well = if r.focused || r.hovered { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(rect, well);
        if r.focused {
            self.outline(rect, self.m.border, self.theme.focus);
        }
        let clip = inner.intersection(&self.clip());
        self.draw.push_clip(clip);
        let origin = Vec2::new(inner.min.x, inner.min.y - scroll);
        let te = self.state.text_edit(id).clone();
        if r.focused && te.has_selection() {
            let (s0, s1) = te.selection();
            for (i, &(rs, re)) in rows.iter().enumerate() {
                let (rs, re) = (rs as usize, re as usize);
                if re < s0 || rs > s1 || (rs == re && !(s0 <= rs && rs < s1)) {
                    continue;
                }
                let y = origin.y + i as f64 * lh;
                if y + lh < clip.min.y || y > clip.max.y {
                    continue;
                }
                self.text.advances(&value[rs..re], &style, &mut adv);
                let x0 = origin.x + x_at(&adv, s0.max(rs) - rs);
                let mut x1 = origin.x + x_at(&adv, s1.min(re) - rs);
                if s1 > re {
                    x1 += self.m.px(6.0); // the newline has a little width
                }
                self.draw.rect(Rect::new(Vec2::new(x0, y), Vec2::new(x1.max(x0 + 1.0), y + lh)), self.theme.selection);
            }
        }
        self.text_at(value, &style, origin, width, self.theme.text);
        if r.focused {
            let row = row_of(&rows, te.cursor);
            let (rs, re) = (rows[row].0 as usize, rows[row].1 as usize);
            self.text.advances(&value[rs..re], &style, &mut adv);
            let cx = (origin.x + x_at(&adv, te.cursor - rs)).round();
            let cy = origin.y + row as f64 * lh;
            self.draw.rect(Rect::new(Vec2::new(cx, cy), Vec2::new(cx + self.m.px(2.0), cy + lh)), self.theme.text);
        }
        self.draw.pop_clip();

        // ---- scrollbar ----
        let track = Rect::new(Vec2::new(rect.max.x - self.m.pad - bar_w, inner.min.y), Vec2::new(rect.max.x - self.m.pad, inner.max.y));
        if max_scroll > 0.0 {
            let ratio = inner.height() / content_h.max(1.0);
            let thumb_h = (track.height() * ratio).max(self.m.widget_h * 0.5).min(track.height());
            let travel = track.height() - thumb_h;
            let thumb = Rect::from_min_size(Vec2::new(track.min.x, track.min.y + travel * (scroll / max_scroll)), Vec2::new(bar_w, thumb_h));
            let tr = self.interact(id.with("thumb"), thumb, Sense::DRAG);
            if tr.dragging && travel > 0.0 && !tr.pressed {
                scroll = (scroll + tr.drag_delta.y / travel * max_scroll).clamp(0.0, max_scroll);
            }
            self.draw.rounded_rect(track, bar_w * 0.5, self.theme.shade(self.theme.field));
            let thumb = Rect::from_min_size(Vec2::new(track.min.x, track.min.y + travel * (scroll / max_scroll)), Vec2::new(bar_w, thumb_h));
            let base = if tr.held { self.theme.accent } else if tr.hovered { self.theme.hover(self.theme.widget) } else { self.theme.widget };
            self.draw.rounded_rect_gradient(thumb.shrink(self.m.border), bar_w * 0.5, self.theme.top(base), self.theme.bottom(base));
        }
        let mem = self.state.scroll(id);
        mem.offset = scroll;
        mem.content = content_h;
        self.focus_ring(id, rect);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_words() {
        let rows = [(0u32, 5u32), (6, 11), (11, 15)];
        assert_eq!(row_of(&rows, 0), 0);
        assert_eq!(row_of(&rows, 5), 0, "before the newline");
        assert_eq!(row_of(&rows, 6), 1);
        assert_eq!(row_of(&rows, 11), 2, "at a soft wrap the caret shows on the next row");
        assert_eq!(row_of(&rows, 99), 2);
        assert_eq!(word_at("hello big_world!", 7), (6, 15));
        assert_eq!(word_at("hello big_world!", 0), (0, 5));
        assert_eq!(word_at("hello big_world!", 15), (6, 15), "a caret at the end of a word still means that word");
        assert_eq!(word_at("a", 1), (0, 1));
    }
}
