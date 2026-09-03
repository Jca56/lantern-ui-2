//! Level meters and waveforms: what an audio app looks at all day. Levels
//! are `0..=1`; the caller maps decibels however it likes.

use lntrn_math::{Color, Rect, Vec2};

use crate::ui::{FILL, Sense, Ui};

/// Meter colours: quiet, loud, too loud.
const GREEN: Color = Color::hex(0x3DBB5E);
const YELLOW: Color = Color::hex(0xE8C33A);
const RED: Color = Color::hex(0xE0473A);
/// Where green turns yellow and yellow turns red.
const LOUD: f64 = 0.7;
const HOT: f64 = 0.9;

impl Ui<'_> {
    /// Vertical level meters, one per channel: `(level, peak)` each in
    /// `0..=1`. Green, yellow past 0.7, red past 0.9, a held peak line.
    /// `height` physical pixels; the label sits above.
    pub fn level_meter(&mut self, label: &str, channels: &[(f64, f64)], height: f64) {
        let m = self.m;
        let style = self.text_style();
        let line = style.line_height() as f64 + m.gap;
        let bar_w = m.px(20.0);
        let n = channels.len().max(1) as f64;
        let total_w = n * bar_w + (n - 1.0) * m.gap;
        let w = if self.in_row() { total_w.max(self.measure(label, &style) + m.pad * 2.0) } else { FILL };
        let rect = self.alloc(Vec2::new(w, line + height.max(m.widget_h)));
        let label_rect = Rect::new(rect.min, Vec2::new(rect.max.x, rect.min.y + line));
        let x0 = (rect.center().x - total_w * 0.5).round();
        for (i, &(level, peak)) in channels.iter().enumerate() {
            let track = Rect::from_min_size(Vec2::new(x0 + i as f64 * (bar_w + m.gap), label_rect.max.y), Vec2::new(bar_w, rect.max.y - label_rect.max.y));
            self.recessed(track, self.theme.field);
            let inner = track.shrink(m.border);
            let y_of = |t: f64| inner.max.y - inner.height() * t.clamp(0.0, 1.0);
            let level = level.clamp(0.0, 1.0);
            for (from, to, color) in [(0.0, LOUD, GREEN), (LOUD, HOT, YELLOW), (HOT, 1.0, RED)] {
                if level > from {
                    self.draw.rect(Rect::new(Vec2::new(inner.min.x, y_of(level.min(to))), Vec2::new(inner.max.x, y_of(from))), color);
                }
            }
            if peak > 0.0 {
                let y = y_of(peak);
                let ink = if peak > HOT { RED } else { self.theme.text };
                self.draw.rect(Rect::new(Vec2::new(inner.min.x, y - m.px(1.0)), Vec2::new(inner.max.x, y + m.px(1.0))), ink);
            }
        }
        self.text_centered(label, &style, label_rect, self.theme.text_dim);
    }

    /// `samples` in `-1..=1` as a filled envelope (the lowest and highest
    /// sample of every pixel column), `height` tall, with a playhead line
    /// `playhead` of the way across when given. Returns where a click
    /// landed, `0..=1` across.
    pub fn waveform(&mut self, label: &str, samples: &[f32], height: f64, playhead: Option<f64>) -> Option<f64> {
        let id = self.id(label);
        let m = self.m;
        let rect = self.alloc(Vec2::new(FILL, height.max(m.widget_h)));
        let r = self.interact(id, rect, Sense::CLICK);
        self.recessed(rect, self.theme.field);
        let inner = rect.shrink(m.border);
        let mid = inner.center().y.round();
        self.draw.hline(inner.min.x, inner.max.x, mid, m.border, self.theme.border_light.fade(0.3));
        let cols = inner.width().max(0.0) as usize;
        let n = samples.len();
        if n > 0 && cols > 0 {
            let half = (inner.height() * 0.5 - m.px(1.0)).max(1.0);
            for c in 0..cols {
                let s0 = c * n / cols;
                let s1 = ((c + 1) * n / cols).clamp(s0 + 1, n);
                let (lo, hi) = samples[s0..s1].iter().fold((1.0f32, -1.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
                let x = inner.min.x + c as f64 + 0.5;
                let (top, bottom) = (mid - f64::from(hi.clamp(-1.0, 1.0)) * half, mid - f64::from(lo.clamp(-1.0, 1.0)) * half);
                self.draw.vline(x, top.min(bottom), bottom.max(top + 1.0), 1.0, self.theme.accent);
            }
        }
        if let Some(t) = playhead {
            let x = (inner.min.x + inner.width() * t.clamp(0.0, 1.0)).round();
            self.draw.vline(x, inner.min.y, inner.max.y, m.px(2.0), self.theme.text);
        }
        if r.clicked {
            // The caller moves the playhead after this frame drew it.
            self.state.request_rebuild = true;
        }
        r.clicked.then(|| ((self.state.pointer.x - inner.min.x) / inner.width().max(1.0)).clamp(0.0, 1.0))
    }
}
