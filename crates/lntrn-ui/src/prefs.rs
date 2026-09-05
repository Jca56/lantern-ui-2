//! Preferences: a `props!` struct and the auto-generated panel over it,
//! headed by the theme chooser: presets and saved themes in a dropdown,
//! a name to save the current look under, and a way to delete one.

use lntrn_props::props;

use crate::theme::Theme;
use crate::themes;
use crate::ui::Ui;

props! {
    /// Preferences the shell owns. A host keeps its own settings struct and
    /// can show it next to this one in its Preferences editor.
    pub struct Prefs {
        /// Multiplies the window's scale factor.
        pub ui_scale: f64 = 1.0 => { id: 1, label: "UI Scale", hard: 0.5..=3.0, soft: 0.75..=2.0, step: 0.05 },
        /// Keyboard goes to the area under the pointer instead of the last-clicked one.
        pub focus_follows_mouse: bool = false => { id: 2 },
        /// No easing, fades or sweeps: things go straight to where they are going.
        pub reduce_motion: bool = false => { id: 4 },
        /// What the last rebuild cost and what the caches hold, in the corner.
        pub debug_overlay: bool = false => { id: 5 },
        pub theme: Theme = Theme::default() => { id: 3 },
        /// The preset or saved theme the look came from.
        pub theme_name: String = String::new() => { id: 6, flags: HIDDEN },
        /// What the Save As field holds; never written to disk.
        pub save_name: String = String::new() => { id: 7, flags: HIDDEN | SKIP_SAVE },
    }
}

/// The name for a look that came from nowhere: a saved theme so it is
/// never lost.
const CUSTOM: &str = "Custom";

/// Returns `true` when anything changed.
pub fn draw(ui: &mut Ui, prefs: &mut Prefs) -> bool {
    let mut changed = false;
    ui.scroll_area("prefs", None, |ui| {
        ui.heading("Look");
        changed |= theme_chooser(ui, prefs);
        ui.space(ui.m.gap);
        changed |= ui.props_panel(prefs);
    });
    changed
}

/// The dropdown of themes, Save As, Delete. A look that matches no
/// theme is saved as `Custom` the first time it shows, so nothing made
/// before themes existed disappears.
fn theme_chooser(ui: &mut Ui, prefs: &mut Prefs) -> bool {
    let mut changed = false;
    let saved = themes::list();
    let mut names: Vec<String> = Theme::PRESETS.iter().map(|(n, _)| (*n).to_owned()).collect();
    let extra: Vec<String> = saved.iter().filter(|s| !names.contains(s)).cloned().collect();
    names.extend(extra);
    if prefs.theme_name.is_empty() {
        prefs.theme_name = match themes::preset_name(&prefs.theme) {
            Some(n) => n.to_owned(),
            None => {
                if themes::save(CUSTOM, &prefs.theme).is_ok() && !names.iter().any(|n| n == CUSTOM) {
                    names.push(CUSTOM.to_owned());
                }
                CUSTOM.to_owned()
            }
        };
        changed = true;
    }
    let current = names.iter().position(|n| *n == prefs.theme_name);
    let modified = current.is_some_and(|i| themes::named(&names[i]).is_none_or(|t| t != prefs.theme));
    let label = if modified { "Theme (modified)" } else { "Theme" };
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut idx = current.unwrap_or(0);
    ui.labelled(label, |ui| {
        if ui.dropdown("theme", &mut idx, &refs)
            && let Some(t) = themes::named(&names[idx])
        {
            prefs.theme = t;
            prefs.theme_name = names[idx].clone();
            changed = true;
        }
    });
    ui.labelled("Save as", |ui| {
        ui.row(|ui| {
            let r = ui.text_field_hint("save-name", &mut prefs.save_name, "A name for this look");
            let name = prefs.save_name.trim().to_owned();
            if (ui.button("Save").clicked || r.committed) && !name.is_empty() {
                match themes::save(&name, &prefs.theme) {
                    Ok(()) => {
                        prefs.theme_name = name;
                        prefs.save_name.clear();
                        changed = true;
                    }
                    Err(e) => lntrn_core::log_error!("saving theme: {e}"),
                }
            }
        });
    });
    if saved.contains(&prefs.theme_name) {
        ui.labelled("", |ui| {
            if ui.button("Delete this theme").clicked {
                if let Err(e) = themes::delete(&prefs.theme_name) {
                    lntrn_core::log_error!("deleting theme: {e}");
                }
                // The look stays; it just has no name until it is saved again.
                prefs.theme_name = themes::preset_name(&prefs.theme).map(str::to_owned).unwrap_or_default();
                if prefs.theme_name.is_empty() {
                    prefs.theme_name = CUSTOM.to_owned();
                    let _ = themes::save(CUSTOM, &prefs.theme);
                }
                changed = true;
            }
        });
    }
    changed
}
