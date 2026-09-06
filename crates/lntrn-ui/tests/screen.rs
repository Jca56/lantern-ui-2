//! The area tree on its own: layout, splits, joins, drags, maximize, swaps
//! and tabs.

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{Axis, Screen};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum K {
    Empty,
    Prefs,
    Gallery,
}

fn win() -> Rect {
    Rect::from_xywh(0.0, 0.0, 1000.0, 600.0)
}

#[test]
fn single_area_fills_window() {
    let mut s: Screen<K> = Screen::new(K::Empty);
    s.layout(win(), 45.0, 5.0);
    assert_eq!(s.layouts().len(), 1);
    let l = s.layouts()[0];
    assert_eq!(l.rect, win());
    assert_eq!(l.header.height(), 45.0);
    assert_eq!(l.body.min.y, 45.0);
    assert!(s.separators().is_empty());
    assert!(!s.join(0), "cannot remove the last area");
    assert_eq!(s.active_editor(), Some(K::Empty));
}

#[test]
fn split_join_and_drag() {
    let mut s: Screen<K, u32> = Screen::new(K::Empty);
    let right = s.split(0, Axis::Horizontal, 0.6, K::Prefs).unwrap();
    assert_eq!(s.area_count(), 2);
    *s.area_mut(right).unwrap().state_mut() = 7;
    s.layout(win(), 45.0, 10.0);
    let l0 = *s.layout_of(0).unwrap();
    let l1 = *s.layout_of(right).unwrap();
    assert_eq!(l0.rect.width(), 594.0, "60% of (1000 - gap)");
    assert_eq!(l1.rect.min.x, 604.0);
    assert_eq!(l1.rect.max.x, 1000.0);
    assert_eq!(s.separators().len(), 1);
    assert_eq!(s.separators()[0].gap, Rect::from_xywh(594.0, 0.0, 10.0, 600.0));
    assert_eq!(s.area_at(Vec2::new(700.0, 10.0)), Some(right));
    assert_eq!(s.separator_at(Vec2::new(598.0, 300.0)), Some(0));
    assert_eq!(s.separator_at(Vec2::new(100.0, 300.0)), None);
    assert_eq!(s.target(K::Prefs), Some(right), "the first area hosting the editor");
    assert_eq!(s.target(K::Gallery), None);

    s.drag_separator(0, Vec2::new(300.0, 0.0), 90.0);
    s.layout(win(), 45.0, 10.0);
    assert!((s.layout_of(0).unwrap().rect.width() - 297.0).abs() <= 1.0);
    // Clamped to the minimum.
    s.drag_separator(0, Vec2::new(0.0, 0.0), 90.0);
    s.layout(win(), 45.0, 10.0);
    assert!(s.layout_of(0).unwrap().rect.width() >= 80.0);

    // Nested split of the right area, then join it away again.
    let bottom = s.split(right, Axis::Vertical, 0.5, K::Gallery).unwrap();
    assert_eq!(*s.area(bottom).unwrap().state(), 0, "a new area starts with default state");
    assert_eq!(*s.area(right).unwrap().state(), 7, "the split area keeps its state");
    s.layout(win(), 45.0, 10.0);
    assert_eq!(s.layouts().len(), 3);
    assert_eq!(s.separators().len(), 2);
    s.active = Some(bottom);
    assert!(s.join(bottom));
    assert_eq!(s.area_count(), 2);
    assert_ne!(s.active, Some(bottom));
    assert_eq!(s.area_ids().collect::<Vec<_>>(), vec![0, right]);
    s.layout(win(), 45.0, 10.0);
    assert_eq!(s.layouts().len(), 2);
    // Maximize hides the tree without changing it.
    s.toggle_maximize(right);
    s.layout(win(), 45.0, 10.0);
    assert_eq!(s.layouts().len(), 1);
    assert_eq!(s.layouts()[0].rect, win());
    assert_eq!(s.layouts()[0].area, right);
    assert!(s.separators().is_empty());
    assert_eq!(s.active, Some(right));
    s.toggle_maximize(right);
    s.layout(win(), 45.0, 10.0);
    assert_eq!(s.layouts().len(), 2, "restored");
    assert!(s.swap(0, right));
    assert_eq!(s.area(0).unwrap().editor(), K::Prefs);
    assert_eq!(*s.area(0).unwrap().state(), 7, "state travels with the editor");
    assert_eq!(s.area(right).unwrap().editor(), K::Empty);
    assert!(!s.swap(0, 0) && !s.swap(0, 99));
    assert!(s.swap(right, 0));
    s.toggle_maximize(right);
    assert!(s.join(right), "joining the maximized area away clears it");
    assert_eq!(s.maximized, None);
    s.layout(win(), 45.0, 10.0);
    assert_eq!(s.layouts()[0].rect, win(), "sibling took the whole window back");
    assert!(s.separators().is_empty());
}

#[test]
fn tabs_stack_in_an_area() {
    let mut s: Screen<K, u32> = Screen::new(K::Gallery);
    *s.area_mut(0).unwrap().state_mut() = 1;
    assert!(!s.close_tab(0), "the last tab stays");
    assert_eq!(s.add_tab(0, K::Prefs), Some(1));
    assert_eq!(s.area(0).unwrap().editor(), K::Prefs, "a new tab shows");
    assert_eq!(*s.area(0).unwrap().state(), 0, "with state of its own");
    *s.area_mut(0).unwrap().state_mut() = 2;
    s.add_tab(0, K::Empty);
    assert_eq!(s.area(0).unwrap().tabs.len(), 3);
    s.cycle_tab(0, 1);
    assert_eq!(s.area(0).unwrap().editor(), K::Gallery, "wraps forward");
    assert_eq!(*s.area(0).unwrap().state(), 1, "the first tab kept its state");
    s.cycle_tab(0, -1);
    assert_eq!(s.area(0).unwrap().editor(), K::Empty, "wraps back");
    s.select_tab(0, 1);
    s.area_mut(0).unwrap().set_editor(K::Gallery);
    assert_eq!(s.area(0).unwrap().tabs[1].editor, K::Gallery, "the showing tab changed editor");
    assert!(s.close_tab(0));
    assert_eq!(s.area(0).unwrap().tabs.len(), 2);
    assert_eq!(s.area(0).unwrap().editor(), K::Empty, "the next tab along shows");
    s.select_tab(0, 99);
    assert_eq!(s.area(0).unwrap().current, 1, "clamped");
    assert_eq!(s.target(K::Gallery), None, "target looks at showing tabs only");
    assert_eq!(s.target(K::Empty), Some(0));
}

#[test]
fn headerless_area_gives_the_body_the_whole_area() {
    let mut s: Screen<K> = Screen::new(K::Empty);
    let right = s.split(0, Axis::Horizontal, 0.2, K::Gallery).unwrap();
    s.layout_with(win(), 45.0, 10.0, |a| a != right);
    let l0 = *s.layout_of(0).unwrap();
    let l1 = *s.layout_of(right).unwrap();
    assert_eq!(l0.header.height(), 45.0, "the left area keeps its header");
    assert_eq!(l1.header.height(), 0.0, "the right one has none");
    assert_eq!(l1.body, l1.rect, "so its body is the whole area");
    // Maximized, the same rule applies.
    s.toggle_maximize(right);
    s.layout_with(win(), 45.0, 10.0, |a| a != right);
    assert_eq!(s.layout_of(right).unwrap().body, win());
}
