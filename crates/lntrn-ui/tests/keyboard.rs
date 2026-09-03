//! Keyboard focus, arrows, the clipboard, knobs and animation, headless.

use std::cell::{Cell, RefCell};

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, Modifiers, Ui, WidgetId};

#[test]
fn tab_walks_widgets_and_enter_activates() {
    let mut h = Harness::new(800.0, 600.0);
    let clicks = Cell::new(0);
    let on = Cell::new(false);
    let tab = Cell::new(0);
    let f = |ui: &mut Ui| {
        if ui.button("First").clicked {
            clicks.set(clicks.get() + 1);
        }
        let mut v = on.get();
        ui.toggle("Snap", &mut v);
        on.set(v);
        let mut t = tab.get();
        ui.tabs(&mut t, &["A", "B"]);
        tab.set(t);
    };
    h.frame(f);
    h.key(Key::Tab);
    h.settle(3, f);
    assert_eq!(h.state.focus, Some(WidgetId::ROOT.with("First")));
    assert!(h.state.focus_visible);
    h.key(Key::Enter);
    h.settle(3, f);
    assert_eq!(clicks.get(), 1, "Enter clicks the focused button");
    h.key(Key::Tab);
    h.settle(3, f);
    h.key(Key::Space);
    h.settle(3, f);
    assert!(on.get(), "Space toggles the focused checkbox");
    h.key(Key::Tab);
    h.settle(3, f);
    h.key(Key::Tab);
    h.settle(3, f);
    h.key(Key::Enter);
    h.settle(3, f);
    assert_eq!(tab.get(), 1, "the second tab button is a focus stop of its own");
    for _ in 0..3 {
        h.key_with(Key::Tab, Modifiers::SHIFT);
        h.settle(3, f);
    }
    assert_eq!(h.state.focus, Some(WidgetId::ROOT.with("First")), "Shift+Tab walks back");
    // A pointer press elsewhere clears focus and hides the rings.
    h.move_to(Vec2::new(700.0, 550.0));
    h.press();
    h.frame(f);
    h.release();
    h.settle(2, f);
    assert_eq!(h.state.focus, None);
    assert!(!h.state.focus_visible);
}

#[test]
fn arrows_drive_sliders_dropdowns_and_knobs() {
    let mut h = Harness::new(800.0, 600.0);
    let v = RefCell::new(0.5);
    let choice = Cell::new(1);
    let k = RefCell::new(0.0);
    let f = |ui: &mut Ui| {
        ui.slider("Opacity", &mut v.borrow_mut(), 0.0, 1.0, 0.0);
        let mut c = choice.get();
        ui.dropdown("shading", &mut c, &["Solid", "Wire", "Material"]);
        choice.set(c);
        ui.knob("Gain", &mut k.borrow_mut(), 0.0, 10.0);
    };
    h.frame(f);
    h.key(Key::Tab);
    h.settle(2, f);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowRight);
    h.settle(2, f);
    assert!((*v.borrow() - 0.52).abs() < 1e-9, "two 1% steps in one frame both count: {}", v.borrow());
    h.key(Key::End);
    h.settle(2, f);
    assert_eq!(*v.borrow(), 1.0);
    h.key(Key::Tab);
    h.settle(2, f);
    h.key(Key::ArrowDown);
    h.settle(2, f);
    assert_eq!(choice.get(), 0, "Down picks the previous option without opening");
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    h.settle(2, f);
    assert_eq!(choice.get(), 2);
    h.key(Key::Tab);
    h.settle(2, f);
    h.key(Key::ArrowUp);
    h.settle(2, f);
    assert!((*k.borrow() - 0.1).abs() < 1e-9, "1% of the knob's range: {}", k.borrow());
    h.key(Key::Home);
    h.settle(2, f);
    assert_eq!(*k.borrow(), 0.0);
}

#[test]
fn knob_drags_up_for_more() {
    let mut h = Harness::new(800.0, 600.0);
    let k = RefCell::new(2.0);
    let f = |ui: &mut Ui| {
        ui.knob("Gain", &mut k.borrow_mut(), 0.0, 10.0);
    };
    h.frame(f);
    let r = h.rect_of_label("Gain").unwrap();
    assert!(r.width() >= 64.0 && r.height() >= 64.0, "knobs are big: {r:?}");
    // 100 px up is half the drag range: +5.
    h.drag(r.center(), r.center() - Vec2::new(0.0, 100.0), 5, f);
    assert!((*k.borrow() - 7.0).abs() < 0.05, "{}", k.borrow());
    // Dragging right also counts as more; past the top it clamps. (A second
    // press within 0.4 s would be a double click, which opens typing.)
    h.advance(1.0);
    h.drag(r.center(), r.center() + Vec2::new(300.0, 0.0), 5, f);
    assert_eq!(*k.borrow(), 10.0);
}

#[test]
fn clipboard_copy_cut_paste() {
    let mut h = Harness::new(800.0, 600.0);
    let a = RefCell::new(String::from("hello"));
    let b = RefCell::new(String::new());
    let f = |ui: &mut Ui| {
        ui.text_field("a", &mut a.borrow_mut());
        ui.text_field("b", &mut b.borrow_mut());
    };
    h.click_on(WidgetId::ROOT.with("a"), f);
    h.key_with(Key::Char('a'), Modifiers::CTRL);
    h.key_with(Key::Char('c'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(h.state.clipboard, "hello");
    h.click_on(WidgetId::ROOT.with("b"), f);
    h.key_with(Key::Char('v'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(*b.borrow(), "hello");
    h.key_with(Key::Char('a'), Modifiers::CTRL);
    h.key_with(Key::Char('x'), Modifiers::CTRL);
    h.frame(f);
    assert_eq!(*b.borrow(), "");
    assert_eq!(h.state.clipboard, "hello");
    // Tab from a text field moves on instead of typing a tab.
    h.key(Key::Tab);
    h.settle(2, f);
    assert_eq!(h.state.focus, Some(WidgetId::ROOT.with("a")), "wrapped to the first field");
    assert_eq!(*a.borrow(), "hello");
}

#[test]
fn animation_settles_and_asks_for_frames() {
    let mut h = Harness::new(800.0, 600.0);
    let seen = RefCell::new(Vec::new());
    let target = Cell::new(0.0);
    let f = |ui: &mut Ui| {
        let v = ui.animate(WidgetId::ROOT.with("fade"), target.get(), 0.2);
        seen.borrow_mut().push(v);
    };
    h.frame(f);
    assert_eq!(h.state.wake_after, None, "at rest costs nothing");
    target.set(1.0);
    h.frame(f);
    assert!(h.state.wake_after.is_some(), "moving: wants a frame soon");
    for _ in 0..30 {
        h.advance(1.0 / 60.0);
        h.frame(f);
    }
    let last = *seen.borrow().last().unwrap();
    assert!((last - 1.0).abs() < 1e-9, "settled after half a second: {last}");
    assert_eq!(h.state.wake_after, None);
    let v = seen.borrow();
    assert!(v.windows(2).all(|w| w[1] >= w[0]), "monotonic: {v:?}");
    assert!(v[2] > 0.0 && v[2] < 1.0, "in between on the way: {}", v[2]);
}

#[test]
fn progress_and_columns_lay_out() {
    let mut h = Harness::new(800.0, 600.0);
    let f = |ui: &mut Ui| {
        ui.progress("Loading", 0.4);
        ui.progress("Busy", -1.0);
        ui.columns(&[200.0, lntrn_ui::FILL, lntrn_ui::FILL], |ui, i| {
            ui.button(&format!("col {i}"));
            if i == 1 {
                ui.button("tall");
            }
        });
        ui.button("after");
    };
    h.frame(f);
    assert!(h.state.wake_after.is_some(), "the busy bar animates");
    let c0 = h.rect_of(WidgetId::ROOT.with_index(0).with("col 0")).unwrap();
    let c1 = h.rect_of(WidgetId::ROOT.with_index(1).with("col 1")).unwrap();
    let c2 = h.rect_of(WidgetId::ROOT.with_index(2).with("col 2")).unwrap();
    let after = h.rect_of_label("after").unwrap();
    assert_eq!(c0.min.y, c1.min.y);
    assert!(c1.min.x >= c0.min.x + 200.0 && c2.min.x > c1.min.x);
    let tall = h.rect_of(WidgetId::ROOT.with_index(1).with("tall")).unwrap();
    assert!(after.min.y >= tall.max.y, "layout continues below the tallest column");
}

#[test]
fn tab_scrolls_the_focused_widget_into_view() {
    let mut h = Harness::new(600.0, 300.0);
    let f = |ui: &mut Ui| {
        ui.scroll_area("list", None, |ui| {
            for i in 0..30 {
                ui.push_index(i);
                ui.button_wide(&format!("Button {i}"));
                ui.pop_id();
            }
        });
    };
    h.frame(f);
    let list = WidgetId::ROOT.with("list");
    assert_eq!(h.state.scroll(list).offset.y, 0.0);
    // Tab through the first ten buttons: the tenth is below the 280px
    // viewport, so the area scrolls to show it.
    for _ in 0..10 {
        h.key(Key::Tab);
        h.settle(3, f);
    }
    assert_eq!(h.state.focus, Some(list.with_index(9).with("Button 9")));
    assert!(h.state.scroll(list).offset.y > 0.0, "scrolled to the focused button");
    let r = h.rect_of(list.with_index(9).with("Button 9")).unwrap();
    assert!(r.max.y <= 300.0 && r.min.y >= 0.0, "in view: {r:?}");
    // Shift+Tab back to the top scrolls up again.
    for _ in 0..9 {
        h.key_with(Key::Tab, Modifiers::SHIFT);
        h.settle(3, f);
    }
    assert_eq!(h.state.scroll(list).offset.y, 0.0);
}

#[test]
fn theme_presets_apply_from_preferences() {
    use lntrn_ui::{Prefs, Theme, prefs};
    let mut h = Harness::new(900.0, 700.0);
    let p = RefCell::new(Prefs::default());
    let f = |ui: &mut Ui| {
        prefs::draw(ui, &mut p.borrow_mut());
    };
    h.click_on(WidgetId::ROOT.with("prefs").with("Light"), f);
    assert_eq!(p.borrow().theme.panel, Theme::light().panel);
    h.advance(1.0);
    h.click_on(WidgetId::ROOT.with("prefs").with("High Contrast"), f);
    assert_eq!(p.borrow().theme.text_size, Theme::high_contrast().text_size);
    h.advance(1.0);
    h.click_on(WidgetId::ROOT.with("prefs").with("Dark"), f);
    assert_eq!(p.borrow().theme.panel, Theme::default().panel);
}
