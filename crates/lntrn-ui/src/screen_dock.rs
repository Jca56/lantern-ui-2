//! Moving an area to a new place in the tree: beside another area, or
//! along an edge of the window, spanning it. Where a dragged area would
//! land ([`Drop`]) is worked out from the pointer here too, so the shell
//! only draws the preview and applies the result.

use lntrn_math::{Rect, Vec2};

use crate::screen::{AreaId, Axis, Node, NodeId, Screen};

/// A side of an area or of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// The axis a split on this side has.
    pub fn axis(self) -> Axis {
        match self {
            Side::Left | Side::Right => Axis::Horizontal,
            Side::Top | Side::Bottom => Axis::Vertical,
        }
    }

    /// Whether this side comes first along its axis.
    fn leads(self) -> bool {
        matches!(self, Side::Left | Side::Top)
    }

    /// The part of `rect` on this side: `share` of it along the axis.
    fn part(self, r: Rect, share: f64) -> Rect {
        let (w, h) = (r.width() * share, r.height() * share);
        match self {
            Side::Left => Rect::new(r.min, Vec2::new(r.min.x + w, r.max.y)),
            Side::Right => Rect::new(Vec2::new(r.max.x - w, r.min.y), r.max),
            Side::Top => Rect::new(r.min, Vec2::new(r.max.x, r.min.y + h)),
            Side::Bottom => Rect::new(Vec2::new(r.min.x, r.max.y - h), r.max),
        }
    }
}

/// Where a dragged area would land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    /// Trade places with `target`; the tree keeps its shape.
    Swap(AreaId),
    /// Split `target` and take its `side`.
    Beside(AreaId, Side),
    /// Span the window along `side`.
    Edge(Side),
}

/// The outer part of an area that docks beside it, as a share of its
/// smaller dimension; inside that a drop swaps.
const BESIDE_BAND: f64 = 0.3;
/// The least and the most a moved area takes of the space it lands in.
const SHARE_MIN: f64 = 0.15;
const SHARE_MAX: f64 = 0.5;

impl<E: Copy + PartialEq, S: Default> Screen<E, S> {
    /// Put a new leaf for `area` beside what `node` holds, on `side`,
    /// with `share` of the space.
    fn insert_beside(&mut self, node: NodeId, area: AreaId, side: Side, share: f64) {
        let Some(old) = self.nodes[node].clone() else {
            return;
        };
        // Allocated while `node` is still taken, so its slot is not reused.
        let keep = self.alloc_node(old);
        let leaf = self.alloc_node(Node::Leaf(area));
        let share = share.clamp(0.1, 0.9);
        let (children, ratio) = if side.leads() { ([leaf, keep], share) } else { ([keep, leaf], 1.0 - share) };
        self.nodes[node] = Some(Node::Split { axis: side.axis(), ratio, children });
    }

    /// Move `area` beside `target`, taking `share` of its space on
    /// `side`. The tabs stay with the area. A maximized layout is put
    /// back so the result shows.
    pub fn move_beside(&mut self, area: AreaId, target: AreaId, side: Side, share: f64) -> bool {
        if area == target || self.area(target).is_none() || !self.unlink(area) {
            return false;
        }
        let node = self.leaf_of(target).unwrap_or(self.root);
        self.insert_beside(node, area, side, share);
        self.maximized = None;
        true
    }

    /// Move `area` to the window's `side`, spanning it, with `share` of
    /// the window. The only area stays where it is.
    pub fn move_to_edge(&mut self, area: AreaId, side: Side, share: f64) -> bool {
        if !self.unlink(area) {
            return false;
        }
        let root = self.root;
        self.insert_beside(root, area, side, share);
        self.maximized = None;
        true
    }

    /// Carry out a drop of `source`. Returns the area that now holds what
    /// was dragged (after a swap that is the target's id).
    pub fn apply_drop(&mut self, source: AreaId, drop: Drop, share: f64) -> Option<AreaId> {
        match drop {
            Drop::Swap(t) => self.swap(source, t).then_some(t),
            Drop::Beside(t, side) => self.move_beside(source, t, side, share).then_some(source),
            Drop::Edge(side) => self.move_to_edge(source, side, share).then_some(source),
        }
    }

    /// What dropping `source` at `p` would do, from the last layout:
    /// within `band` of the window's edge (or past it) the area spans
    /// that edge; over another area, the outer part docks beside it and
    /// the middle swaps. Nothing over `source` itself, with one area, or
    /// while an area is maximized.
    pub fn drop_at(&self, source: AreaId, p: Vec2, window: Rect, band: f64) -> Option<Drop> {
        if self.maximized.is_some() || self.area_count() < 2 {
            return None;
        }
        if let Some(side) = nearest_side(window, p, band) {
            return Some(Drop::Edge(side));
        }
        let target = self.area_at(p).filter(|&t| t != source)?;
        let rect = self.layout_of(target)?.rect;
        let band = rect.width().min(rect.height()) * BESIDE_BAND;
        Some(match nearest_side(rect, p, band) {
            Some(side) => Drop::Beside(target, side),
            None => Drop::Swap(target),
        })
    }

    /// The rect a drop would fill, for the preview.
    pub fn drop_rect(&self, drop: Drop, window: Rect, share: f64) -> Option<Rect> {
        Some(match drop {
            Drop::Swap(t) => self.layout_of(t)?.rect,
            Drop::Beside(t, side) => side.part(self.layout_of(t)?.rect, share),
            Drop::Edge(side) => side.part(window, share),
        })
    }

    /// How much of the space it lands in `source` should take: what it
    /// has now along the new axis, kept within reason, so a narrow panel
    /// stays narrow and a wide one takes half. A swap needs none.
    pub fn drop_share(&self, source: AreaId, drop: Drop, window: Rect) -> f64 {
        let Some(src) = self.layout_of(source) else {
            return SHARE_MAX;
        };
        let (side, dest) = match drop {
            Drop::Swap(_) => return SHARE_MAX,
            Drop::Beside(t, side) => match self.layout_of(t) {
                Some(l) => (side, l.rect),
                None => return SHARE_MAX,
            },
            Drop::Edge(side) => (side, window),
        };
        let have = extent(side.axis(), src.rect);
        let room = extent(side.axis(), dest).max(1.0);
        (have / room).clamp(SHARE_MIN, SHARE_MAX)
    }
}

fn extent(axis: Axis, r: Rect) -> f64 {
    match axis {
        Axis::Horizontal => r.width(),
        Axis::Vertical => r.height(),
    }
}

/// The side of `rect` that `p` is within `band` of (past it counts), the
/// nearest when several.
fn nearest_side(rect: Rect, p: Vec2, band: f64) -> Option<Side> {
    let sides = [(Side::Left, p.x - rect.min.x), (Side::Right, rect.max.x - p.x), (Side::Top, p.y - rect.min.y), (Side::Bottom, rect.max.y - p.y)];
    sides.into_iter().filter(|(_, d)| *d < band).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(s, _)| s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum K {
        Files,
        Code,
        Term,
    }

    fn read(text: &str) -> Screen<K> {
        Screen::from_description(text, |n| match n {
            "Files" => Some(K::Files),
            "Code" => Some(K::Code),
            "Term" => Some(K::Term),
            _ => None,
        })
        .unwrap()
    }

    fn text(s: &Screen<K>) -> String {
        s.describe(|k| format!("{k:?}"))
    }

    fn area(s: &Screen<K>, k: K) -> AreaId {
        s.area_ids().find(|&a| s.area(a).unwrap().editor() == k).unwrap()
    }

    /// Files top-left, code top-right, a terminal along the bottom.
    const IDE: &str = "(v 0.5500 (h 0.1600 [Files] [Code]) [Term])";

    fn win() -> Rect {
        Rect::from_xywh(0.0, 0.0, 2000.0, 1000.0)
    }

    #[test]
    fn to_an_edge_spans_the_window() {
        let mut s = read(IDE);
        let files = area(&s, K::Files);
        assert!(s.move_to_edge(files, Side::Left, 0.16));
        assert_eq!(text(&s), "(h 0.1600 [Files] (v 0.5500 [Code] [Term]))", "files now runs the full height");
        let term = area(&s, K::Term);
        assert!(s.move_to_edge(term, Side::Right, 0.4));
        assert_eq!(text(&s), "(h 0.6000 (h 0.1600 [Files] [Code]) [Term])", "the terminal stands on the right");
        assert!(s.move_to_edge(term, Side::Top, 0.3));
        assert_eq!(text(&s), "(v 0.3000 [Term] (h 0.1600 [Files] [Code]))");
        assert!(s.move_to_edge(term, Side::Bottom, 0.3));
        assert_eq!(text(&s), "(v 0.7000 (h 0.1600 [Files] [Code]) [Term])");
        // The tree stays sound: joining still works and the ids are reused.
        assert!(s.join(term));
        assert_eq!(text(&s), "(h 0.1600 [Files] [Code])");
        assert_eq!(s.area_count(), 2);
    }

    #[test]
    fn beside_splits_the_target() {
        let mut s = read(IDE);
        let (term, code) = (area(&s, K::Term), area(&s, K::Code));
        assert!(s.move_beside(term, code, Side::Right, 0.5));
        assert_eq!(text(&s), "(h 0.1600 [Files] (h 0.5000 [Code] [Term]))");
        let files = area(&s, K::Files);
        assert!(s.move_beside(files, term, Side::Bottom, 0.25));
        assert_eq!(text(&s), "(h 0.5000 [Code] (v 0.7500 [Term] [Files]))");
        // Onto itself, or a missing area: nothing.
        assert!(!s.move_beside(files, files, Side::Left, 0.5));
        assert!(!s.move_beside(files, 99, Side::Left, 0.5));
        assert_eq!(s.area_count(), 3);
        assert_eq!(text(&s), "(h 0.5000 [Code] (v 0.7500 [Term] [Files]))");
    }

    #[test]
    fn the_only_area_stays_put() {
        let mut s: Screen<K> = Screen::new(K::Code);
        assert!(!s.move_to_edge(0, Side::Left, 0.5));
        assert_eq!(text(&s), "[Code]");
        s.layout(win(), 40.0, 4.0);
        assert_eq!(s.drop_at(0, Vec2::new(5.0, 500.0), win(), 30.0), None);
    }

    #[test]
    fn drop_zones() {
        let mut s = read(IDE);
        s.layout(win(), 40.0, 4.0);
        let (files, code, term) = (area(&s, K::Files), area(&s, K::Code), area(&s, K::Term));
        let w = win();
        let band = 30.0;
        // The window's edges win over whatever area is under the pointer.
        assert_eq!(s.drop_at(term, Vec2::new(10.0, 300.0), w, band), Some(Drop::Edge(Side::Left)));
        assert_eq!(s.drop_at(term, Vec2::new(1990.0, 300.0), w, band), Some(Drop::Edge(Side::Right)));
        assert_eq!(s.drop_at(files, Vec2::new(1000.0, 995.0), w, band), Some(Drop::Edge(Side::Bottom)));
        assert_eq!(s.drop_at(term, Vec2::new(-20.0, 300.0), w, band), Some(Drop::Edge(Side::Left)), "past the edge still counts");
        // Inside another area: the outer part docks beside it, the middle swaps.
        let code_rect = s.layout_of(code).unwrap().rect;
        let c = code_rect.center();
        assert_eq!(s.drop_at(term, c, w, band), Some(Drop::Swap(code)));
        assert_eq!(s.drop_at(term, Vec2::new(code_rect.min.x + 50.0, c.y), w, band), Some(Drop::Beside(code, Side::Left)));
        assert_eq!(s.drop_at(term, Vec2::new(c.x, code_rect.max.y - 50.0), w, band), Some(Drop::Beside(code, Side::Bottom)));
        // Over itself: nothing.
        let t = s.layout_of(term).unwrap().rect.center();
        assert_eq!(s.drop_at(term, t, w, band), None);
        // The share keeps a panel's size where that makes sense.
        let share = s.drop_share(files, Drop::Edge(Side::Left), w);
        assert!((share - 0.16).abs() < 0.02, "files keeps its width: {share}");
        let ghost = s.drop_rect(Drop::Edge(Side::Left), w, share).unwrap();
        assert_eq!(ghost.min, w.min);
        assert!((ghost.height() - w.height()).abs() < 1e-9, "the preview runs the full height");
        assert_eq!(s.drop_share(term, Drop::Edge(Side::Right), w), SHARE_MAX, "a full-width area asks for half");
        assert_eq!(s.drop_share(term, Drop::Swap(code), w), SHARE_MAX);
        let beside = s.drop_rect(Drop::Beside(code, Side::Bottom), w, 0.25).unwrap();
        assert_eq!(beside.max, code_rect.max);
        assert!((beside.height() - code_rect.height() * 0.25).abs() < 1e-9);
        // Applying puts the tabs where the preview was and says where they went.
        assert_eq!(s.apply_drop(files, Drop::Edge(Side::Left), 0.2), Some(files));
        assert_eq!(text(&s), "(h 0.2000 [Files] (v 0.5500 [Code] [Term]))");
        assert_eq!(s.apply_drop(term, Drop::Swap(code), 0.5), Some(code));
        assert_eq!(text(&s), "(h 0.2000 [Files] (v 0.5500 [Term] [Code]))");
    }

    #[test]
    fn a_move_clears_maximize_and_reads_back() {
        let mut s = read(IDE);
        let term = area(&s, K::Term);
        s.toggle_maximize(term);
        s.layout(win(), 40.0, 4.0);
        assert_eq!(s.drop_at(term, Vec2::new(5.0, 500.0), win(), 30.0), None, "no docking while maximized");
        assert!(s.move_to_edge(term, Side::Right, 0.5));
        assert_eq!(s.maximized, None);
        let back = read(&text(&s));
        assert_eq!(text(&back), text(&s));
        assert_eq!(back.area_count(), 3);
    }
}
