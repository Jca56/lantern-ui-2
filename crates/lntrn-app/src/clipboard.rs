//! The system clipboard, by way of `wl-copy` and `wl-paste` (the
//! wl-clipboard tools) when they are on the PATH under Wayland. Nothing
//! here is a dependency: without the tools, or off Wayland, the clipboard
//! stays in-app and everything else works. A from-scratch data-control
//! client can replace this later without the harnesses noticing.
//!
//! The harness pulls the system clipboard in right before a rebuild that
//! carries a paste key ([`lntrn_ui::Event::is_paste`]) and pushes ours out
//! after a rebuild in which a widget copied
//! ([`lntrn_ui::UiState::take_clipboard_dirty`]).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// `wl-copy` and `wl-paste`, looked up once.
fn tools() -> Option<&'static (PathBuf, PathBuf)> {
    static TOOLS: OnceLock<Option<(PathBuf, PathBuf)>> = OnceLock::new();
    TOOLS
        .get_or_init(|| {
            std::env::var_os("WAYLAND_DISPLAY")?;
            Some((find("wl-copy")?, find("wl-paste")?))
        })
        .as_ref()
}

fn find(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(name)).find(|p| Path::is_file(p))
}

/// Whether the system clipboard can be reached at all.
pub fn available() -> bool {
    tools().is_some()
}

/// The system clipboard's text, if it holds any.
pub fn read() -> Option<String> {
    let (_, paste) = tools()?;
    let out = Command::new(paste).arg("--no-newline").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Put `text` on the system clipboard. `false` when it could not be.
pub fn write(text: &str) -> bool {
    let Some((copy, _)) = tools() else {
        return false;
    };
    let Ok(mut child) = Command::new(copy).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return false;
    };
    // wl-copy reads everything, then forks off a server for the selection
    // and exits, so waiting here is quick.
    let written = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    written && child.wait().is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a Wayland session with wl-clipboard: `cargo test -p lntrn-app -- --ignored`.
    #[test]
    #[ignore]
    fn round_trip_through_the_system_clipboard() {
        assert!(available(), "wl-copy and wl-paste on the PATH under Wayland");
        let text = format!("lantern clipboard {}", std::process::id());
        assert!(write(&text));
        assert_eq!(read().as_deref(), Some(text.as_str()));
        assert!(write("two\nlines"), "newlines survive");
        assert_eq!(read().as_deref(), Some("two\nlines"));
    }
}
