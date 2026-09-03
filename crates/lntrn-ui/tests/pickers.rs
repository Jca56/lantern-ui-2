//! The colour picker and the tree view, headless.

use std::cell::{Cell, RefCell};

use lntrn_math::{Color, Vec2};
use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, Ui, WidgetId};

#[test]
fn color_picker_opens_and_drags() {
    let mut h = Harness::new(900.0, 700.0);
    let color = RefCell::new(Color::hex(0x336699));
    let changes = Cell::new(0);
    let f = |ui: &mut Ui| {
        if ui.color_picker("Accent", &mut color.borrow_mut()) {
            changes.set(changes.get() + 1);
        }
    };
    let id = WidgetId::ROOT.with("Accent");
    h.click_on(id, f);
    let sv = h.rect_of(id.with("sv")).expect("the picker is open");
    let hue = h.rect_of(id.with("hue")).unwrap();
    let alpha = h.rect_of(id.with("alpha")).unwrap();
    // Top-right of the square is the pure hue at full value.
    h.advance(1.0);
    h.click_at(Vec2::new(sv.max.x - 1.0, sv.min.y + 1.0), f);
    let c = *color.borrow();
    let [_, s, v] = c.to_hsv();
    assert!(s > 0.98 && v > 0.98, "saturated and bright: {c:?}");
    assert!(changes.get() >= 1);
    // Drag the hue to the middle: cyan-ish.
    h.advance(1.0);
    h.drag(Vec2::new(hue.min.x + 2.0, hue.center().y), Vec2::new(hue.center().x, hue.center().y), 4, f);
    let [hh, _, _] = color.borrow().to_hsv();
    assert!((hh - 0.5).abs() < 0.03, "hue at the middle of the bar: {hh}");
    // Alpha to the far left is transparent.
    h.advance(1.0);
    h.click_at(Vec2::new(alpha.min.x + 1.0, alpha.center().y), f);
    assert!(color.borrow().a < 0.02, "{:?}", color.borrow());
    // Escape closes it; the swatch stays.
    h.key(Key::Escape);
    h.settle(3, f);
    assert!(h.rect_of(id.with("sv")).is_none(), "closed");
    assert!(h.rect_of(id).is_some());
}

#[test]
fn color_picker_hex_field() {
    let mut h = Harness::new(900.0, 700.0);
    let color = RefCell::new(Color::BLACK);
    let f = |ui: &mut Ui| {
        ui.color_picker("Ink", &mut color.borrow_mut());
    };
    let id = WidgetId::ROOT.with("Ink");
    h.click_on(id, f);
    let hex = h.rect_of(id.with("hex")).expect("hex field");
    h.advance(1.0);
    h.click_at(hex.center(), f);
    h.key_with(Key::Char('a'), lntrn_ui::Modifiers::CTRL);
    h.type_text("#FF8800");
    h.frame(f);
    assert_eq!(color.borrow().to_hex_string(), "#FF8800");
    h.key(Key::Enter);
    h.settle(3, f);
    // Black keeps its hue memory: the square still shows the orange hue.
    assert!(h.rect_of(id.with("sv")).is_some(), "Enter leaves the picker open");
}

#[test]
fn tree_selects_and_folds() {
    let mut h = Harness::new(800.0, 600.0);
    let pick = Cell::new(0);
    let f = |ui: &mut Ui| {
        if ui.tree_node("Scene", pick.get() == 1, |ui| {
            if ui.tree_leaf("Cube", pick.get() == 2).clicked {
                pick.set(2);
            }
            if ui.tree_leaf("Light", pick.get() == 3).clicked {
                pick.set(3);
            }
        })
        .clicked
        {
            pick.set(1);
        }
    };
    h.frame(f);
    let scene = h.rect_of(WidgetId::ROOT.with("Scene")).unwrap();
    let cube = h.rect_of(WidgetId::ROOT.with("Scene").with("Cube")).expect("children start open");
    assert!(cube.min.x > scene.min.x, "children are indented");
    h.click_at(cube.center(), f);
    assert_eq!(pick.get(), 2);
    // Clicking the label selects the node; clicking its triangle folds it.
    h.advance(1.0);
    h.click_at(Vec2::new(scene.min.x + scene.height() * 2.0, scene.center().y), f);
    assert_eq!(pick.get(), 1);
    h.advance(1.0);
    h.click_at(Vec2::new(scene.min.x + scene.height() * 0.5, scene.center().y), f);
    assert_eq!(pick.get(), 1, "the triangle does not select");
    assert!(h.rect_of(WidgetId::ROOT.with("Scene").with("Cube")).is_none(), "folded");
    // Right arrow on the focused node opens it again.
    h.key(Key::Tab);
    h.settle(2, f);
    h.key(Key::ArrowRight);
    h.settle(3, f);
    assert!(h.rect_of(WidgetId::ROOT.with("Scene").with("Cube")).is_some(), "open again");
}
