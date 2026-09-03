//! What widgets remember between frames, keyed by widget id and kind: text
//! carets, scroll offsets, open flags, drag origins, eased values, spare
//! numbers, undo history and table layout. One id can hold every kind at
//! once.

use lntrn_math::Vec2;

use crate::id::WidgetId;
use crate::state::UiState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MemKind {
    Text,
    Scroll,
    Open,
    DragStart,
    Anim,
    Floats,
    History,
    Table,
}

/// Caret and selection of a text field, in byte offsets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdit {
    pub cursor: usize,
    pub anchor: usize,
    /// Horizontal scroll so the caret stays visible.
    pub scroll: f64,
    /// While editing a number field: the text being typed.
    pub buffer: Option<String>,
    /// A press landed on the selection: a drag from here takes the text
    /// out of the window; a release collapses the selection to a caret.
    pub drag_pending: bool,
}

impl TextEdit {
    pub fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }
    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }
}

/// Where a scroll area is and how big its content was last frame, both in
/// physical pixels: `x` across, `y` down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollMem {
    pub offset: Vec2,
    pub content: Vec2,
}

impl Default for ScrollMem {
    fn default() -> Self {
        Self { offset: Vec2::ZERO, content: Vec2::ZERO }
    }
}

/// A table's column widths (logical pixels; [`crate::FILL`] for a column
/// that takes what is left) and which column it is sorted by, ascending
/// or not.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableMem {
    pub widths: Vec<f64>,
    pub sort: Option<(usize, bool)>,
}

/// One remembered state of an editor: the text and its caret.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
}

/// Undo and redo stacks of a text editor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
    /// When the last undo step was recorded, so quick successive edits
    /// coalesce into one step.
    pub last_at: f64,
}

/// Undo steps kept per editor.
const HISTORY_CAP: usize = 200;

impl History {
    /// Remember `before` as an undo step unless one was recorded within
    /// `coalesce` seconds (then the earlier one stands). Clears redo.
    pub fn record(&mut self, before: Snapshot, now: f64, coalesce: f64) {
        self.redo.clear();
        if !self.undo.is_empty() && now - self.last_at < coalesce {
            return;
        }
        self.last_at = now;
        self.undo.push(before);
        if self.undo.len() > HISTORY_CAP {
            self.undo.remove(0);
        }
    }

    /// Step back: returns the state to restore, remembering `current` for redo.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let s = self.undo.pop()?;
        self.redo.push(current);
        self.last_at = f64::NEG_INFINITY;
        Some(s)
    }

    /// Step forward again.
    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let s = self.redo.pop()?;
        self.undo.push(current);
        self.last_at = f64::NEG_INFINITY;
        Some(s)
    }
}

/// An eased value on its way to a target (see [`crate::Ui::animate`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimMem {
    pub value: f64,
    /// When it was last stepped.
    pub time: f64,
}

pub(crate) enum Mem {
    Text(TextEdit),
    Scroll(ScrollMem),
    Open(bool),
    DragStart(f64),
    Anim(AnimMem),
    Floats([f64; 4]),
    History(Box<History>),
    Table(TableMem),
}

impl UiState {
    // ---- per-widget memory ----

    pub fn text_edit(&mut self, id: WidgetId) -> &mut TextEdit {
        match self.mem.entry((id, MemKind::Text)).or_insert_with(|| Mem::Text(TextEdit::default())) {
            Mem::Text(t) => t,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    pub fn scroll(&mut self, id: WidgetId) -> &mut ScrollMem {
        match self.mem.entry((id, MemKind::Scroll)).or_insert_with(|| Mem::Scroll(ScrollMem::default())) {
            Mem::Scroll(s) => s,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    pub fn open(&mut self, id: WidgetId) -> &mut bool {
        match self.mem.entry((id, MemKind::Open)).or_insert(Mem::Open(false)) {
            Mem::Open(b) => b,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    /// Like [`Self::open`], but a fresh slot starts as `default`.
    pub fn open_default(&mut self, id: WidgetId, default: bool) -> bool {
        match self.mem.entry((id, MemKind::Open)).or_insert(Mem::Open(default)) {
            Mem::Open(b) => *b,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    /// Value a drag started from (for number drags).
    pub fn drag_start(&mut self, id: WidgetId) -> &mut f64 {
        match self.mem.entry((id, MemKind::DragStart)).or_insert(Mem::DragStart(0.0)) {
            Mem::DragStart(v) => v,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    /// Eased-value slot; a fresh one starts at `init`.
    pub fn anim(&mut self, id: WidgetId, init: f64) -> &mut AnimMem {
        let now = self.now;
        match self.mem.entry((id, MemKind::Anim)).or_insert(Mem::Anim(AnimMem { value: init, time: now })) {
            Mem::Anim(a) => a,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    /// Four numbers a widget keeps between frames (a colour picker's hue
    /// and saturation, say); a fresh slot is `init`.
    pub fn floats(&mut self, id: WidgetId, init: [f64; 4]) -> &mut [f64; 4] {
        match self.mem.entry((id, MemKind::Floats)).or_insert(Mem::Floats(init)) {
            Mem::Floats(f) => f,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    /// The undo history of the editor `id`.
    pub fn history(&mut self, id: WidgetId) -> &mut History {
        match self.mem.entry((id, MemKind::History)).or_insert_with(|| Mem::History(Box::default())) {
            Mem::History(h) => h,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    /// The column widths and sort of the table `id`.
    pub fn table_mem(&mut self, id: WidgetId) -> &mut TableMem {
        match self.mem.entry((id, MemKind::Table)).or_insert_with(|| Mem::Table(TableMem::default())) {
            Mem::Table(t) => t,
            _ => unreachable!("slot kind is fixed by its key"),
        }
    }

    /// Drop every kind of memory `id` has.
    pub fn forget(&mut self, id: WidgetId) {
        self.mem.retain(|(k, _), _| *k != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_coalesces_and_steps() {
        let snap = |t: &str| Snapshot { text: t.to_owned(), cursor: t.len(), anchor: t.len() };
        let mut h = History::default();
        h.record(snap(""), 0.0, 0.8);
        h.record(snap("a"), 0.1, 0.8);
        h.record(snap("ab"), 0.2, 0.8);
        assert_eq!(h.undo.len(), 1, "quick typing is one step");
        h.record(snap("abc"), 2.0, 0.8);
        assert_eq!(h.undo.len(), 2);
        let back = h.undo(snap("abcd")).unwrap();
        assert_eq!(back.text, "abc");
        let back = h.undo(back).unwrap();
        assert_eq!(back.text, "");
        assert!(h.undo(back.clone()).is_none());
        let fwd = h.redo(back).unwrap();
        assert_eq!(fwd.text, "abc");
        h.record(snap("x"), 9.0, 0.8);
        assert!(h.redo.is_empty(), "a new edit drops the redo branch");
    }

    #[test]
    fn memory_slots() {
        let mut s = UiState::new();
        let id = WidgetId::ROOT.with("x");
        s.text_edit(id).cursor = 3;
        assert_eq!(s.text_edit(id).cursor, 3);
        *s.open(id) = true;
        *s.drag_start(id) = 4.5;
        assert!(*s.open(id));
        assert_eq!(s.text_edit(id).cursor, 3, "one id keeps every kind of memory at once");
        assert_eq!(*s.drag_start(id), 4.5);
        s.scroll(id).offset.y = 9.0;
        assert_eq!(s.scroll(id).offset.y, 9.0);
        s.table_mem(id).widths = vec![1.0, 2.0];
        s.table_mem(id).sort = Some((1, false));
        assert_eq!(s.table_mem(id).widths.len(), 2);
        s.forget(id);
        assert_eq!(s.scroll(id).offset.y, 0.0);
        assert_eq!(*s.drag_start(id), 0.0);
        assert_eq!(s.table_mem(id).sort, None);
    }
}
