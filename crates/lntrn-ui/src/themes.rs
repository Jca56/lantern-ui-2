//! Named themes: the built-in presets, and the user's own saved as small
//! text files (`name.theme`, one `field = value` per line, colors as
//! `#RRGGBB`, gradients as two of them) in a folder every Lantern UI app
//! shares, so a look made in one app is there in the next.

use std::path::{Path, PathBuf};

use lntrn_math::Color;
use lntrn_props::{Gradient, Kind, Reflect, ReflectStatic, Value, walk};

use crate::persist;
use crate::theme::Theme;

/// The folder the saved themes live in.
pub fn dir() -> Option<PathBuf> {
    let d = persist::config_dir("lantern-ui")?.join("themes");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

/// A theme as text: one leaf per line.
pub fn to_text(theme: &Theme) -> String {
    let mut out = String::from("# Lantern UI theme\n");
    walk::walk(theme, &mut |path, _field, value| {
        let v = match value {
            Value::Color(c) => c.to_hex_string(),
            Value::Gradient(g) => format!("{} {}", g.top.to_hex_string(), g.bottom.to_hex_string()),
            Value::F64(x) => format!("{x}"),
            Value::I64(x) => format!("{x}"),
            Value::Bool(b) => format!("{b}"),
            Value::Str(s) => s,
            other => format!("{other}"),
        };
        out.push_str(&format!("{path} = {v}\n"));
    });
    out
}

/// A theme from text: the defaults with every line that parses applied
/// over them. Unknown fields and bad values are skipped.
pub fn from_text(text: &str) -> Theme {
    let mut theme = Theme::default();
    let info = Theme::info();
    let target: &mut dyn Reflect = &mut theme;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let (key, raw) = (key.trim(), raw.trim());
        let Some(field) = info.field(key) else {
            continue;
        };
        let value = match &field.kind {
            Kind::Color => Color::parse_hex(raw).map(Value::Color),
            Kind::Gradient => {
                let mut parts = raw.split_whitespace();
                let top = parts.next().and_then(Color::parse_hex);
                let bottom = parts.next().and_then(Color::parse_hex).or(top);
                top.zip(bottom).map(|(t, b)| Value::Gradient(Gradient::new(t, b)))
            }
            Kind::F64 => raw.parse::<f64>().ok().map(|x| Value::F64(field.clamp(x))),
            Kind::I64 => raw.parse::<i64>().ok().map(Value::I64),
            Kind::Bool => raw.parse::<bool>().ok().map(Value::Bool),
            Kind::Str => Some(Value::Str(raw.to_owned())),
            _ => None,
        };
        if let Some(v) = value {
            let _ = target.set_by_name(key, v);
        }
    }
    theme
}

/// A file name for a theme name: what is not a letter, digit, space,
/// dash or underscore is dropped.
fn file_name(name: &str) -> Option<String> {
    let clean: String = name.trim().chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).collect();
    (!clean.is_empty()).then(|| format!("{clean}.theme"))
}

/// The saved themes in `dir`, by name, sorted.
pub fn list_in(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter_map(|e| e.path().file_name()?.to_str()?.strip_suffix(".theme").map(str::to_owned)).collect())
        .unwrap_or_default();
    out.sort_by_key(|n| n.to_lowercase());
    out
}

pub fn list() -> Vec<String> {
    dir().map(|d| list_in(&d)).unwrap_or_default()
}

pub fn save_in(dir: &Path, name: &str, theme: &Theme) -> std::io::Result<()> {
    let file = file_name(name).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a theme needs a name"))?;
    persist::save_text(&dir.join(file), &to_text(theme))
}

pub fn save(name: &str, theme: &Theme) -> std::io::Result<()> {
    let d = dir().ok_or_else(|| std::io::Error::other("no config folder"))?;
    save_in(&d, name, theme)
}

pub fn load_in(dir: &Path, name: &str) -> Option<Theme> {
    let text = persist::load_text(&dir.join(file_name(name)?))?;
    Some(from_text(&text))
}

pub fn delete_in(dir: &Path, name: &str) -> std::io::Result<()> {
    match file_name(name) {
        Some(f) => std::fs::remove_file(dir.join(f)),
        None => Ok(()),
    }
}

pub fn delete(name: &str) -> std::io::Result<()> {
    match dir() {
        Some(d) => delete_in(&d, name),
        None => Ok(()),
    }
}

/// The theme called `name`: a saved one first, else a built-in preset.
pub fn named(name: &str) -> Option<Theme> {
    if let Some(t) = dir().and_then(|d| load_in(&d, name)) {
        return Some(t);
    }
    Theme::PRESETS.iter().find(|(n, _)| *n == name).map(|(_, make)| make())
}

/// The preset `theme` is, if it is one unchanged.
pub fn preset_name(theme: &Theme) -> Option<&'static str> {
    Theme::PRESETS.iter().find(|(_, make)| make() == *theme).map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        // Eight bits per channel is what the file holds, so the round trip
        // is exact for colors that are (a shaded pair is not).
        let mut t = Theme::high_contrast();
        t.header = Gradient::new(Color::hex(0x112233), Color::hex(0x445566));
        t.border_width = 3.0;
        let text = to_text(&t);
        assert!(text.contains("header = #112233 #445566"), "{text}");
        assert!(text.contains("border_width = 3"));
        assert_eq!(from_text(&text), t);
        let loose = "# comment\nnot a line\naccent = #FF0000\nbogus = 1\ntext_size = 9999\nheader = #ABCDEF\n";
        let t = from_text(loose);
        assert_eq!(t.accent, Color::hex(0xFF0000));
        assert_eq!(t.text_size, 80.0, "clamped to the hard range");
        assert!(t.header.is_flat() && t.header.top == Color::hex(0xABCDEF), "one color is a flat gradient");
    }

    #[test]
    fn saved_themes() {
        let dir = std::env::temp_dir().join(format!("lntrn-ui-themes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(list_in(&dir).is_empty());
        let t = Theme::high_contrast();
        save_in(&dir, "Mine / Night?", &t).unwrap();
        save_in(&dir, "alpha", &Theme::light()).unwrap();
        assert_eq!(list_in(&dir), vec!["alpha".to_owned(), "Mine  Night".to_owned()], "cleaned name, sorted without case");
        assert_eq!(load_in(&dir, "Mine  Night").unwrap(), t);
        assert!(load_in(&dir, "nope").is_none());
        assert!(save_in(&dir, "///", &t).is_err(), "a name of nothing");
        delete_in(&dir, "alpha").unwrap();
        assert_eq!(list_in(&dir), vec!["Mine  Night".to_owned()]);
        assert_eq!(preset_name(&Theme::light()), Some("Light"));
        assert_eq!(preset_name(&t), Some("High Contrast"));
        assert_eq!(preset_name(&Theme { accent: Color::RED, ..Theme::light() }), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
