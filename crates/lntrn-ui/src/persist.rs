//! Keeping things between runs: any `props!` value (preferences, settings)
//! as field-id-tagged bytes, and small text files (the area layout), in
//! the app's config directory. Writes go through a temp file and a rename
//! so a crash mid-write cannot leave a half file.

use std::io;
use std::path::{Path, PathBuf};

use lntrn_props::{Reflect, serial};

/// Identifies a preferences file written by this module.
const MAGIC: &[u8; 8] = b"LNTRNPF1";

/// The app's config directory: `$XDG_CONFIG_HOME/<app_id>` or
/// `~/.config/<app_id>`. Made if missing. `None` with no home.
pub fn config_dir(app_id: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let dir = base.join(app_id);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Save a reflected value. Old builds skip fields they do not know; new
/// builds keep defaults for fields the file lacks.
pub fn save(path: &Path, value: &dyn Reflect) -> io::Result<()> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(serial::to_bytes(value));
    write_atomic(path, &bytes)
}

/// Load into `value`. `false` (and `value` untouched) when the file is
/// missing or not one of ours.
pub fn load(path: &Path, value: &mut dyn Reflect) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Some(body) = bytes.strip_prefix(MAGIC) else {
        return false;
    };
    serial::from_bytes(value, body).is_ok()
}

pub fn save_text(path: &Path, text: &str) -> io::Result<()> {
    write_atomic(path, text.as_bytes())
}

pub fn load_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Prefs;
    use lntrn_math::Color;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lntrn-persist-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prefs_round_trip_and_reject_garbage() {
        let dir = scratch("prefs");
        let path = dir.join("prefs.bin");
        let mut p = Prefs::default();
        p.ui_scale = 1.4;
        p.focus_follows_mouse = true;
        p.theme.accent = Color::hex(0x00FF88);
        p.theme.text_size = 30.0;
        save(&path, &p).unwrap();
        let mut back = Prefs::default();
        assert!(load(&path, &mut back));
        assert_eq!(back.ui_scale, 1.4);
        assert!(back.focus_follows_mouse);
        assert_eq!(back.theme.accent, Color::hex(0x00FF88));
        assert_eq!(back.theme.text_size, 30.0);
        assert!(!dir.join("prefs.tmp").exists(), "the temp file was renamed away");

        std::fs::write(&path, b"not ours at all").unwrap();
        let mut untouched = Prefs::default();
        untouched.ui_scale = 2.0;
        assert!(!load(&path, &mut untouched));
        assert_eq!(untouched.ui_scale, 2.0);
        assert!(!load(&dir.join("missing.bin"), &mut untouched));

        save_text(&dir.join("layout.txt"), "(h 0.5 [A] [B])").unwrap();
        assert_eq!(load_text(&dir.join("layout.txt")).as_deref(), Some("(h 0.5 [A] [B])"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
