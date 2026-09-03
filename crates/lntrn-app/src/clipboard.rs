//! The system clipboard: over the window's own Wayland connection when it
//! has one (see [`crate::wayland`]), else in-app only, and everything
//! else works the same. The harness pulls it in right before a rebuild
//! that carries a paste key ([`lntrn_ui::Event::is_paste`]) and pushes
//! ours out after a rebuild in which a widget copied
//! ([`lntrn_ui::UiState::take_clipboard_dirty`]). Files dragged in from
//! outside arrive over the same connection ([`Clipboard::drag_events`]).

use lntrn_core::log_info;
use lntrn_math::Vec2;
use lntrn_render::wgpu::rwh::RawDisplayHandle;
use lntrn_ui::Event;

pub struct Clipboard {
    #[cfg(target_os = "linux")]
    native: Option<crate::wayland::Clipboard>,
}

impl Clipboard {
    /// The clipboard of the window behind `display`. Anything but a live
    /// Wayland display gives the in-app one.
    pub fn new(display: Option<RawDisplayHandle>) -> Self {
        #[cfg(target_os = "linux")]
        let native = match display {
            // SAFETY: the display handle comes from the window this
            // clipboard belongs to and outlives it (the harness drops the
            // clipboard with the window, on the window's thread).
            Some(RawDisplayHandle::Wayland(h)) => unsafe { crate::wayland::Clipboard::new(h.display.as_ptr()) },
            _ => None,
        };
        #[cfg(not(target_os = "linux"))]
        let _ = display;
        let me = Self {
            #[cfg(target_os = "linux")]
            native,
        };
        log_info!("clipboard: {}", if me.available() { "the window's Wayland connection" } else { "in-app only" });
        me
    }

    /// The in-app clipboard alone.
    pub fn none() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            native: None,
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
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = scale;
        out
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
}
