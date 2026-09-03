//! Preferences: a `props!` struct and the auto-generated panel over it.

use lntrn_props::props;

use crate::theme::Theme;
use crate::ui::Ui;

props! {
    /// Preferences the shell owns. A host keeps its own settings struct and
    /// can show it next to this one in its Preferences editor.
    pub struct Prefs {
        /// Multiplies the window's scale factor.
        pub ui_scale: f64 = 1.0 => { id: 1, label: "UI Scale", hard: 0.5..=3.0, soft: 0.75..=2.0, step: 0.05 },
        /// Keyboard goes to the area under the pointer instead of the last-clicked one.
        pub focus_follows_mouse: bool = false => { id: 2 },
        pub theme: Theme = Theme::default() => { id: 3 },
    }
}

/// Returns `true` when anything changed.
pub fn draw(ui: &mut Ui, prefs: &mut Prefs) -> bool {
    let mut changed = false;
    ui.scroll_area("prefs", None, |ui| {
        changed = ui.props_panel(prefs);
        ui.space(ui.m.gap);
        if ui.button("Reset Theme").clicked {
            prefs.theme = Theme::default();
            changed = true;
        }
    });
    changed
}
