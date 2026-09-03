//! Toasts: short messages stacked in the bottom-right corner that fade out
//! on their own. Asked for with [`crate::ShellRequest::Toast`].

use lntrn_math::{Rect, Vec2};
use lntrn_render::DrawList;
use lntrn_text::TextEngine;

use crate::host::Host;
use crate::id::WidgetId;
use crate::shell::Shell;
use crate::theme::{Metrics, Theme};
use crate::ui::Ui;

/// How long a toast stays, and how long its fade at the end takes.
const LIFE: f64 = 4.0;
const FADE: f64 = 0.6;
/// Toasts draw above tooltips.
const LAYER: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    /// When it appeared, on the shell clock.
    pub at: f64,
}

impl<H: Host> Shell<H> {
    /// The toasts on screen, oldest first.
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    /// Drop expired toasts, draw the rest, and keep frames coming while
    /// any is fading.
    pub(crate) fn draw_toasts(&mut self, draw: &mut DrawList, text: &mut TextEngine, theme: &Theme, m: Metrics, window: Rect) {
        let now = self.state.now;
        self.toasts.retain(|t| now - t.at < LIFE);
        if self.toasts.is_empty() {
            return;
        }
        let mut ui = Ui::new(draw, text, theme, m, &mut self.state, window, window, WidgetId::ROOT.with("toasts"), LAYER);
        let style = ui.text_style();
        let max_w = m.px(600.0).min(window.width() - m.pad * 2.0);
        let mut y = window.max.y - m.pad;
        let mut soonest = f64::MAX;
        for t in self.toasts.iter().rev() {
            let remaining = LIFE - (now - t.at);
            let alpha = (remaining / FADE).clamp(0.0, 1.0);
            soonest = soonest.min(if remaining <= FADE { 1.0 / 60.0 } else { remaining - FADE });
            let w = (ui.measure(&t.text, &style) + m.pad * 2.0).min(max_w);
            let rect = Rect::from_min_size(Vec2::new(window.max.x - m.pad - w, y - m.widget_h), Vec2::new(w, m.widget_h));
            ui.floating_panel(rect, theme.header.fade(alpha));
            ui.draw.stroke_rect(rect, m.border, m.radius, theme.accent.fade(alpha));
            let inner = Rect::new(Vec2::new(rect.min.x + m.pad, rect.min.y), Vec2::new(rect.max.x - m.pad, rect.max.y));
            ui.text_in_rect(&t.text, &style, inner, theme.text.fade(alpha));
            y = rect.min.y - m.gap;
        }
        ui.finish();
        if soonest < f64::MAX {
            self.state.request_redraw_after(soonest);
        }
    }
}
