//! Tables, virtual lists and two-way scroll areas, headless.

use std::cell::{Cell, RefCell};

use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Column, FILL, Key, Modifiers, RowStep, Ui, WidgetId};

#[test]
fn table_selects_steps_sorts_and_resizes() {
    let mut h = Harness::new(900.0, 600.0);
    let rows = RefCell::new(vec![("pear", 3.0f64), ("apple", 1.0), ("fig", 2.0)]);
    let picked = Cell::new(None);
    let f = |ui: &mut Ui| {
        let cols = [Column::new("Name", 200.0).sortable(), Column::new("Size", 150.0).right().sortable(), Column::fill("On")];
        let sel = picked.get();
        let n = rows.borrow().len();
        let resp = ui.table("t", &cols, n, Some(400.0), |t| {
            for i in t.visible() {
                t.row(i, sel == Some(i), |c| {
                    let r = rows.borrow();
                    match c.col {
                        0 => c.label(r[i].0),
                        1 => c.label(&format!("{}", r[i].1)),
                        _ => {}
                    }
                });
            }
        });
        if let Some(i) = resp.clicked {
            picked.set(Some(i));
        }
        if resp.sort_changed && let Some((col, asc)) = resp.sort {
            rows.borrow_mut().sort_by(|a, b| {
                let o = if col == 0 { a.0.cmp(b.0) } else { a.1.total_cmp(&b.1) };
                if asc { o } else { o.reverse() }
            });
        }
        let last = n as i64 - 1;
        match resp.step {
            RowStep::By(d) => picked.set(Some((sel.unwrap_or(0) as i64 + d as i64).clamp(0, last) as usize)),
            RowStep::First => picked.set(Some(0)),
            RowStep::Last => picked.set(Some(last as usize)),
            RowStep::None => {}
        }
    };
    let names = || rows.borrow().iter().map(|r| r.0).collect::<Vec<_>>();
    h.frame(f);
    let t = WidgetId::ROOT.with("t");
    assert!(h.rect_of(t.with("row").with_index(2)).is_some(), "three rows laid out");
    assert!(h.rect_of(t.with("row").with_index(3)).is_none());

    // A click selects; then the table has focus and the arrows step.
    let row1 = h.rect_of(t.with("row").with_index(1)).unwrap();
    h.click_at(row1.center(), f);
    assert_eq!(picked.get(), Some(1));
    assert_eq!(h.state.focus, Some(t));
    h.key(Key::ArrowDown);
    h.settle(2, f);
    assert_eq!(picked.get(), Some(2), "Down is the next row");
    h.key(Key::ArrowDown);
    h.settle(2, f);
    assert_eq!(picked.get(), Some(2), "and stops at the end");
    h.key(Key::Home);
    h.settle(2, f);
    assert_eq!(picked.get(), Some(0));
    h.key(Key::End);
    h.settle(2, f);
    assert_eq!(picked.get(), Some(2));

    // Clicking a header sorts; again reverses.
    let name = h.rect_of(t.with("col").with_index(0)).unwrap();
    h.click_at(name.center(), f);
    assert_eq!(names(), ["apple", "fig", "pear"]);
    h.click_at(name.center(), f);
    assert_eq!(names(), ["pear", "fig", "apple"]);
    let size = h.rect_of(t.with("col").with_index(1)).unwrap();
    h.click_at(size.center(), f);
    assert_eq!(names(), ["apple", "fig", "pear"], "by size, ascending, fresh");
    let on = h.rect_of(t.with("col").with_index(2)).unwrap();
    h.click_at(on.center(), f);
    assert_eq!(names(), ["apple", "fig", "pear"], "an unsortable header does nothing");

    // Dragging a grip widens its column.
    let grip = h.rect_of(t.with("col").with_index(0).with("grip")).unwrap();
    assert!((grip.center().x - name.max.x).abs() < 1.0, "the grip sits on the column's edge");
    h.drag(grip.center(), grip.center() + Vec2::new(100.0, 0.0), 4, f);
    let name_after = h.rect_of(t.with("col").with_index(0)).unwrap();
    assert!((name_after.width() - (name.width() + 100.0)).abs() < 2.0, "{} vs {}", name_after.width(), name.width());
    let size_after = h.rect_of(t.with("col").with_index(1)).unwrap();
    assert!((size_after.min.x - name_after.max.x).abs() < 1.0, "the next column moved over");
}

#[test]
fn table_cells_are_widgets() {
    let mut h = Harness::new(900.0, 400.0);
    let on = RefCell::new(vec![false, false]);
    let f = |ui: &mut Ui| {
        let cols = [Column::new("Name", 200.0), Column::fill("On")];
        ui.table("t", &cols, 2, Some(300.0), |t| {
            for i in t.visible() {
                t.row(i, false, |c| {
                    if c.col == 0 {
                        c.label("row");
                    } else {
                        c.toggle("", &mut on.borrow_mut()[i]);
                    }
                });
            }
        });
    };
    h.frame(f);
    let toggle = WidgetId::ROOT.with("t").with("body").with_index(1).with_index(1).with("");
    let r = h.rect_of(toggle).expect("the toggle in row 1, column 1");
    h.click_at(Vec2::new(r.min.x + 10.0, r.center().y), f);
    assert_eq!(*on.borrow(), vec![false, true], "the cell widget took the click, not the row");
}

#[test]
fn virtual_list_lays_out_only_the_rows_in_view() {
    let mut h = Harness::new(600.0, 300.0);
    let laid = RefCell::new(Vec::new());
    let picked = Cell::new(None);
    let f = |ui: &mut Ui| {
        laid.borrow_mut().clear();
        ui.virtual_list("big", 10_000, 50.0, None, |ui, i| {
            laid.borrow_mut().push(i);
            if ui.selectable(&format!("Row {i}"), false).clicked {
                picked.set(Some(i));
            }
        });
    };
    h.frame(f);
    let n = laid.borrow().len();
    assert!((5..=8).contains(&n), "only the rows in view: {n}");
    assert_eq!(laid.borrow()[0], 0);
    h.move_to(Vec2::new(200.0, 150.0));
    h.wheel(-40.0);
    h.frame(f);
    let first = laid.borrow()[0];
    assert!(first > 30, "scrolled well down: {first}");
    // A row deep in the list is clickable by its own id.
    let row = laid.borrow()[2];
    let r = h.rect_of(WidgetId::ROOT.with("big").with_index(row).with(&format!("Row {row}"))).expect("a visible row");
    h.click_at(r.center(), f);
    assert_eq!(picked.get(), Some(row));
    // Dragging the thumb to the bottom shows the last row.
    let thumb = h.rect_of(WidgetId::ROOT.with("big").with("thumb")).unwrap();
    h.drag(thumb.center(), Vec2::new(thumb.center().x, 900.0), 3, f);
    assert_eq!(*laid.borrow().last().unwrap(), 9_999);
}

#[test]
fn scroll_area_2d_goes_both_ways() {
    let mut h = Harness::new(400.0, 300.0);
    let seen = Cell::new(Vec2::ZERO);
    let f = |ui: &mut Ui| {
        ui.scroll_area_2d("grid", None, 2000.0, |ui, view| {
            seen.set(view.offset);
            ui.alloc(Vec2::new(FILL, 1500.0));
        });
    };
    h.frame(f);
    h.frame(f);
    assert_eq!(seen.get(), Vec2::ZERO);
    h.move_to(Vec2::new(200.0, 150.0));
    h.wheel(-2.0);
    h.frame(f);
    assert!(seen.get().y > 0.0 && seen.get().x == 0.0, "the wheel scrolls down: {:?}", seen.get());
    h.set_mods(Modifiers::SHIFT);
    h.wheel(-2.0);
    h.frame(f);
    h.set_mods(Modifiers::NONE);
    assert!(seen.get().x > 0.0, "Shift+wheel scrolls across: {:?}", seen.get());
    let before = seen.get().x;
    let thumb = h.rect_of(WidgetId::ROOT.with("grid").with("hthumb")).expect("a bar along the bottom");
    h.drag(thumb.center(), thumb.center() + Vec2::new(100.0, 0.0), 2, f);
    assert!(seen.get().x > before, "the bottom thumb drags: {:?}", seen.get());
    assert!(h.rect_of(WidgetId::ROOT.with("grid").with("thumb")).is_some(), "and the side one is there too");
}
