//! The gallery's bigger tabs: knobs, lists, tables, audio and pictures.

use lntrn_math::{Rect, Vec2};

use super::{GalleryState, KINDS};
use crate::ui::{FILL, Ui};
use crate::widgets::{Column, RowStep};

pub(super) fn audio(ui: &mut Ui, g: &mut GalleryState) {
    ui.label_dim("Meters hold their peaks, the waveform seeks on a click, the curve's points drag; double-click adds one, Delete removes it.");
    ui.row(|ui| {
        ui.toggle("Play", &mut g.playing);
        ui.label_dim(if g.playing { "A pretend signal breathes at thirty frames a second." } else { "Stopped: nothing asks for frames." });
    });
    if g.playing {
        let t = ui.now();
        let levels = [(0.55 + 0.35 * (t * 5.3).sin() * (t * 0.7).cos().abs()).clamp(0.0, 1.0), (0.5 + 0.4 * (t * 4.1 + 1.0).sin() * (t * 0.5).sin().abs()).clamp(0.0, 1.0)];
        for (ch, level) in levels.into_iter().enumerate() {
            let (_, peak) = g.meter[ch];
            g.meter[ch] = (level, (peak - 0.004).max(level));
        }
        g.playhead = (t * 0.08).fract();
        ui.state.request_redraw_after(1.0 / 30.0);
    }
    ui.columns(&[ui.m.px(120.0), FILL], |ui, col| {
        if col == 0 {
            ui.level_meter("L  R", &g.meter, ui.m.px(260.0));
        } else {
            ui.heading("Waveform");
            if let Some(t) = ui.waveform("wave", &g.samples, ui.m.px(150.0), Some(g.playhead)) {
                g.playhead = t;
            }
            ui.slider("Playhead", &mut g.playhead, 0.0, 1.0, 0.0);
        }
    });
    ui.heading("Curve");
    let h = ui.remaining_height().min(ui.m.px(260.0));
    let resp = ui.curve_editor("curve", &mut g.curve, h);
    if let Some(i) = resp.selected {
        let p = g.curve[i];
        ui.label_dim(&format!("Point {} at {:.2}, {:.2}", i + 1, p.x, p.y));
    }
}

pub(super) fn tables(ui: &mut Ui, g: &mut GalleryState) {
    ui.label_dim("Click a header to sort, drag its edge to resize, click a row and use Up and Down. Cells are widgets: drag the sizes, flip the toggles.");
    let cols = [Column::new("Name", 260.0).sortable(), Column::new("Kind", 180.0).sortable(), Column::new("Size", 160.0).right().sortable(), Column::fill("On")];
    let half = (ui.remaining_height() * 0.5).round();
    let picked = g.picked_row;
    let n = g.rows.len();
    let rows = &mut g.rows;
    let resp = ui.table("objects", &cols, n, Some(half), |t| {
        for i in t.visible() {
            t.row(i, picked == Some(i), |c| {
                let r = &mut rows[i];
                match c.col {
                    0 => c.label(&r.name),
                    1 => c.label_dim(KINDS[r.kind]),
                    2 => {
                        c.drag_value("", &mut r.size, 0.1, Some((0.0, 1000.0)), 1);
                    }
                    _ => {
                        c.toggle("", &mut r.on);
                    }
                }
            });
        }
    });
    if resp.sort_changed && let Some((col, asc)) = resp.sort {
        g.rows.sort_by(|a, b| {
            let o = match col {
                0 => a.name.cmp(&b.name),
                1 => a.kind.cmp(&b.kind),
                _ => a.size.total_cmp(&b.size),
            };
            if asc { o } else { o.reverse() }
        });
        g.picked_row = None;
    }
    if let Some(i) = resp.clicked {
        g.picked_row = Some(i);
    }
    let last = n.saturating_sub(1) as i64;
    match resp.step {
        RowStep::By(d) => g.picked_row = Some((picked.unwrap_or(0) as i64 + d as i64).clamp(0, last) as usize),
        RowStep::First => g.picked_row = Some(0),
        RowStep::Last => g.picked_row = Some(last as usize),
        RowStep::None => {}
    }
    ui.space(ui.m.gap);
    ui.columns(&[FILL, FILL], |ui, col| {
        if col == 0 {
            ui.heading("Ten thousand rows");
            let row_h = ui.m.widget_h + ui.m.gap;
            let pick = g.big_pick;
            let mut clicked = None;
            ui.virtual_list("big", 10_000, row_h, None, |ui, i| {
                if ui.selectable(&format!("Row {}", i + 1), pick == Some(i)).clicked {
                    clicked = Some(i);
                }
            });
            if clicked.is_some() {
                g.big_pick = clicked;
            }
        } else {
            ui.heading("Both ways");
            let cell = ui.m.px(120.0);
            let (nx, ny) = (30usize, 30usize);
            ui.scroll_area_2d("grid", None, nx as f64 * cell, |ui, view| {
                // Only the cells in view are drawn.
                let x0 = ((view.offset.x / cell).floor().max(0.0) as usize).min(nx);
                let y0 = ((view.offset.y / cell).floor().max(0.0) as usize).min(ny);
                let x1 = (((view.offset.x + view.viewport.width()) / cell).ceil() as usize).min(nx);
                let y1 = (((view.offset.y + view.viewport.height()) / cell).ceil() as usize).min(ny);
                let style = ui.text_style();
                for y in y0..y1 {
                    for x in x0..x1 {
                        let r = Rect::from_min_size(view.origin + Vec2::new(x as f64 * cell, y as f64 * cell), Vec2::splat(cell)).shrink(ui.m.gap * 0.5);
                        let tint = if (x + y) % 2 == 0 { ui.theme.widget } else { ui.theme.field };
                        ui.fill(r, tint);
                        ui.text_centered(&format!("{},{}", x + 1, y + 1), &style, r, ui.theme.text_dim);
                    }
                }
                // The content is the whole grid, whatever is in view.
                ui.alloc(Vec2::new(FILL, ny as f64 * cell));
            });
        }
    });
}

pub(super) fn pictures(ui: &mut Ui, g: &mut GalleryState) {
    // The whole tab takes pictures dragged in from outside.
    let zone_rect = Rect::new(ui.cursor(), ui.clip().max);
    let zone = ui.drop_zone(zone_rect);
    if !zone.files.is_empty() {
        g.dropped = zone.files;
    }
    if zone.hovering {
        ui.draw.stroke_rect(zone_rect, ui.m.px(3.0), ui.m.radius, ui.theme.accent);
    }
    ui.scroll_area("pictures", None, |ui| {
        ui.heading(if zone.hovering { "Drop it here" } else { "Pictures" });
        match g.image {
            Some(img) => {
                ui.label_dim(&format!("{} · {}×{}", g.image_name, img.width, img.height));
                ui.label("Fit inside a box, aspect kept, corners rounded:");
                ui.image_fit(img, Vec2::new(FILL, 320.0));
                ui.label("At its own size, or as wide as the panel:");
                ui.row(|ui| {
                    ui.image(img, Vec2::new(120.0, 120.0 / img.aspect()));
                    ui.image(img, Vec2::new(60.0, 60.0 / img.aspect()));
                    ui.image(img, Vec2::new(30.0, 30.0 / img.aspect()));
                });
                ui.image(img, Vec2::new(FILL, 0.0));
            }
            None => {
                ui.paragraph("No picture yet. The host uploads one with Images::add and hands the gallery its handle; the demo makes one and can open a PNG or JPEG from the File menu, or one dropped on this tab.");
            }
        }
    });
}

pub(super) fn knobs(ui: &mut Ui, g: &mut GalleryState) {
    ui.scroll_area("knobs", None, |ui| {
        ui.heading("Knobs");
        ui.label_dim("Drag up or right for more, Shift for fine, double-click to type. Tab reaches everything; arrows nudge.");
        ui.row(|ui| {
            ui.knob("Gain", &mut g.gain, 0.0, 1.0);
            ui.knob_sized("Cutoff", &mut g.cutoff, 20.0, 20000.0, 110.0);
            ui.knob("Resonance", &mut g.resonance, 0.0, 1.0);
        });
        ui.separator();
        ui.heading("Faders and pads");
        ui.row(|ui| {
            for (i, name) in ["Bass", "Mid", "Treble"].iter().enumerate() {
                ui.vslider(name, &mut g.faders[i], 0.0, 1.0, 0.0, ui.m.px(180.0));
            }
            ui.xy_pad("Pan / Tilt", &mut g.pad.0, &mut g.pad.1, (0.0, 1.0), (0.0, 1.0), 180.0);
        });
        ui.range_slider("Crop", &mut g.lo, &mut g.hi, 0.0, 100.0, 1.0);
        ui.separator();
        ui.heading("Progress");
        ui.slider("Set progress", &mut g.progress, 0.0, 1.0, 0.0);
        ui.progress("Rendering", g.progress);
        ui.progress("Scanning fonts", -1.0);
        ui.separator();
        ui.heading("Columns");
        ui.columns(&[FILL, FILL, FILL], |ui, i| {
            ui.label_dim(["Left", "Middle", "Right"][i]);
            ui.button_wide(["Alpha", "Beta", "Gamma"][i]);
            if i == 1 {
                ui.toggle("Extra tall column", &mut g.toggle_a);
            }
        });
        ui.label_dim("Layout continues below the tallest column.");
    });
}

pub(super) fn lists(ui: &mut Ui, g: &mut GalleryState) {
    ui.columns(&[FILL, FILL], |ui, col| {
        if col == 0 {
            ui.heading("Tree");
            let h = ui.remaining_height();
            ui.scroll_area("tree", Some(h), |ui| {
                let mut n = 0;
                let mut row = |ui: &mut Ui, label: &str, pick: &mut usize| {
                    n += 1;
                    if ui.tree_leaf(label, *pick == n).clicked {
                        *pick = n;
                    }
                };
                if ui.tree_node("Scene", g.tree_pick == 100, |ui| {
                    if ui.tree_node("Lights", g.tree_pick == 101, |ui| {
                        row(ui, "Key", &mut g.tree_pick);
                        row(ui, "Fill", &mut g.tree_pick);
                        row(ui, "Rim", &mut g.tree_pick);
                    }).clicked {
                        g.tree_pick = 101;
                    }
                    if ui.tree_node("Meshes", g.tree_pick == 102, |ui| {
                        row(ui, "Cube", &mut g.tree_pick);
                        row(ui, "Sphere", &mut g.tree_pick);
                        if ui.tree_node("Chair", g.tree_pick == 103, |ui| {
                            row(ui, "Seat", &mut g.tree_pick);
                            row(ui, "Legs", &mut g.tree_pick);
                        }).clicked {
                            g.tree_pick = 103;
                        }
                    }).clicked {
                        g.tree_pick = 102;
                    }
                    row(ui, "Camera", &mut g.tree_pick);
                }).clicked {
                    g.tree_pick = 100;
                }
            });
        } else {
            ui.heading("Selectable list");
            let h = ui.remaining_height();
            ui.scroll_area("list", Some(h), |ui| {
                for i in 0..40 {
                    ui.push_index(i);
                    if ui.selectable(&format!("Object {:02}", i + 1), g.selected == i).clicked {
                        g.selected = i;
                    }
                    ui.pop_id();
                }
            });
        }
    });
}
