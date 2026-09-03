//! An editable dropdown: type freely, or pick from the options that match
//! what is typed so far.

use lntrn_math::{Rect, Vec2};

use crate::event::Key;
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};
use crate::widgets::text_field::{TextOpts, TextResponse};

impl Ui<'_> {
    /// A text field with a button that lists `options`; typing narrows the
    /// list to the ones containing the text, Down opens it, a pick fills
    /// the field. `changed` is set on a pick as well as on typing.
    pub fn combo(&mut self, label: &str, value: &mut String, options: &[&str], placeholder: &str) -> TextResponse {
        let id = self.id(label);
        let m = self.m;
        let w = if self.in_row() { m.px(260.0) } else { FILL };
        let rect = self.alloc(Vec2::new(w, m.widget_h));
        let (field, button) = rect.split_x(rect.max.x - m.widget_h);
        // Down opens the list; the field would otherwise swallow the key.
        let open_key = self.state.has_focus(id) && self.state.take_key(|k| k.key == Key::ArrowDown && k.mods.is_empty()).is_some();
        let mut out = self.text_edit_core_with(id, field, value, TextOpts { placeholder, password: false });

        let open_now = *self.state.open(id);
        let b = self.interact(id.with("open"), button, Sense::CLICK);
        if b.hovered {
            self.state.cursor_icon = CursorIcon::Pointer;
        }
        let base = if open_now { self.theme.shade(self.theme.widget) } else { self.widget_color(&b) };
        self.raised(button, base, b.held || open_now);
        self.draw_chevron(Rect::from_center_size(button.center(), Vec2::new(m.pad * 2.0 + m.px(12.0), button.height())));
        if b.clicked || open_key {
            *self.state.open(id) = open_key || !open_now;
            self.state.focus = Some(id);
            self.state.request_rebuild = true;
        }
        if *self.state.open(id) {
            let needle = value.to_lowercase();
            let matches: Vec<&str> = options.iter().copied().filter(|o| needle.is_empty() || o.to_lowercase().contains(&needle)).collect();
            if matches.is_empty() {
                *self.state.open(id) = false;
            } else {
                let res = self.popup_list(id, rect, &matches, None);
                if let Some(i) = res.picked {
                    *value = matches[i].to_owned();
                    out.changed = true;
                    *self.state.open(id) = false;
                    let te = self.state.text_edit(id);
                    te.cursor = value.len();
                    te.anchor = value.len();
                    self.state.focus = Some(id);
                }
                if res.closed {
                    *self.state.open(id) = false;
                }
            }
        }
        out
    }
}
