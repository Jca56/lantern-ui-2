//! The debug overlay: what the last rebuild cost and what the caches hold,
//! in the corner when the `debug_overlay` preference is on. It asks for no
//! frames of its own, so an idle app stays idle.

use lntrn_math::{Rect, Vec2};
use lntrn_render::DrawList;
use lntrn_text::TextEngine;

use crate::host::Host;
use crate::id::WidgetId;
use crate::shell::Shell;
use crate::theme::{Metrics, Theme};
use crate::ui::Ui;

/// What the shell measured about the last rebuild.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// How long the last rebuild took, in milliseconds (the CPU side; the
    /// GPU draws after).
    pub rebuild_ms: f64,
    /// Vertices the last rebuild drew.
    pub vertices: usize,
    /// Rebuilds since the shell was made.
    pub frames: u64,
}

/// Above toasts.
const LAYER: usize = 4;

impl<H: Host> Shell<H> {
    /// What the last rebuild cost.
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    pub(crate) fn draw_debug(&mut self, draw: &mut DrawList, text: &mut TextEngine, theme: &Theme, m: Metrics, window: Rect) {
        let (hits, misses) = text.cache_stats();
        let total = hits + misses;
        let rate = if total == 0 { 100.0 } else { hits as f64 * 100.0 / total as f64 };
        let atlas = text.atlas();
        let lines = [
            format!("rebuild {:.2} ms", self.stats.rebuild_ms),
            format!("{} vertices", self.stats.vertices),
            format!("atlas {}², {} glyphs", atlas.size(), atlas.len()),
            format!("{} layouts cached, {rate:.0}% hits", text.cached_layouts()),
            format!("{} rebuilds", self.stats.frames),
        ];
        let mut ui = Ui::new(draw, text, theme, m, &mut self.state, window, window, WidgetId::ROOT.with("debug"), LAYER);
        let style = ui.mono_style();
        let lh = style.line_height() as f64;
        let w = lines.iter().map(|l| ui.measure(l, &style)).fold(0.0, f64::max) + m.pad * 2.0;
        let h = lh * lines.len() as f64 + m.pad * 2.0;
        let rect = Rect::from_min_size(Vec2::new(window.max.x - m.pad - w, window.min.y + m.header_h + m.pad), Vec2::new(w, h));
        ui.floating_panel(rect, theme.header.map(|c| c.fade(0.92)));
        ui.draw.stroke_rect(rect, m.border, m.radius, theme.focus);
        for (i, line) in lines.iter().enumerate() {
            ui.text_at(line, &style, Vec2::new(rect.min.x + m.pad, rect.min.y + m.pad + i as f64 * lh), 1.0e6, theme.text);
        }
        ui.finish();
    }
}
