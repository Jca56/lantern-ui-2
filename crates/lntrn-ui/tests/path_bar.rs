//! The path bar: crumbs that go somewhere, an end that turns into a field.

use std::cell::RefCell;
use std::path::PathBuf;

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, Ui, WidgetId};

#[test]
fn crumbs_go_and_the_end_types() {
    let mut h = Harness::new(900.0, 300.0);
    let path = RefCell::new(PathBuf::from("/home/alva/Projects/demo"));
    let text = RefCell::new(String::new());
    let typed = RefCell::new(None::<String>);
    let f = |ui: &mut Ui| {
        let p = path.borrow().clone();
        let r = ui.path_bar("where", &p, &mut text.borrow_mut());
        if let Some(g) = r.go {
            *path.borrow_mut() = g;
        }
        if let Some(t) = r.typed {
            *typed.borrow_mut() = Some(t);
        }
    };
    let id = WidgetId::ROOT.with("where");
    h.frame(f);
    let bar = h.rect_of(id).expect("the bar");
    // Crumbs: "/", home, alva, Projects, demo.
    let alva = h.rect_of(id.with("crumb").with_index(2)).expect("a crumb per segment");
    let demo = h.rect_of(id.with("crumb").with_index(4)).expect("the last crumb");
    assert!(demo.min.x > alva.max.x, "left to right");
    h.click_at(alva.center(), f);
    assert_eq!(*path.borrow(), PathBuf::from("/home/alva"), "a crumb click climbs");
    assert!(h.rect_of(id.with("edit")).is_none(), "still crumbs");
    // A click past the last crumb turns the bar into a field holding the path.
    h.advance(1.0);
    h.click_at(Vec2::new(bar.max.x - 4.0, bar.center().y), f);
    assert!(h.rect_of(id.with("edit")).is_some(), "editing");
    assert_eq!(*text.borrow(), "/home/alva");
    h.type_text("/tmp");
    h.key(Key::Enter);
    h.settle(3, f);
    assert_eq!(typed.borrow().as_deref(), Some("/home/alva/tmp"), "Enter hands the typed path back");
    assert!(h.rect_of(id.with("edit")).is_none(), "back to crumbs");
    // Escape gives up without a word.
    h.advance(1.0);
    let bar = h.rect_of(id).unwrap();
    h.click_at(Vec2::new(bar.max.x - 4.0, bar.center().y), f);
    *typed.borrow_mut() = None;
    h.key(Key::Escape);
    h.settle(3, f);
    assert!(typed.borrow().is_none() && h.rect_of(id.with("edit")).is_none());
}

#[test]
fn long_paths_fold_from_the_left() {
    let mut h = Harness::new(320.0, 200.0);
    let path = PathBuf::from("/one/two/three/four/five/six/seven/eight");
    let text = RefCell::new(String::new());
    let went = RefCell::new(None::<PathBuf>);
    let f = |ui: &mut Ui| {
        if let Some(g) = ui.path_bar("where", &path, &mut text.borrow_mut()).go {
            *went.borrow_mut() = Some(g);
        }
    };
    let id = WidgetId::ROOT.with("where");
    h.frame(f);
    let first = h.rect_of(id.with("crumb").with_index(0)).expect("an ellipsis crumb");
    let bar = h.rect_of(id).unwrap();
    assert!(first.min.x >= bar.min.x, "nothing hangs off the left");
    let mut n = 0;
    while h.rect_of(id.with("crumb").with_index(n)).is_some() {
        let r = h.rect_of(id.with("crumb").with_index(n)).unwrap();
        assert!(r.max.x <= bar.max.x + 0.5, "crumb {n} fits: {r:?} in {bar:?}");
        n += 1;
    }
    assert!(n < 9 && n >= 2, "some crumbs fell off: {n} shown");
    h.click_at(first.center(), f);
    let g = went.borrow().clone().expect("the ellipsis goes somewhere");
    assert!(path.starts_with(&g) && g != path, "to a folder above: {g:?}");
}
