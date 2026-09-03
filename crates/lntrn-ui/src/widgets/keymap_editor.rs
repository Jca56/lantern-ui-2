//! A keymap editor: every binding of a [`KeyConfig`] grouped by map, with
//! press-to-rebind, editable action ids, remove and add.

use lntrn_math::{Rect, Vec2};

use crate::event::{Key, Modifiers};
use crate::keymap::{KeyConfig, KeyItem, Trigger};
use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

impl Ui<'_> {
    /// Show and edit `keys`. Click a trigger, then press the new key (Esc
    /// keeps the old one). Returns `true` when anything changed.
    pub fn keymap_editor(&mut self, label: &str, keys: &mut KeyConfig) -> bool {
        let id = self.id(label);
        let mut changed = false;
        // Which binding is waiting for a key: [map, item] or -1.
        let listening = *self.state.floats(id, [-1.0; 4]);
        let mut waiting = if listening[0] >= 0.0 { Some((listening[0] as usize, listening[1] as usize)) } else { None };
        if let Some((mi, ii)) = waiting
            && let Some(k) = self.state.take_key(|k| !k.key.is_modifier())
        {
            if k.key != Key::Escape
                && let Some(item) = keys.maps.get_mut(mi).and_then(|m| m.items.get_mut(ii))
            {
                item.trigger = Trigger::key(k.key, k.mods);
                changed = true;
            }
            waiting = None;
            self.state.request_rebuild = true;
        }
        let trigger_w = self.m.px(230.0);
        self.push_id(label);
        for mi in 0..keys.maps.len() {
            self.push_index(mi);
            let name = keys.maps[mi].name.clone();
            self.collapsing(&name, |ui| {
                let mut remove = None;
                let map = &mut keys.maps[mi];
                for ii in 0..map.items.len() {
                    ui.push_index(ii);
                    let is_waiting = waiting == Some((mi, ii));
                    ui.row(|ui| {
                        let tid = ui.id("trigger");
                        let rect = ui.alloc(Vec2::new(trigger_w, ui.m.widget_h));
                        let mut r = ui.interact(tid, rect, Sense::CLICK);
                        ui.focusable(tid, rect);
                        ui.key_click(tid, &mut r);
                        if r.hovered {
                            ui.state.cursor_icon = CursorIcon::Pointer;
                        }
                        let style = ui.text_style();
                        if is_waiting {
                            ui.raised(rect, ui.theme.accent, false);
                            ui.text_centered("Press a key…", &style, rect, ui.theme.accent_text);
                        } else {
                            ui.button_face(rect, &r);
                            ui.text_centered(&map.items[ii].trigger.label(), &style, rect, ui.theme.text);
                        }
                        ui.focus_ring(tid, rect);
                        if r.clicked && !is_waiting {
                            waiting = Some((mi, ii));
                            ui.state.request_rebuild = true;
                        }
                        let field_w = (ui.avail_width() - ui.m.widget_h - ui.m.gap).max(ui.m.px(120.0));
                        let frect = ui.alloc(Vec2::new(field_w, ui.m.widget_h));
                        if ui.text_edit_core(ui.id("action"), frect, &mut map.items[ii].op).changed {
                            changed = true;
                        }
                        if ui.button("−").clicked {
                            remove = Some(ii);
                        }
                    });
                    ui.pop_id();
                }
                if let Some(i) = remove {
                    map.items.remove(i);
                    changed = true;
                }
                if ui.button("+ Add binding").clicked {
                    map.items.push(KeyItem::new(Trigger::key(Key::Unknown, Modifiers::NONE), "action.id"));
                    changed = true;
                }
            });
            self.pop_id();
        }
        self.pop_id();
        if keys.maps.is_empty() {
            let r = self.alloc(Vec2::new(FILL, self.m.widget_h));
            let style = self.text_style();
            self.text_in_rect("No key maps.", &style, Rect::new(r.min, r.max), self.theme.text_dim);
        }
        *self.state.floats(id, [-1.0; 4]) = match waiting {
            Some((mi, ii)) => [mi as f64, ii as f64, 0.0, 0.0],
            None => [-1.0; 4],
        };
        changed
    }
}
