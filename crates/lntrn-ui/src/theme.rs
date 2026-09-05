//! Theme: colors and logical sizes, described with `props!` so the
//! Preferences editor edits it live. Sizes are logical pixels in multiples of
//! five; [`Metrics`] is the theme scaled to physical pixels for one frame.
//!
//! The look: near-black panels with a little vertical gradient, raised
//! controls (light top edge, dark bottom edge), recessed fields, a bright
//! amber accent for "on", and blue for selection and focus. The title
//! bar, headers, bodies and raised controls are each a [`Gradient`]: two
//! colors, top and bottom, the same one twice for flat. `gradient` is
//! the depth knob for everything else that is shaded from one color
//! (accent buttons, thumbs, bevels); zero is flat.

use lntrn_math::Color;
use lntrn_props::{Gradient, props};

props! {
    /// Colors and sizes of the editor UI.
    pub struct Theme {
        /// Window background, gaps between areas.
        pub bg: Color = Color::hex(0x0F0F11) => { id: 1 },
        /// The title bar, top to bottom.
        pub title: Gradient = Gradient::shaded(Color::hex(0x2B2B2F), 0.12) => { id: 16, label: "Title Bar" },
        /// Area header bars, top to bottom.
        pub header: Gradient = Gradient::shaded(Color::hex(0x2B2B2F), 0.12) => { id: 2 },
        /// Area bodies, top to bottom.
        pub panel: Gradient = Gradient::shaded(Color::hex(0x1B1B1E), 0.12) => { id: 3 },
        /// Raised controls: buttons, tabs, thumbs.
        pub widget: Gradient = Gradient::shaded(Color::hex(0x37373C), 0.12) => { id: 4 },
        /// Recessed controls: fields, sliders tracks, the viewport well.
        pub field: Color = Color::hex(0x131315) => { id: 5 },
        pub text: Color = Color::hex(0xF2F2F4) => { id: 6 },
        pub text_dim: Color = Color::hex(0xA0A0A8) => { id: 7 },
        /// "On": active tabs, checked toggles, slider fill.
        pub accent: Color = Color::hex(0xFFB733) => { id: 8 },
        /// Text on top of `accent`.
        pub accent_text: Color = Color::hex(0x1A1508) => { id: 9 },
        /// Selected rows and items.
        pub selection: Color = Color::hex(0x4A82DC) => { id: 10 },
        pub selection_text: Color = Color::hex(0xFFFFFF) => { id: 11 },
        /// Outline of the focused area and focused field.
        pub focus: Color = Color::hex(0x6EA6FF) => { id: 12 },
        /// The close button while hovered.
        pub close: Color = Color::hex(0xE0473A) => { id: 13 },
        /// Dark lines: panel edges, control outlines.
        pub border_dark: Color = Color::hex(0x060608) => { id: 14 },
        /// Light lines: bevels, etched separators.
        pub border_light: Color = Color::hex(0x4E4E55) => { id: 15 },
        /// Strength of the vertical shading on headers and controls. 0 = flat.
        pub gradient: f64 = 0.12 => { id: 20, hard: 0.0..=0.4, subtype: Factor },
        /// Width of every line: outlines, bevels, separators. 0 = none.
        pub border_width: f64 = 1.0 => { id: 40, hard: 0.0..=6.0, step: 1.0, subtype: Pixels },

        /// Body text size.
        pub text_size: f64 = 25.0 => { id: 30, hard: 10.0..=80.0, step: 5.0, subtype: Pixels },
        pub heading_size: f64 = 30.0 => { id: 31, hard: 10.0..=100.0, step: 5.0, subtype: Pixels },
        pub header_height: f64 = 45.0 => { id: 32, hard: 20.0..=120.0, step: 5.0, subtype: Pixels },
        /// Height of buttons, fields, sliders.
        pub widget_height: f64 = 45.0 => { id: 33, hard: 20.0..=120.0, step: 5.0, subtype: Pixels },
        /// Inner padding of widgets and panels.
        pub padding: f64 = 10.0 => { id: 34, hard: 0.0..=40.0, step: 5.0, subtype: Pixels },
        /// Space between widgets.
        pub gap: f64 = 5.0 => { id: 35, hard: 0.0..=40.0, step: 5.0, subtype: Pixels },
        pub radius: f64 = 5.0 => { id: 36, hard: 0.0..=30.0, step: 5.0, subtype: Pixels },
        /// Width of the label column in property panels.
        pub label_width: f64 = 200.0 => { id: 37, hard: 50.0..=500.0, step: 10.0, subtype: Pixels },
        pub scrollbar_width: f64 = 15.0 => { id: 38, hard: 5.0..=40.0, step: 5.0, subtype: Pixels },
        /// Gap between areas; also the drag handle.
        pub separator: f64 = 5.0 => { id: 39, hard: 0.0..=20.0, step: 5.0, subtype: Pixels },
    }
}

/// Makes a theme preset.
pub type ThemeMaker = fn() -> Theme;

impl Theme {
    /// The built-in looks, by name: what the Preferences editor offers.
    pub const PRESETS: [(&'static str, ThemeMaker); 6] = [
        ("Dark", <Theme as Default>::default),
        ("Ember", Theme::ember),
        ("Nightfall", Theme::nightfall),
        ("Moss", Theme::moss),
        ("Light", Theme::light),
        ("High Contrast", Theme::high_contrast),
    ];

    /// Warm near-black: coffee-brown surfaces, cream ink, the amber
    /// accent, a warm selection instead of the blue.
    pub fn ember() -> Theme {
        Theme {
            bg: Color::hex(0x0B0A09),
            title: Gradient::new(Color::hex(0x2C251C), Color::hex(0x1C1712)),
            header: Gradient::new(Color::hex(0x2A231A), Color::hex(0x1A1611)),
            panel: Gradient::new(Color::hex(0x14120F), Color::hex(0x100E0C)),
            widget: Gradient::new(Color::hex(0x3C3227), Color::hex(0x2A231B)),
            field: Color::hex(0x0D0C0B),
            text: Color::hex(0xF4EBDC),
            text_dim: Color::hex(0xB3A48C),
            accent: Color::hex(0xFFB733),
            accent_text: Color::hex(0x1F1606),
            selection: Color::hex(0x8B5A16),
            selection_text: Color::hex(0xFFF3DD),
            focus: Color::hex(0xFFC85A),
            close: Color::hex(0xE0473A),
            border_dark: Color::hex(0x070605),
            border_light: Color::hex(0x4E4436),
            ..Theme::default()
        }
    }

    /// Cool blue-black: slate surfaces, sky accent, blue selection.
    pub fn nightfall() -> Theme {
        Theme {
            bg: Color::hex(0x0A0C11),
            title: Gradient::new(Color::hex(0x1E2532), Color::hex(0x141A24)),
            header: Gradient::new(Color::hex(0x222A39), Color::hex(0x171D29)),
            panel: Gradient::new(Color::hex(0x121721), Color::hex(0x0E121A)),
            widget: Gradient::new(Color::hex(0x303A4C), Color::hex(0x232B3A)),
            field: Color::hex(0x0B0F16),
            text: Color::hex(0xE8EEF7),
            text_dim: Color::hex(0x93A0B6),
            accent: Color::hex(0x62BBFF),
            accent_text: Color::hex(0x061320),
            selection: Color::hex(0x3A6BD1),
            selection_text: Color::hex(0xFFFFFF),
            focus: Color::hex(0x8CCBFF),
            close: Color::hex(0xE0473A),
            border_dark: Color::hex(0x05070A),
            border_light: Color::hex(0x46516A),
            ..Theme::default()
        }
    }

    /// Green-grey: mossy surfaces, leaf accent, calm on the eyes.
    pub fn moss() -> Theme {
        Theme {
            bg: Color::hex(0x0C0E0B),
            title: Gradient::new(Color::hex(0x232A20), Color::hex(0x171C15)),
            header: Gradient::new(Color::hex(0x262E22), Color::hex(0x1A2017)),
            panel: Gradient::new(Color::hex(0x151913), Color::hex(0x11140F)),
            widget: Gradient::new(Color::hex(0x36402F), Color::hex(0x272E23)),
            field: Color::hex(0x0E110D),
            text: Color::hex(0xEAF0E1),
            text_dim: Color::hex(0xA1AC94),
            accent: Color::hex(0x9CD46A),
            accent_text: Color::hex(0x0E1707),
            selection: Color::hex(0x4A7A2C),
            selection_text: Color::hex(0xF2FFE6),
            focus: Color::hex(0xBEEB92),
            close: Color::hex(0xE0473A),
            border_dark: Color::hex(0x060805),
            border_light: Color::hex(0x46523E),
            ..Theme::default()
        }
    }

    /// Paper panels and dark ink, the same amber accent.
    pub fn light() -> Theme {
        Theme {
            bg: Color::hex(0xD6D6DB),
            title: Gradient::shaded(Color::hex(0xC3C3CA), 0.07),
            header: Gradient::shaded(Color::hex(0xC3C3CA), 0.07),
            panel: Gradient::shaded(Color::hex(0xEDEDF0), 0.07),
            widget: Gradient::shaded(Color::hex(0xDCDCE2), 0.07),
            field: Color::hex(0xFFFFFF),
            text: Color::hex(0x141418),
            text_dim: Color::hex(0x55555E),
            accent: Color::hex(0xE58A00),
            accent_text: Color::hex(0x1A1508),
            selection: Color::hex(0x3868C8),
            selection_text: Color::hex(0xFFFFFF),
            focus: Color::hex(0x2A5CC0),
            close: Color::hex(0xD9432F),
            border_dark: Color::hex(0x8E8E98),
            border_light: Color::hex(0xFFFFFF),
            gradient: 0.07,
            ..Theme::default()
        }
    }

    /// Black and white, hard edges, bigger text.
    pub fn high_contrast() -> Theme {
        Theme {
            bg: Color::BLACK,
            title: Gradient::flat(Color::hex(0x101010)),
            header: Gradient::flat(Color::hex(0x101010)),
            panel: Gradient::flat(Color::BLACK),
            widget: Gradient::flat(Color::hex(0x222222)),
            field: Color::hex(0x060606),
            text: Color::WHITE,
            text_dim: Color::hex(0xD8D8D8),
            accent: Color::hex(0xFFD400),
            accent_text: Color::BLACK,
            selection: Color::hex(0x00A8FF),
            selection_text: Color::BLACK,
            focus: Color::WHITE,
            close: Color::hex(0xFF3B2F),
            border_dark: Color::hex(0x8A8A8A),
            border_light: Color::WHITE,
            gradient: 0.0,
            text_size: 30.0,
            heading_size: 35.0,
            widget_height: 55.0,
            header_height: 55.0,
            ..Theme::default()
        }
    }

    /// Lighter end of a shaded control.
    pub fn top(&self, base: Color) -> Color {
        base.scale_rgb(1.0 + self.gradient)
    }

    /// Darker end of a shaded control.
    pub fn bottom(&self, base: Color) -> Color {
        base.scale_rgb(1.0 - self.gradient)
    }

    /// The bright edge along the top of a raised control.
    pub fn highlight(&self, base: Color) -> Color {
        base.scale_rgb(1.0 + self.gradient * 3.0)
    }

    /// The dark edge along the bottom of a raised control (and the inner
    /// shadow at the top of a recessed one).
    pub fn shade(&self, base: Color) -> Color {
        base.scale_rgb(1.0 - self.gradient * 3.5)
    }

    /// A hovered control: a little brighter.
    pub fn hover(&self, base: Color) -> Color {
        base.scale_rgb(1.14)
    }

    /// A hovered surface: both ends a little brighter.
    pub fn hover_g(&self, g: Gradient) -> Gradient {
        g.map(|c| self.hover(c))
    }

    /// One color as a surface: lighter above, darker below by `gradient`.
    pub fn shaded(&self, base: Color) -> Gradient {
        Gradient::shaded(base, self.gradient)
    }
}

/// The theme's sizes in physical pixels for the current scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub scale: f64,
    pub text_size: f32,
    pub heading_size: f32,
    pub header_h: f64,
    pub widget_h: f64,
    pub pad: f64,
    pub gap: f64,
    pub radius: f64,
    /// Outlines, separators and etched lines (`Theme::border_width`).
    pub border: f64,
    /// The bevel strokes on raised and recessed controls: always one pixel.
    pub bevel: f64,
    pub label_w: f64,
    pub scrollbar_w: f64,
    pub sep: f64,
    /// Half-width of the separator's grab zone on each side of the gap.
    pub sep_grab: f64,
    /// Width of the focused-area outline.
    pub focus_border: f64,
}

impl Theme {
    /// Scale to physical pixels. `scale` is window scale × UI scale.
    pub fn metrics(&self, scale: f64) -> Metrics {
        let px = |v: f64| (v * scale).round().max(1.0);
        Metrics {
            scale,
            text_size: (self.text_size * scale).round().max(4.0) as f32,
            heading_size: (self.heading_size * scale).round().max(4.0) as f32,
            header_h: px(self.header_height),
            widget_h: px(self.widget_height),
            pad: (self.padding * scale).round(),
            gap: (self.gap * scale).round(),
            radius: (self.radius * scale).round(),
            border: (self.border_width * scale).round(),
            bevel: px(1.0),
            label_w: px(self.label_width),
            scrollbar_w: px(self.scrollbar_width),
            sep: (self.separator * scale).round(),
            sep_grab: px(5.0),
            focus_border: px(2.0),
        }
    }
}

impl Metrics {
    /// Round a logical size to physical pixels.
    #[inline]
    pub fn px(&self, logical: f64) -> f64 {
        (logical * self.scale).round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lntrn_props::ReflectStatic;

    #[test]
    fn metrics_scale_and_round() {
        let t = Theme::default();
        let m = t.metrics(1.4);
        assert_eq!(m.widget_h, 63.0);
        assert_eq!(m.text_size, 35.0);
        assert_eq!(m.border, 1.0);
        assert_eq!(m.px(10.0), 14.0);
        let m1 = t.metrics(1.0);
        assert_eq!(m1.header_h, 45.0);
        assert_eq!(m1.gap, 5.0);
        let thick = Theme { border_width: 3.0, ..Theme::default() };
        assert_eq!(thick.metrics(1.4).border, 4.0);
        assert_eq!(thick.metrics(1.4).bevel, 1.0, "the bevel never thickens");
        assert_eq!(Theme { border_width: 0.0, ..Theme::default() }.metrics(2.0).border, 0.0, "no lines at all");
    }

    #[test]
    fn presets_are_distinct_and_readable() {
        let dark = Theme::default();
        let light = Theme::light();
        let hc = Theme::high_contrast();
        let lum = |c: Color| c.to_linear().luminance_linear();
        assert!(lum(light.panel.mid()) > 0.5 && lum(light.text) < 0.05, "light: dark ink on paper");
        assert!(lum(dark.panel.mid()) < 0.05 && lum(dark.text) > 0.8);
        assert!(hc.text_size > dark.text_size && hc.gradient == 0.0 && hc.header.is_flat());
        assert_eq!(Theme::PRESETS.len(), 6);
        assert_eq!((Theme::PRESETS[4].1)().panel, light.panel);
        assert_eq!((Theme::PRESETS[0].1)().panel, dark.panel);
        for (name, make) in Theme::PRESETS {
            let t = make();
            let dark_ui = lum(t.panel.mid()) < 0.5;
            assert!(if dark_ui { lum(t.text) > 0.6 } else { lum(t.text) < 0.1 }, "{name}: ink against its panels");
            assert!(t.header.top.to_linear().luminance_linear() >= t.header.bottom.to_linear().luminance_linear(), "{name}: headers light above");
            assert!((lum(t.accent) - lum(t.accent_text)).abs() > 0.3, "{name}: accent text reads");
        }
    }

    #[test]
    fn theme_is_reflected_and_shades() {
        let info = Theme::info();
        assert_eq!(info.field("accent").unwrap().id, 8);
        assert!(info.field("text_size").unwrap().hard.is_some());
        assert_eq!(info.fields.len(), 28);
        assert_eq!(info.field("header").unwrap().kind, lntrn_props::Kind::Gradient);
        let t = Theme::default();
        assert!(t.widget.top.r > t.widget.mid().r && t.widget.bottom.r < t.widget.mid().r, "surfaces are lighter above");
        assert!(t.top(t.accent).r >= t.accent.r && t.bottom(t.accent).r < t.accent.r);
        assert!(t.highlight(t.field).r > t.top(t.field).r);
        assert_eq!(t.shaded(t.accent), Gradient::new(t.top(t.accent), t.bottom(t.accent)));
        assert!(t.hover_g(t.header).top.r > t.header.top.r);
        let flat = Theme { gradient: 0.0, ..Theme::default() };
        assert_eq!(flat.top(flat.field), flat.field);
        assert!(flat.shaded(flat.accent).is_flat());
    }
}
