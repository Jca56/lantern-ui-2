//! Single-line text editing: caret, selection, keyboard navigation, and
//! horizontal scrolling so the caret stays visible.

use std::borrow::Cow;

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::id::WidgetId;
use crate::state::{CursorIcon, DragPayload};
use crate::ui::{FILL, Response, Sense, Ui};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextResponse {
    /// The text changed this frame.
    pub changed: bool,
    /// Enter was pressed.
    pub committed: bool,
    /// Escape was pressed.
    pub cancelled: bool,
    pub focused: bool,
}

/// How a text field shows itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextOpts<'a> {
    /// Dim text shown while the field is empty.
    pub placeholder: &'a str,
    /// Dots instead of the text, for secrets.
    pub password: bool,
}

/// What a password field shows per character.
const MASK: &str = "•";

pub(crate) fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].chars().next_back().map_or(0, |c| i - c.len_utf8())
}

pub(crate) fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

/// Pen x at byte `b`, from a cluster-boundary advance table.
pub(crate) fn x_at(adv: &[(u32, f32)], b: usize) -> f64 {
    let b = b as u32;
    match adv.binary_search_by_key(&b, |&(o, _)| o) {
        Ok(i) => adv[i].1 as f64,
        Err(i) => {
            // Inside a ligature: interpolate between neighbours.
            let (b0, x0) = adv.get(i.wrapping_sub(1)).copied().unwrap_or((0, 0.0));
            let (b1, x1) = adv.get(i).copied().unwrap_or((b0, x0));
            if b1 == b0 {
                x0 as f64
            } else {
                x0 as f64 + (x1 - x0) as f64 * (b - b0) as f64 / (b1 - b0) as f64
            }
        }
    }
}

/// Byte offset whose pen x is closest to `x`.
pub(crate) fn byte_at_x(adv: &[(u32, f32)], x: f64) -> usize {
    adv.iter()
        .min_by(|a, b| (a.1 as f64 - x).abs().total_cmp(&(b.1 as f64 - x).abs()))
        .map_or(0, |a| a.0 as usize)
}

/// One step of editing input, in the order it arrived.
pub(crate) enum Edit {
    Key(crate::state::KeyPress),
    Text(String),
}

/// Take this frame's keys (except Tab, which the focus walk keeps) and
/// typed text, in arrival order.
pub(crate) fn take_edits(state: &mut crate::state::UiState) -> Vec<Edit> {
    let mut out: Vec<(u32, Edit)> = Vec::new();
    state.keys.retain(|k| {
        if k.key == Key::Tab {
            true
        } else {
            out.push((k.seq, Edit::Key(*k)));
            false
        }
    });
    for (seq, t) in state.text_input.drain(..) {
        out.push((seq, Edit::Text(t)));
    }
    out.sort_by_key(|(seq, _)| *seq);
    out.into_iter().map(|(_, e)| e).collect()
}

/// The pointer on a text widget, at byte `b` of `value`: place the caret
/// or extend the selection. A press on the selection waits instead: a
/// drag from it takes the text out of the window, a release collapses it
/// to a caret. `word` is what a double click at `b` selects.
pub(super) fn pointer_edit(ui: &mut Ui, id: WidgetId, r: &Response, value: &str, b: usize, word: impl Fn(usize) -> (usize, usize)) {
    let shift = ui.state.mods.shift();
    if r.pressed {
        let te = ui.state.text_edit(id);
        let (s0, s1) = te.selection();
        te.drag_pending = !shift && !r.double_clicked && s0 < b && b < s1;
    }
    if ui.state.text_edit(id).drag_pending {
        if r.released {
            let te = ui.state.text_edit(id);
            te.drag_pending = false;
            te.cursor = b;
            te.anchor = b;
        } else if ui.drag_out_starts(r) {
            let te = ui.state.text_edit(id);
            te.drag_pending = false;
            let (s0, s1) = te.selection();
            ui.state.start_drag_out(DragPayload::Text(value[s0..s1].to_owned()));
        }
    } else if !r.released {
        let te = ui.state.text_edit(id);
        te.cursor = b;
        if r.pressed && !shift {
            te.anchor = b;
        }
        if r.double_clicked {
            let (ws, we) = word(b);
            te.anchor = ws;
            te.cursor = we;
        }
    }
}

/// Insert typed text at the selection. Returns `true` if anything went in.
pub(crate) fn insert_typed(state: &mut crate::state::UiState, id: WidgetId, value: &mut String, typed: &str) -> bool {
    let typed: String = typed.chars().filter(|c| !c.is_control()).collect();
    if typed.is_empty() {
        return false;
    }
    let te = state.text_edit(id);
    let (s0, s1) = te.selection();
    value.replace_range(s0..s1, &typed);
    te.cursor = s0 + typed.len();
    te.anchor = te.cursor;
    true
}

/// The text as shown while an input method composes: the preedit spliced
/// in at the caret. Returns the shown text, where the caret is in it, and
/// the preedit's byte range in it.
pub(crate) fn with_preedit<'a>(value: &'a str, cursor: usize, preedit: Option<&(String, Option<(usize, usize)>)>) -> (Cow<'a, str>, usize, Option<(usize, usize)>) {
    let cursor = cursor.min(value.len());
    match preedit {
        Some((t, c)) if !t.is_empty() => {
            let mut s = String::with_capacity(value.len() + t.len());
            s.push_str(&value[..cursor]);
            s.push_str(t);
            s.push_str(&value[cursor..]);
            let caret = cursor + c.map_or(t.len(), |(a, _)| a.min(t.len()));
            (Cow::Owned(s), caret, Some((cursor, cursor + t.len())))
        }
        _ => (Cow::Borrowed(value), cursor, None),
    }
}

/// Ctrl+C / Ctrl+X / Ctrl+V on the text `id` edits. Returns `true` when
/// the text changed. `multiline` keeps newlines in pasted text.
pub(crate) fn clipboard_key(state: &mut crate::state::UiState, id: WidgetId, key: Key, value: &mut String, multiline: bool) -> bool {
    let (s0, s1) = state.text_edit(id).selection();
    match key {
        Key::Char('c') => {
            if s1 > s0 {
                state.set_clipboard(&value[s0..s1]);
            }
            false
        }
        Key::Char('x') => {
            if s1 > s0 {
                state.set_clipboard(&value[s0..s1]);
                value.replace_range(s0..s1, "");
                let te = state.text_edit(id);
                te.cursor = s0;
                te.anchor = s0;
                return true;
            }
            false
        }
        _ => {
            let pasted: String = state.clipboard.chars().filter(|c| !c.is_control() || (multiline && *c == '\n')).collect();
            if pasted.is_empty() {
                return false;
            }
            value.replace_range(s0..s1, &pasted);
            let te = state.text_edit(id);
            te.cursor = s0 + pasted.len();
            te.anchor = te.cursor;
            true
        }
    }
}

impl Ui<'_> {
    /// Editable single-line text. Click to focus, drag to select.
    pub fn text_field(&mut self, label: &str, value: &mut String) -> TextResponse {
        self.text_field_with(label, value, TextOpts::default())
    }

    /// A text field that shows `placeholder`, dim, while it is empty.
    pub fn text_field_hint(&mut self, label: &str, value: &mut String, placeholder: &str) -> TextResponse {
        self.text_field_with(label, value, TextOpts { placeholder, password: false })
    }

    /// A text field that shows dots instead of what is typed.
    pub fn password_field(&mut self, label: &str, value: &mut String, placeholder: &str) -> TextResponse {
        self.text_field_with(label, value, TextOpts { placeholder, password: true })
    }

    pub fn text_field_with(&mut self, label: &str, value: &mut String, opts: TextOpts) -> TextResponse {
        let id = self.id(label);
        let rect = self.alloc(Vec2::new(FILL, self.m.widget_h));
        self.text_edit_core_with(id, rect, value, opts)
    }

    /// The editor behind text fields and number typing. Draws into `rect`.
    pub fn text_edit_core(&mut self, id: WidgetId, rect: Rect, value: &mut String) -> TextResponse {
        self.text_edit_core_with(id, rect, value, TextOpts::default())
    }

    /// Pen positions per character of `value` as drawn: of the dots when
    /// `password`, so carets and clicks land on the right character.
    fn text_advances(&mut self, value: &str, password: bool, style: &lntrn_text::TextStyle, adv: &mut Vec<(u32, f32)>) {
        if !password {
            self.text.advances(value, style, adv);
            return;
        }
        let n = value.chars().count();
        let mut masked = Vec::new();
        self.text.advances(&MASK.repeat(n), style, &mut masked);
        adv.clear();
        adv.extend(value.char_indices().enumerate().map(|(i, (b, _))| (b as u32, x_at(&masked, i * MASK.len()) as f32)));
        adv.push((value.len() as u32, x_at(&masked, n * MASK.len()) as f32));
    }

    /// [`Self::text_edit_core`] with a placeholder or password dots.
    pub fn text_edit_core_with(&mut self, id: WidgetId, rect: Rect, value: &mut String, opts: TextOpts) -> TextResponse {
        let r = self.interact(id, rect, Sense::FOCUS);
        self.focusable(id, rect);
        if r.hovered {
            self.state.cursor_icon = CursorIcon::Text;
        }
        let style = self.text_style();
        let inner = Rect::new(Vec2::new(rect.min.x + self.m.pad, rect.min.y), Vec2::new(rect.max.x - self.m.pad, rect.max.y));
        let mut adv = Vec::new();
        self.text_advances(value, opts.password, &style, &mut adv);

        let mut out = TextResponse { focused: r.focused, ..TextResponse::default() };
        let mut scroll = self.state.text_edit(id).scroll;

        if r.focused {
            if r.pressed || r.dragging || r.released {
                let b = byte_at_x(&adv, self.state.pointer.x - inner.min.x + scroll);
                pointer_edit(self, id, &r, value, b, |_| (0, value.len()));
            }
            // Keyboard and typed text, in the order they arrived.
            for edit in take_edits(self.state) {
                let k = match edit {
                    Edit::Text(t) => {
                        out.changed |= insert_typed(self.state, id, value, &t);
                        continue;
                    }
                    Edit::Key(k) => k,
                };
                if k.mods.ctrl() && matches!(k.key, Key::Char('c' | 'x' | 'v')) {
                    out.changed |= clipboard_key(self.state, id, k.key, value, false);
                    continue;
                }
                let te = self.state.text_edit(id);
                let (s0, s1) = te.selection();
                match k.key {
                    Key::ArrowLeft => {
                        te.cursor = if te.has_selection() && !k.mods.shift() { s0 } else { prev_boundary(value, te.cursor) };
                        if k.mods.ctrl() {
                            te.cursor = 0;
                        }
                        if !k.mods.shift() {
                            te.anchor = te.cursor;
                        }
                    }
                    Key::ArrowRight => {
                        te.cursor = if te.has_selection() && !k.mods.shift() { s1 } else { next_boundary(value, te.cursor) };
                        if k.mods.ctrl() {
                            te.cursor = value.len();
                        }
                        if !k.mods.shift() {
                            te.anchor = te.cursor;
                        }
                    }
                    Key::Home => {
                        te.cursor = 0;
                        if !k.mods.shift() {
                            te.anchor = 0;
                        }
                    }
                    Key::End => {
                        te.cursor = value.len();
                        if !k.mods.shift() {
                            te.anchor = value.len();
                        }
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
                    Key::Enter => out.committed = true,
                    Key::Escape => out.cancelled = true,
                    Key::Char('a') if k.mods.ctrl() => {
                        te.anchor = 0;
                        te.cursor = value.len();
                    }
                    _ => {}
                }
            }
            if out.changed {
                self.text_advances(value, opts.password, &style, &mut adv);
            }
            let te = self.state.text_edit(id);
            te.cursor = te.cursor.min(value.len());
            te.anchor = te.anchor.min(value.len());
        }

        // While an input method composes, its text shows at the caret.
        let te = self.state.text_edit(id).clone();
        let (shown, caret, pre) = with_preedit(value, te.cursor, if r.focused && !opts.password { self.state.ime_preedit.as_ref() } else { None });
        if pre.is_some() {
            self.text.advances(&shown, &style, &mut adv);
        }
        if r.focused {
            // Keep the caret in view.
            let cx = x_at(&adv, caret);
            let w = inner.width().max(1.0);
            if cx - scroll > w - 2.0 {
                scroll = cx - w + 2.0;
            }
            if cx - scroll < 0.0 {
                scroll = cx;
            }
            scroll = scroll.max(0.0);
            self.state.text_edit(id).scroll = scroll;
        }

        // Draw.
        let well = if r.focused || r.hovered { self.theme.hover(self.theme.field) } else { self.theme.field };
        self.recessed(rect, well);
        if r.focused {
            self.outline(rect, self.m.border, self.theme.focus);
        }
        let clip = inner.intersection(&self.clip());
        self.draw.push_clip(clip);
        let lh = style.line_height() as f64;
        let ty = (rect.center().y - lh * 0.5).round();
        let origin = Vec2::new(inner.min.x - scroll, ty);
        if r.focused && pre.is_none() && te.has_selection() {
            let (s0, s1) = te.selection();
            let x0 = origin.x + x_at(&adv, s0);
            let x1 = origin.x + x_at(&adv, s1);
            self.draw.rect(Rect::new(Vec2::new(x0, ty), Vec2::new(x1, ty + lh)), self.theme.selection);
        }
        if opts.password {
            let dots = MASK.repeat(value.chars().count());
            self.text_at(&dots, &style, origin, 1.0e6, self.theme.text);
        } else {
            self.text_at(&shown, &style, origin, 1.0e6, self.theme.text);
        }
        if value.is_empty() && pre.is_none() && !opts.placeholder.is_empty() {
            self.text_at(opts.placeholder, &style, origin, 1.0e6, self.theme.text_dim);
        }
        if let Some((p0, p1)) = pre {
            // The composition, underlined until it commits.
            let (x0, x1) = (origin.x + x_at(&adv, p0), origin.x + x_at(&adv, p1));
            let uy = ty + lh - self.m.px(3.0);
            self.draw.rect(Rect::new(Vec2::new(x0, uy), Vec2::new(x1.max(x0 + 1.0), uy + self.m.px(2.0))), self.theme.accent);
        }
        if r.focused {
            let cx = (origin.x + x_at(&adv, caret)).round();
            let cw = self.m.px(2.0);
            let caret_rect = Rect::new(Vec2::new(cx, ty), Vec2::new(cx + cw, ty + lh));
            self.draw.rect(caret_rect, self.theme.text);
            self.state.ime_rect = Some(caret_rect);
        }
        self.draw.pop_clip();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries() {
        let s = "aé🦊";
        assert_eq!(next_boundary(s, 0), 1);
        assert_eq!(next_boundary(s, 1), 3);
        assert_eq!(next_boundary(s, 3), 7);
        assert_eq!(next_boundary(s, 7), 7);
        assert_eq!(prev_boundary(s, 7), 3);
        assert_eq!(prev_boundary(s, 0), 0);
    }

    #[test]
    fn advance_lookup() {
        let adv = [(0u32, 0.0f32), (1, 10.0), (3, 30.0)];
        assert_eq!(x_at(&adv, 1), 10.0);
        assert_eq!(x_at(&adv, 2), 20.0, "interpolates inside a cluster");
        assert_eq!(x_at(&adv, 9), 30.0, "clamps past the end");
        assert_eq!(byte_at_x(&adv, 12.0), 1);
        assert_eq!(byte_at_x(&adv, 26.0), 3);
        assert_eq!(byte_at_x(&adv, -5.0), 0);
    }

    #[test]
    fn preedit_splices_in_at_the_caret() {
        let none = with_preedit("abc", 1, None);
        assert_eq!((none.0.as_ref(), none.1, none.2), ("abc", 1, None));
        assert!(matches!(none.0, Cow::Borrowed(_)), "no composition: nothing allocated");
        let empty = with_preedit("abc", 1, Some(&(String::new(), None)));
        assert_eq!(empty.2, None, "an empty preedit is no preedit");
        let (shown, caret, pre) = with_preedit("abc", 1, Some(&("ni".to_owned(), Some((1, 1)))));
        assert_eq!(shown, "anibc");
        assert_eq!(caret, 2, "one byte into the composition");
        assert_eq!(pre, Some((1, 3)));
        let (_, caret, _) = with_preedit("abc", 1, Some(&("ni".to_owned(), None)));
        assert_eq!(caret, 3, "no composition caret: after it");
        let (shown, caret, pre) = with_preedit("ab", 9, Some(&("x".to_owned(), Some((5, 5)))));
        assert_eq!((shown.as_ref(), caret, pre), ("abx", 3, Some((2, 3))), "offsets clamp");
    }
}
