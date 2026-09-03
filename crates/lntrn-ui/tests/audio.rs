//! Meters, waveforms and the curve editor, headless.

use std::cell::RefCell;

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Key, Ui, WidgetId};

#[test]
fn meters_and_waveforms_draw_and_seek() {
    let mut h = Harness::new(800.0, 500.0);
    let samples: Vec<f32> = (0..4000).map(|i| (i as f32 * 0.05).sin()).collect();
    let sought = RefCell::new(None);
    let f = |ui: &mut Ui| {
        ui.level_meter("L R", &[(0.5, 0.8), (0.95, 0.97)], 200.0);
        if let Some(t) = ui.waveform("wave", &samples, 120.0, Some(0.25)) {
            *sought.borrow_mut() = Some(t);
        }
    };
    let loud = h.frame(f);
    assert!(loud.vertices > 400, "one column per pixel: {}", loud.vertices);
    let wave = h.rect_of(WidgetId::ROOT.with("wave")).unwrap();
    h.click_at(Vec2::new(wave.min.x + wave.width() * 0.75, wave.center().y), f);
    let t = sought.borrow().expect("a click seeks");
    assert!((t - 0.75).abs() < 0.02, "{t}");
    // Silence still draws the frame, just less of it.
    let quiet = vec![0.0f32; 4000];
    let g = |ui: &mut Ui| {
        ui.waveform("wave", &quiet, 120.0, None);
    };
    let flat = h.frame(g);
    assert!(flat.vertices < loud.vertices && flat.vertices > 0);
}

#[test]
fn curve_editor_drags_adds_and_deletes() {
    let mut h = Harness::new(800.0, 500.0);
    let points = RefCell::new(vec![Vec2::new(0.0, 0.0), Vec2::new(0.5, 0.5), Vec2::new(1.0, 1.0)]);
    let f = |ui: &mut Ui| {
        ui.curve_editor("curve", &mut points.borrow_mut(), 300.0);
    };
    h.frame(f);
    let id = WidgetId::ROOT.with("curve");
    let mid = h.rect_of(id.with_index(1)).expect("the middle point's handle");
    // Dragging the middle point up raises its y.
    h.drag(mid.center(), mid.center() - Vec2::new(0.0, 100.0), 4, f);
    let p = points.borrow()[1];
    assert!(p.y > 0.7 && (p.x - 0.5).abs() < 0.02, "{p:?}");
    // A double click on empty space adds a point, kept in order by x.
    let area = h.rect_of(id).unwrap();
    let at = Vec2::new(area.min.x + area.width() * 0.75, area.center().y);
    h.advance(1.0);
    h.click_at(at, f);
    h.click_at(at, f);
    assert_eq!(points.borrow().len(), 4, "{:?}", points.borrow());
    assert!((points.borrow()[2].x - 0.75).abs() < 0.05, "in order: {:?}", points.borrow());
    // The new point is selected; Delete removes it.
    h.key(Key::Delete);
    h.settle(2, f);
    assert_eq!(points.borrow().len(), 3);
    // Dragging a point past its neighbour keeps the order.
    let first = h.rect_of(id.with_index(0)).unwrap();
    h.advance(1.0);
    h.drag(first.center(), Vec2::new(area.max.x - 5.0, area.center().y), 4, f);
    let pts = points.borrow();
    assert!(pts.windows(2).all(|w| w[0].x <= w[1].x), "still sorted: {pts:?}");
}
