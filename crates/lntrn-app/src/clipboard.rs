//! The system clipboard: over the window's own Wayland connection when it
//! has one (see [`crate::wayland`]), else in-app only, and everything
//! else works the same. The harness pulls it in right before a rebuild
//! that carries a paste key ([`lntrn_ui::Event::is_paste`]) and pushes
//! ours out after a rebuild in which a widget copied
//! ([`lntrn_ui::UiState::take_clipboard_dirty`]). Files dragged in from
//! outside arrive over the same connection ([`Clipboard::drag_events`]),
//! and what a widget drags out leaves over it ([`Clipboard::start_drag`]).

use std::path::PathBuf;

use lntrn_core::log_info;
use lntrn_math::Vec2;
use lntrn_render::wgpu::rwh::{RawDisplayHandle, RawWindowHandle};
use lntrn_ui::{DragPayload, Event};

pub struct Clipboard {
    #[cfg(target_os = "linux")]
    native: Option<crate::wayland::Clipboard>,
    /// Where dragged pictures are written as files, once one was; gone
    /// with the clipboard.
    drag_dir: Option<PathBuf>,
}

impl Clipboard {
    /// The clipboard of the window behind `display`, and `window` (for
    /// drags out). Anything but a live Wayland display gives the in-app
    /// one.
    pub fn new(display: Option<RawDisplayHandle>, window: Option<RawWindowHandle>) -> Self {
        #[cfg(target_os = "linux")]
        let native = match display {
            // SAFETY: the handles come from the window this clipboard
            // belongs to and outlive it (the harness drops the clipboard
            // with the window, on the window's thread).
            Some(RawDisplayHandle::Wayland(h)) => unsafe {
                let surface = match window {
                    Some(RawWindowHandle::Wayland(w)) => w.surface.as_ptr(),
                    _ => std::ptr::null_mut(),
                };
                crate::wayland::Clipboard::new(h.display.as_ptr(), surface)
            },
            _ => None,
        };
        #[cfg(not(target_os = "linux"))]
        let _ = (display, window);
        let me = Self {
            #[cfg(target_os = "linux")]
            native,
            drag_dir: None,
        };
        log_info!("clipboard: {}", if me.available() { "the window's Wayland connection" } else { "in-app only" });
        me
    }

    /// The in-app clipboard alone.
    pub fn none() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            native: None,
            drag_dir: None,
        }
    }

    /// Whether the system clipboard can be reached at all.
    pub fn available(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.native.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Handle what the compositor sent since last time. Once per loop turn.
    pub fn poll(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(n) = &mut self.native {
            n.poll();
        }
    }

    /// The system clipboard's text, if it holds any.
    pub fn read(&mut self) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            self.native.as_mut()?.read()
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// What a drag from outside did since last time, as events at `scale`
    /// physical pixels per logical one: the pointer's position, the files
    /// hovering, a drop, or the drag leaving.
    pub fn drag_events(&mut self, scale: f64) -> Vec<Event> {
        let mut out = Vec::new();
        #[cfg(target_os = "linux")]
        if let Some(n) = &mut self.native {
            for ev in n.take_drag_events() {
                match ev {
                    crate::wayland::DragEvent::Moved(x, y) => out.push(Event::PointerMoved(Vec2::new(x * scale, y * scale))),
                    crate::wayland::DragEvent::Hovered(paths) => out.extend(paths.into_iter().map(Event::FileHovered)),
                    crate::wayland::DragEvent::Left => out.push(Event::FileHoverLeft),
                    crate::wayland::DragEvent::Dropped(paths) => out.extend(paths.into_iter().map(Event::FileDropped)),
                    crate::wayland::DragEvent::Ended { dropped } => out.push(Event::DragEnded { dropped }),
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = scale;
        out
    }

    /// Start dragging `payload` out of the window, the pointer's button
    /// being down. A picture goes as PNG and, for the apps that take files
    /// rather than pictures, as a file under the runtime directory that
    /// lives as long as this clipboard. `false` when there is no window
    /// system drag to be had.
    pub fn start_drag(&mut self, payload: DragPayload) -> bool {
        #[cfg(target_os = "linux")]
        {
            use crate::wayland::DragData;
            let data = match payload {
                DragPayload::Text(t) => DragData::Text(t),
                DragPayload::Files(f) => DragData::Files(f),
                DragPayload::Image { image, name } => {
                    let png = lntrn_image::encode_png(&image);
                    let file = self.drag_file(&name, &png);
                    DragData::Png { png, file }
                }
            };
            self.native.as_mut().is_some_and(|n| n.start_drag(data))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = payload;
            false
        }
    }

    /// Write a dragged picture where other apps can pick it up as a file.
    #[cfg(target_os = "linux")]
    fn drag_file(&mut self, name: &str, png: &[u8]) -> Option<PathBuf> {
        let dir = self.drag_dir.get_or_insert_with(|| {
            let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
            base.join(format!("lntrn-{}", std::process::id()))
        });
        std::fs::create_dir_all(&dir).ok()?;
        let mut file: String = name.chars().map(|c| if c == '/' || c.is_control() { '_' } else { c }).collect::<String>().trim().to_owned();
        if file.is_empty() {
            file = "picture".to_owned();
        }
        if !file.to_ascii_lowercase().ends_with(".png") {
            file.push_str(".png");
        }
        let path = dir.join(file);
        std::fs::write(&path, png).ok()?;
        Some(path)
    }

    /// Put `text` on the system clipboard. `false` when it could not be.
    pub fn write(&mut self, text: &str) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.native.as_mut().is_some_and(|n| n.write(text))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = text;
            false
        }
    }

    /// The system clipboard's picture as PNG bytes, if it holds one.
    pub fn read_image(&mut self) -> Option<Vec<u8>> {
        #[cfg(target_os = "linux")]
        {
            self.native.as_mut()?.read_image()
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Put a PNG on the system clipboard. `false` when it could not be.
    pub fn write_image(&mut self, png: &[u8]) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.native.as_mut().is_some_and(|n| n.write_image(png))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = png;
            false
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // The dragged pictures' files: ours alone, under our own name.
        if let Some(dir) = &self.drag_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
