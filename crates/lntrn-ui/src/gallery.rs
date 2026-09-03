//! Every widget, live, for poking at.

use crate::ui::Ui;

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
            notes: "Several lines of text.\nClick to place the caret, drag to select, double-click a word.\nUp and Down remember the column; Enter breaks a line; Ctrl+Enter commits.\n\nWrapping happens at the box edge, so a long line like this one folds onto the next row when the area is narrow enough to need it.".to_owned(),
        }
    }
}

const CHOICES: [&str; 4] = ["Solid", "Wireframe", "Material Preview", "Rendered"];

pub fn draw(ui: &mut Ui, g: &mut GalleryState) {
    ui.tabs(&mut g.tab, &["Controls", "Knobs", "Text", "Lists", "Pictures"]);
    ui.space(ui.m.gap);
    match g.tab {
        0 => controls(ui, g),
        1 => knobs(ui, g),
        2 => text(ui, g),
        3 => lists(ui, g),
        _ => pictures(ui, g),
    }
}

fn pictures(ui: &mut Ui, g: &mut GalleryState) {
    ui.scroll_area("pictures", None, |ui| {
        ui.heading("Pictures");
        match g.image {
            Some(img) => {
                ui.label_dim(&format!("{} · {}×{}", g.image_name, img.width, img.height));
                ui.label("Fit inside a box, aspect kept, corners rounded:");
                ui.image_fit(img, lntrn_math::Vec2::new(crate::ui::FILL, 320.0));
                ui.label("At its own size, or as wide as the panel:");
                ui.row(|ui| {
                    ui.image(img, lntrn_math::Vec2::new(120.0, 120.0 / img.aspect()));
                    ui.image(img, lntrn_math::Vec2::new(60.0, 60.0 / img.aspect()));
                    ui.image(img, lntrn_math::Vec2::new(30.0, 30.0 / img.aspect()));
                });
                ui.image(img, lntrn_math::Vec2::new(crate::ui::FILL, 0.0));
            }
            None => {
                ui.paragraph("No picture yet. The host uploads one with Images::add and hands the gallery its handle; the demo makes one and can open a PNG or JPEG from the File menu.");
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
        ui.columns(&[crate::ui::FILL, crate::ui::FILL, crate::ui::FILL], |ui, i| {
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
        let r = ui.alloc(lntrn_math::Vec2::new(crate::ui::FILL, ui.m.widget_h));
        ui.text_in_rect(&format!("{name}: The quick brown fox jumps over the lazy dog"), &style, r, ui.theme.text);
        let _ = w;
    }
}

fn lists(ui: &mut Ui, g: &mut GalleryState) {
    ui.columns(&[crate::ui::FILL, crate::ui::FILL], |ui, col| {
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
