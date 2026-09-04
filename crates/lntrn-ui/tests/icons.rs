//! Custom icons (U024): an app's own drawing function goes wherever a
//! built-in icon does.

use std::sync::atomic::{AtomicUsize, Ordering};

use lntrn_math::{Color, Rect, Vec2};
use lntrn_render::DrawList;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Icon, Ui, WidgetId, icons};

static DRAWN: AtomicUsize = AtomicUsize::new(0);

fn mark(d: &mut DrawList, rect: Rect, color: Color, stroke: f64) {
    DRAWN.fetch_add(1, Ordering::SeqCst);
    d.line(rect.min, rect.max, stroke, color);
}

fn other(_: &mut DrawList, _: Rect, _: Color, _: f64) {}

#[test]
fn a_custom_icon_draws_through_its_function() {
    let mut d = DrawList::new();
    let before = DRAWN.load(Ordering::SeqCst);
    icons::draw(&mut d, Rect::from_min_size(Vec2::ZERO, Vec2::splat(40.0)), Icon::Custom(mark), Color::WHITE, 2.0);
    assert_eq!(DRAWN.load(Ordering::SeqCst), before + 1);
    assert!(d.vertex_count() > 0, "the function's line landed in the list");
    assert_eq!(Icon::Custom(mark), Icon::Custom(mark), "the same function is the same icon");
    assert_ne!(Icon::Custom(mark), Icon::Custom(other));
    assert_ne!(Icon::Custom(mark), Icon::Plus);
}

#[test]
fn a_custom_icon_button_clicks() {
    let mut h = Harness::new(800.0, 600.0);
    let mut clicks = 0;
    let before = DRAWN.load(Ordering::SeqCst);
    let mut f = |ui: &mut Ui| {
        if ui.icon_button("mine", Icon::Custom(mark), false, "The app's own").clicked {
            clicks += 1;
        }
    };
    h.click_on(WidgetId::ROOT.with("mine"), &mut f);
    assert_eq!(clicks, 1);
    assert!(DRAWN.load(Ordering::SeqCst) > before, "the button drew it");
}
