//! Every widget, live, for poking at.

use std::path::PathBuf;

use lntrn_math::{Rect, Vec2};

use crate::ui::{FILL, Ui};
use crate::widgets::{Column, RowStep};

/// The gallery's tabs, in order.
pub const TABS: [&str; 6] = ["Controls", "Knobs", "Text", "Lists", "Tables", "Pictures"];

/// The kinds the table's rows come in.
pub const KINDS: [&str; 4] = ["Mesh", "Light", "Camera", "Empty"];

/// A row of the gallery's table.
#[derive(Clone, Debug, PartialEq)]
pub struct TableRow {
    pub name: String,
    pub kind: usize,
    pub size: f64,
    pub on: bool,
}

/// Five hundred rows, the same every run.
fn sample_rows() -> Vec<TableRow> {
    const NAMES: [&str; 10] = ["Cube", "Sphere", "Suzanne", "Torus", "Plane", "Cone", "Key", "Fill", "Rim", "Lens"];
    (0..500)
        .map(|i| {
            let mut x = (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            x ^= x >> 29;
            TableRow { name: format!("{} {:03}", NAMES[i % NAMES.len()], i + 1), kind: ((x >> 8) % 4) as usize, size: ((x >> 16) % 10_000) as f64 / 10.0, on: x & 1 == 0 }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct GalleryState {
    pub clicks: u32,
    pub tab: usize,
    pub toggle_a: bool,
    pub toggle_b: bool,
    pub slider: f64,
    pub number: f64,
    pub count: i64,
    pub text: String,
    pub choice: usize,
    pub selected: usize,
    pub gain: f64,
    pub cutoff: f64,
    pub resonance: f64,
    pub progress: f64,
    pub color: lntrn_math::Color,
    pub tree_pick: usize,
    pub notes: String,
    /// A picture the host uploaded (see `lntrn-demo`), shown on its tab.
    pub image: Option<crate::ImageHandle>,
    pub image_name: String,
    /// Files dropped on the Pictures tab, for the host to open.
    pub dropped: Vec<PathBuf>,
    pub rows: Vec<TableRow>,
    pub picked_row: Option<usize>,
    /// The pick in the ten-thousand-row list.
    pub big_pick: Option<usize>,
}

impl Default for GalleryState {
    fn default() -> Self {
        Self {
            clicks: 0,
            tab: 0,
            toggle_a: true,
            toggle_b: false,
            slider: 0.35,
            number: 1.5,
            count: 3,
            text: "Type here".to_owned(),
            choice: 1,
            selected: 2,
            gain: 0.5,
            cutoff: 800.0,
            resonance: 0.2,
            progress: 0.4,
            color: lntrn_math::Color::hex(0xFFB733),
            tree_pick: 0,
            image: None,
            image_name: String::new(),
            dropped: Vec::new(),
            rows: sample_rows(),
            picked_row: None,
            big_pick: None,
            notes: "Several lines of text.\nClick to place the caret, drag to select, double-click a word.\nUp and Down remember the column; Enter breaks a line; Ctrl+Enter commits.\n\nWrapping happens at the box edge, so a long line like this one folds onto the next row when the area is narrow enough to need it.".to_owned(),
        }
    }
}

const CHOICES: [&str; 4] = ["Solid", "Wireframe", "Material Preview", "Rendered"];

pub fn draw(ui: &mut Ui, g: &mut GalleryState) {
    ui.tabs(&mut g.tab, &TABS);
    ui.space(ui.m.gap);
    match g.tab {
        0 => controls(ui, g),
        1 => knobs(ui, g),
        2 => text(ui, g),
        3 => lists(ui, g),
        4 => tables(ui, g),
        _ => pictures(ui, g),
    }
}

fn tables(ui: &mut Ui, g: &mut GalleryState) {
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

fn pictures(ui: &mut Ui, g: &mut GalleryState) {
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

fn knobs(ui: &mut Ui, g: &mut GalleryState) {
    ui.scroll_area("knobs", None, |ui| {
        ui.heading("Knobs");
        ui.label_dim("Drag up or right for more, Shift for fine, double-click to type. Tab reaches everything; arrows nudge.");
        ui.row(|ui| {
            ui.knob("Gain", &mut g.gain, 0.0, 1.0);
            ui.knob_sized("Cutoff", &mut g.cutoff, 20.0, 20000.0, 110.0);
            ui.knob("Resonance", &mut g.resonance, 0.0, 1.0);
        });
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

fn controls(ui: &mut Ui, g: &mut GalleryState) {
    ui.scroll_area("controls", None, |ui| {
        ui.heading("Buttons");
        ui.row(|ui| {
            if ui.button("Click me").clicked {
                g.clicks += 1;
            }
            if ui.button("Reset").clicked {
                g.clicks = 0;
            }
            ui.label(&format!("Clicked {} times", g.clicks));
        });
        if ui.button_wide("Wide button").clicked {
            g.clicks += 10;
        }
        ui.separator();
        ui.heading("Toggles");
        ui.toggle("Show overlays", &mut g.toggle_a);
        ui.toggle("Snap to grid", &mut g.toggle_b);
        ui.separator();
        ui.heading("Numbers");
        ui.slider("Opacity", &mut g.slider, 0.0, 1.0, 0.0);
        ui.labelled("Extrude", |ui| {
            ui.drag_value("", &mut g.number, 0.01, None, 3);
        });
        ui.labelled("Segments", |ui| {
            ui.drag_int("", &mut g.count, Some((1, 64)));
        });
        ui.labelled("Position", |ui| {
            ui.row(|ui| {
                let mut v = [g.number, g.slider, g.count as f64];
                ui.push_id("pos");
                for (k, n) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.push_index(k);
                    ui.drag_value(n, &mut v[k], 0.01, None, 2);
                    ui.pop_id();
                }
                ui.pop_id();
            });
        });
        ui.separator();
        ui.heading("Colour");
        ui.labelled("Accent", |ui| {
            ui.row(|ui| {
                ui.color_picker("accent", &mut g.color);
                ui.label_dim(&g.color.to_hex_string());
            });
        });
        ui.separator();
        ui.heading("Choice");
        ui.labelled("Shading", |ui| {
            ui.dropdown("shading", &mut g.choice, &CHOICES);
        });
        ui.row(|ui| {
            if let Some(i) = ui.menu_button("Menu", &["New", "Open…", "Save", "Quit"]) {
                g.clicks += i as u32;
            }
            ui.dropdown("shading2", &mut g.choice, &CHOICES);
        });
    });
}

fn text(ui: &mut Ui, g: &mut GalleryState) {
    ui.heading("Text field");
    ui.text_field("name", &mut g.text);
    ui.label_dim(&format!("{} bytes, {} chars", g.text.len(), g.text.chars().count()));
    ui.separator();
    ui.heading("Text area");
    ui.text_area("notes", &mut g.notes, Some(ui.m.px(220.0)));
    ui.separator();
    ui.heading("Paragraph");
    ui.paragraph(
        "Immediate-mode widgets inside a retained area tree. Everything you see here is re-declared on every \
         rebuild, and a rebuild only happens when an input event arrives. Idle costs nothing.",
    );
    ui.separator();
    ui.heading("Weights");
    let body = ui.text_style();
    let w = ui.avail_width();
    for (name, style) in [
        ("Regular", body.clone()),
        ("Bold", body.clone().bold()),
        ("Italic", body.clone().italic()),
        ("Mono", body.clone().mono()),
    ] {
        let r = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
        ui.text_in_rect(&format!("{name}: The quick brown fox jumps over the lazy dog"), &style, r, ui.theme.text);
        let _ = w;
    }
}

fn lists(ui: &mut Ui, g: &mut GalleryState) {
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
