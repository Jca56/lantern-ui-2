//! An undo stack of snapshots. A document built on [`crate::ChunkedVec`]
//! clones in O(chunks), so keeping one snapshot per edit is cheap; this is
//! what makes Ctrl+Z work the same in every Lantern app.
//!
//! ```
//! use lntrn_core::Undo;
//! let mut doc = vec![1, 2];
//! let mut undo: Undo<Vec<i32>> = Undo::default();
//! undo.push(doc.clone());
//! doc.push(3);
//! doc = undo.undo(doc).unwrap();
//! assert_eq!(doc, vec![1, 2]);
//! ```

/// Undo and redo stacks of whole snapshots.
#[derive(Clone, Debug)]
pub struct Undo<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    cap: usize,
    /// When the last step was recorded, so quick successive edits coalesce.
    last_at: f64,
}

impl<T> Default for Undo<T> {
    fn default() -> Self {
        Self::new(200)
    }
}

impl<T> Undo<T> {
    /// Keep at most `cap` steps; the oldest fall off.
    pub fn new(cap: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), cap: cap.max(1), last_at: f64::NEG_INFINITY }
    }

    /// Remember `before` as the state to go back to. Clears redo.
    pub fn push(&mut self, before: T) {
        self.record(before, f64::NEG_INFINITY, 0.0);
    }

    /// Like [`Self::push`], but a step recorded within `coalesce` seconds
    /// of the last one is folded into it (the earlier snapshot stands), so
    /// a drag or fast typing is one undo, not a hundred.
    pub fn record(&mut self, before: T, now: f64, coalesce: f64) {
        self.redo.clear();
        if !self.undo.is_empty() && now - self.last_at < coalesce {
            return;
        }
        self.last_at = now;
        self.undo.push(before);
        if self.undo.len() > self.cap {
            self.undo.remove(0);
        }
    }

    /// Step back: the state to restore, with `current` kept for redo.
    pub fn undo(&mut self, current: T) -> Option<T> {
        let s = self.undo.pop()?;
        self.redo.push(current);
        self.last_at = f64::NEG_INFINITY;
        Some(s)
    }

    /// Step forward again.
    pub fn redo(&mut self, current: T) -> Option<T> {
        let s = self.redo.pop()?;
        self.undo.push(current);
        self.last_at = f64::NEG_INFINITY;
        Some(s)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Steps that can be undone.
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// Forget everything (a new document).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_at = f64::NEG_INFINITY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_back_and_forth() {
        let mut u: Undo<i32> = Undo::new(3);
        assert!(!u.can_undo() && !u.can_redo() && u.is_empty());
        u.push(1);
        u.push(2);
        u.push(3);
        u.push(4);
        assert_eq!(u.len(), 3, "capped: the oldest fell off");
        assert_eq!(u.undo(5), Some(4));
        assert_eq!(u.undo(4), Some(3));
        assert!(u.can_redo());
        assert_eq!(u.redo(3), Some(4));
        assert_eq!(u.redo(4), Some(5));
        assert_eq!(u.redo(5), None);
        u.undo(5);
        u.push(9);
        assert!(!u.can_redo(), "a new edit drops the redo branch");
        u.clear();
        assert!(u.is_empty() && !u.can_redo());
    }

    #[test]
    fn quick_edits_coalesce() {
        let mut u: Undo<&str> = Undo::default();
        u.record("a", 0.0, 0.5);
        u.record("b", 0.1, 0.5);
        u.record("c", 0.2, 0.5);
        assert_eq!(u.len(), 1, "one step for the burst");
        u.record("d", 2.0, 0.5);
        assert_eq!(u.len(), 2);
        assert_eq!(u.undo("e"), Some("d"));
        assert_eq!(u.undo("d"), Some("a"), "the burst restores its first snapshot");
        // After an undo, the next edit is a fresh step even if quick.
        u.record("x", 2.1, 0.5);
        assert_eq!(u.len(), 1);
    }
}
