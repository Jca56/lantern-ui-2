//! The small widgets, headless: placeholder and password fields, the
//! combo box, radio groups, spinners, range and vertical sliders, the pad.

use std::cell::{Cell, RefCell};

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, Modifiers, Ui, WidgetId};

#[test]
fn placeholder_and_password_fields_edit_the_real_text() {
    let mut h = Harness::new(800.0, 400.0);
    let secret = RefCell::new(String::new());
    let plain = RefCell::new(String::new());
    let f = |ui: &mut Ui| {
        ui.text_field_hint("search", &mut plain.borrow_mut(), "Search…");
        ui.password_field("pw", &mut secret.borrow_mut(), "Password");
    };
    let empty = h.frame(f).vertices;
    h.click_on(WidgetId::ROOT.with("pw"), f);
    h.type_text("héllo");
    h.settle(2, f);
    assert_eq!(*secret.borrow(), "héllo", "the value is the real text");
    // Backspace removes one character, not one dot's worth of bytes.
    h.key(Key::Backspace);
    h.settle(2, f);
    assert_eq!(*secret.borrow(), "héll");
    h.key(Key::Home);
    h.key(Key::Delete);
    h.settle(2, f);
    assert_eq!(*secret.borrow(), "éll");
    let caret = h.state.ime_rect.unwrap();
    h.key(Key::End);
    h.settle(2, f);
    assert!(h.state.ime_rect.unwrap().min.x > caret.min.x, "the caret walks along the dots");
    // A placeholder draws while empty and goes away once there is text.
    h.click_on(WidgetId::ROOT.with("search"), f);
    let with_placeholder = h.settle(2, f).vertices;
    h.type_text("x");
    h.settle(2, f);
    assert_eq!(*plain.borrow(), "x");
    assert!(with_placeholder > empty - 20, "placeholders draw glyphs");
}

#[test]
fn combo_filters_and_picks() {
    let mut h = Harness::new(800.0, 500.0);
    let value = RefCell::new(String::new());
    let fonts = ["Inter", "Fira Code", "Noto Sans"];
    let f = |ui: &mut Ui| {
        ui.combo("font", &mut value.borrow_mut(), &fonts, "Font");
    };
    let id = WidgetId::ROOT.with("font");
    h.frame(f);
    let button = h.rect_of(id.with("open")).expect("the open button");
    h.click_at(button.center(), f);
    assert!(h.rect_of(id.with("item").with_index(2)).is_some(), "all three listed");
    // Typing narrows the list; the field keeps focus.
    let field = h.rect_of(id).unwrap();
    h.click_at(Vec2::new(field.min.x + 20.0, field.center().y), f);
    h.key(Key::ArrowDown);
    h.settle(2, f);
    assert!(h.rect_of(id.with("item").with_index(0)).is_some(), "Down opens the list");
    h.type_text("no");
    h.settle(2, f);
    assert_eq!(*value.borrow(), "no");
    assert!(h.rect_of(id.with("item").with_index(0)).is_some());
    assert!(h.rect_of(id.with("item").with_index(1)).is_none(), "only Noto Sans matches");
    let pick = h.rect_of(id.with("item").with_index(0)).unwrap();
    h.click_at(pick.center(), f);
    assert_eq!(*value.borrow(), "Noto Sans");
    assert!(h.rect_of(id.with("item").with_index(0)).is_none(), "closed after the pick");
}

#[test]
fn radio_clicks_and_arrows() {
    let mut h = Harness::new(800.0, 400.0);
    let pick = Cell::new(0usize);
    let f = |ui: &mut Ui| {
        let mut p = pick.get();
        ui.radio("mode", &mut p, &["Object", "Edit", "Sculpt"]);
        pick.set(p);
    };
    let id = WidgetId::ROOT.with("mode");
    h.frame(f);
    let third = h.rect_of(id.with_index(2)).unwrap();
    let first = h.rect_of(id.with_index(0)).unwrap();
    assert!(third.min.y > first.max.y - 1.0, "a column");
    h.click_at(third.center(), f);
    assert_eq!(pick.get(), 2);
    assert_eq!(h.state.focus, Some(id), "the group took focus");
    h.key(Key::ArrowUp);
    h.settle(2, f);
    assert_eq!(pick.get(), 1, "Up is the one before");
    h.key(Key::Home);
    h.settle(2, f);
    assert_eq!(pick.get(), 0);
}

#[test]
fn spinner_buttons_step_and_the_middle_types() {
    let mut h = Harness::new(800.0, 400.0);
    let n = Cell::new(5i64);
    let f = |ui: &mut Ui| {
        let mut v = n.get();
        ui.spinner_int("copies", &mut v, Some((0, 10)));
        n.set(v);
    };
    let id = WidgetId::ROOT.with("copies");
    h.frame(f);
    let inc = h.rect_of(id.with("inc")).unwrap();
    let dec = h.rect_of(id.with("dec")).unwrap();
    assert!(dec.max.x <= inc.min.x, "− left, + right");
    h.click_at(inc.center(), f);
    h.click_at(inc.center(), f);
    assert_eq!(n.get(), 7);
    h.click_at(dec.center(), f);
    assert_eq!(n.get(), 6);
    for _ in 0..10 {
        h.click_at(inc.center(), f);
    }
    assert_eq!(n.get(), 10, "clamped to the range");
    // The middle is a number field: click, type, Enter.
    let mid = h.rect_of(id).unwrap();
    h.click_at(mid.center(), f);
    h.key_with(Key::Char('a'), Modifiers::CTRL);
    h.type_text("3");
    h.key(Key::Enter);
    h.settle(3, f);
    assert_eq!(n.get(), 3);
}

#[test]
fn range_slider_ends_and_span_drag() {
    let mut h = Harness::new(1000.0, 400.0);
    let lo = Cell::new(20.0);
    let hi = Cell::new(80.0);
    let f = |ui: &mut Ui| {
        let (mut a, mut b) = (lo.get(), hi.get());
        ui.range_slider("Crop", &mut a, &mut b, 0.0, 100.0, 1.0);
        lo.set(a);
        hi.set(b);
    };
    let id = WidgetId::ROOT.with("Crop");
    h.frame(f);
    let lo_thumb = h.rect_of(id.with("lo")).unwrap();
    let hi_thumb = h.rect_of(id.with("hi")).unwrap();
    let track_w = hi_thumb.center().x - lo_thumb.center().x;
    // Drag the low end right by a fifth of the way between the thumbs.
    h.drag(lo_thumb.center(), lo_thumb.center() + Vec2::new(track_w * 0.5, 0.0), 4, f);
    assert!((lo.get() - 50.0).abs() < 3.0, "lo moved to the middle: {}", lo.get());
    assert_eq!(hi.get(), 80.0);
    // The high end cannot cross the low one.
    let hi_thumb = h.rect_of(id.with("hi")).unwrap();
    h.drag(hi_thumb.center(), Vec2::new(0.0, hi_thumb.center().y), 4, f);
    assert!((hi.get() - lo.get()).abs() < 1.0, "clamped at lo: {} vs {}", hi.get(), lo.get());
    // Keyboard: Tab to the low end, Left nudges it down.
    h.key(Key::Tab);
    h.settle(2, f);
    assert_eq!(h.state.focus, Some(id.with("lo")));
    let before = lo.get();
    h.key(Key::ArrowLeft);
    h.settle(2, f);
    assert!(lo.get() < before);
}

#[test]
fn vertical_slider_drags_up_and_pad_drags_both_ways() {
    let mut h = Harness::new(800.0, 600.0);
    let v = Cell::new(0.2);
    let pad = Cell::new((0.5, 0.5));
    let f = |ui: &mut Ui| {
        ui.row(|ui| {
            let mut x = v.get();
            ui.vslider("Gain", &mut x, 0.0, 1.0, 0.0, 200.0);
            v.set(x);
            let (mut px, mut py) = pad.get();
            ui.xy_pad("Pan", &mut px, &mut py, (-1.0, 1.0), (-1.0, 1.0), 150.0);
            pad.set((px, py));
        });
    };
    h.frame(f);
    let fader = h.rect_of(WidgetId::ROOT.with("Gain")).unwrap();
    let bottom = Vec2::new(fader.center().x, fader.max.y - 2.0);
    h.drag(bottom, Vec2::new(bottom.x, fader.min.y + 2.0), 5, f);
    assert!(v.get() > 0.95, "dragged to the top: {}", v.get());
    let square = h.rect_of(WidgetId::ROOT.with("Pan")).unwrap();
    h.click_at(Vec2::new(square.max.x - 1.0, square.min.y + 1.0), f);
    let (x, y) = pad.get();
    assert!(x > 0.95 && y > 0.95, "top-right is more of both: {x}, {y}");
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowDown);
    h.settle(2, f);
    let (x2, y2) = pad.get();
    assert!(x2 < x && y2 < y, "arrows nudge: {x2}, {y2}");
}
