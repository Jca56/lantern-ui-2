//! Every widget, live, for poking at.

use std::path::PathBuf;

mod tabs;

use lntrn_math::Vec2;

use crate::icons::Icon;
use crate::ui::{FILL, Ui};
use tabs::{audio, knobs, lists, pictures, tables};

/// The gallery's tabs, in order.
pub const TABS: [&str; 7] = ["Controls", "Knobs", "Text", "Lists", "Tables", "Audio", "Pictures"];

/// A second of a decaying chord, for the waveform.
fn sample_wave() -> Vec<f32> {
    use core::f32::consts::TAU;
    (0..24_000)
        .map(|i| {
            let t = i as f32 / 24_000.0;
            let env = (1.0 - t).powi(2);
            ((t * 220.0 * TAU).sin() * 0.6 + (t * 330.0 * TAU).sin() * 0.3 + (t * 1760.0 * TAU).sin() * 0.1) * env
        })
        .collect()
}

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
    /// The picture was dragged off its tab: the host, which has the
    /// pixels, starts the drag out of the window.
    pub drag_picture: bool,
    /// The last link clicked in the rich text sample.
    pub last_link: String,
    /// A validated field: a port number.
    pub port: String,
    pub rows: Vec<TableRow>,
    pub picked_row: Option<usize>,
    /// The pick in the ten-thousand-row list.
    pub big_pick: Option<usize>,
    pub spin: i64,
    pub radio: usize,
    pub radio_row: usize,
    pub search: String,
    pub secret: String,
    pub font: String,
    /// The range slider's ends.
    pub lo: f64,
    pub hi: f64,
    pub faders: [f64; 3],
    pub pad: (f64, f64),
    /// The Audio tab: a pretend transport.
    pub playing: bool,
    pub samples: Vec<f32>,
    pub playhead: f64,
    /// `(level, peak)` per channel.
    pub meter: [(f64, f64); 2],
    pub curve: Vec<Vec2>,
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
            drag_picture: false,
            last_link: String::new(),
            port: "8080".to_owned(),
            rows: sample_rows(),
            picked_row: None,
            big_pick: None,
            spin: 5,
            radio: 0,
            radio_row: 1,
            search: String::new(),
            secret: String::new(),
            font: String::new(),
            lo: 20.0,
            hi: 80.0,
            faders: [0.7, 0.5, 0.3],
            pad: (0.5, 0.5),
            playing: false,
            samples: sample_wave(),
            playhead: 0.25,
            meter: [(0.6, 0.75), (0.5, 0.65)],
            curve: vec![Vec2::new(0.0, 0.0), Vec2::new(0.25, 0.9), Vec2::new(0.6, 0.5), Vec2::new(1.0, 0.1)],
            notes: "Several lines of text.\nClick to place the caret, drag to select, double-click a word.\nUp and Down remember the column; Enter breaks a line; Ctrl+Enter commits.\n\nWrapping happens at the box edge, so a long line like this one folds onto the next row when the area is narrow enough to need it.".to_owned(),
        }
    }
}

const CHOICES: [&str; 4] = ["Solid", "Wireframe", "Material Preview", "Rendered"];
const FONTS: [&str; 6] = ["Inter", "JetBrains Mono", "Noto Sans", "Noto Serif", "Fira Code", "Source Sans"];

pub fn draw(ui: &mut Ui, g: &mut GalleryState) {
    ui.tabs(&mut g.tab, &TABS);
    ui.space(ui.m.gap);
    match g.tab {
        0 => controls(ui, g),
        1 => knobs(ui, g),
        2 => text(ui, g),
        3 => lists(ui, g),
        4 => tables(ui, g),
        5 => audio(ui, g),
        _ => pictures(ui, g),
    }
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
        ui.labelled("Copies", |ui| {
            ui.spinner_int("", &mut g.spin, Some((0, 100)));
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
        ui.labelled("Mode", |ui| {
            ui.radio("mode", &mut g.radio, &["Object", "Edit", "Sculpt"]);
        });
        ui.row(|ui| {
            ui.label_dim("In a row:");
            ui.radio("units", &mut g.radio_row, &["Metric", "Imperial", "None"]);
        });
        ui.separator();
        ui.heading("Icons");
        ui.label_dim("Alt+hover names one.");
        for chunk in Icon::ALL.chunks(8) {
            ui.row(|ui| {
                for icon in chunk {
                    let name = format!("{icon:?}");
                    ui.icon_button(&name, *icon, false, &name);
                }
            });
        }
    });
}

fn text(ui: &mut Ui, g: &mut GalleryState) {
    ui.heading("Text field");
    ui.text_field("name", &mut g.text);
    ui.label_dim(&format!("{} bytes, {} chars", g.text.len(), g.text.chars().count()));
    ui.row(|ui| {
        ui.text_field_hint("search", &mut g.search, "A placeholder, until you type…");
    });
    ui.row(|ui| {
        ui.password_field("secret", &mut g.secret, "A password");
        ui.label_dim(&format!("{} chars", g.secret.chars().count()));
    });
    ui.combo("font", &mut g.font, &FONTS, "Type or pick a font");
    let r = ui.text_field_validated("port", &mut g.port, &|s| match s.trim().parse::<u32>() {
        Ok(1..=65535) => None,
        Ok(_) => Some("1 to 65535".to_owned()),
        Err(_) => Some("a port number".to_owned()),
    });
    ui.label_dim(if r.invalid { "Enter commits nothing until the port passes." } else { "A port: 1 to 65535. Try letters." });
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
    ui.heading("Rich text");
    let r = ui.rich_text(RICH_SAMPLE);
    if let Some(link) = r.link_clicked {
        g.last_link = link;
    }
    ui.label_dim(&if g.last_link.is_empty() { "Click a link.".to_owned() } else { format!("Last link: {}", g.last_link) });
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

/// What `Ui::rich_text` renders: the whole small markdown in one go.
const RICH_SAMPLE: &str = "## A small markdown\n\
Paragraphs wrap at the width, with **bold**, *italic* and `code` runs, and a link to [the docs](docs/ARCHITECTURE.md). \
A second line of the same paragraph joins it.\n\n\
- Bullets, each its own item\n\
- With **bold** inside, too\n\n\
1. Numbered\n\
2. In order\n\n\
```\nlet shell = Shell::new(Editor::Main);\nlntrn_app::run(config, host, shell);\n```\n\n\
---\n\
Unclosed *markers stay visible, and a lone 2 * 3 keeps its stars.";
