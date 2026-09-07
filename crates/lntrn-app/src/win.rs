//! One window of the app: its winit window and surface, its clipboard,
//! its shell, and the input gathered since its last rebuild. The app in
//! `app.rs` owns any number of these; the first is the main window.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lntrn_core::{log_error, log_info};
use lntrn_math::{Rect, Vec2};
use lntrn_render::wgpu;
use lntrn_render::{DrawList, Gpu, Images};
use lntrn_text::TextEngine;
use lntrn_ui::{CursorIcon, Event, Modifiers, NewWindow, ResizeEdge, Shell, WindowCommand, WindowState};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowAttributes};

use crate::app::AppHost;
use crate::clipboard::Clipboard;
use crate::frame::{Gfx, GpuShared, draw_frame, rebuild};

pub(crate) struct Win<H: AppHost> {
    /// 0 for the main window; what the host sees as `RenderCx::window`.
    pub id: u32,
    /// `clipboard` and `gfx` come before `window` on purpose: fields drop
    /// in declaration order, and winit disconnects the Wayland display
    /// when the last handle to the window goes. Both the clipboard's own
    /// Wayland objects and the GPU's (the wgpu instance's GL backend keeps
    /// an EGL display on the same connection, and the surface holds the
    /// instance's last reference) must be destroyed before that, or their
    /// teardown is a segfault in libwayland-client.
    pub clipboard: Clipboard,
    pub gfx: Gfx,
    pub window: Arc<Window>,
    pub shell: Shell<H>,
    /// Events since the last rebuild, in order.
    pub events: Vec<Event>,
    mods: Modifiers,
    pointer: Vec2,
    pub scale: f64,
    focused: bool,
    cursor: CursorIcon,
    /// Where the input method was last told the caret is.
    ime: Option<Rect>,
    /// Something happened; rebuild before the loop goes back to sleep.
    pub dirty: bool,
    /// An animation asked to be woken at this time.
    pub wake: Option<Instant>,
}

/// What a window's frame asks of the app.
#[derive(Default)]
pub(crate) struct Outcome {
    pub quit: bool,
    /// The window asked to close: its title bar, or
    /// [`lntrn_ui::ShellRequest::CloseWindow`].
    pub close: bool,
    pub new_windows: Vec<NewWindow>,
}

impl<H: AppHost> Win<H> {
    /// Open a window. The first one brings the GPU up (filling `shared`)
    /// and lets the host upload its pictures.
    #[allow(clippy::too_many_arguments)]
    pub fn open(event_loop: &ActiveEventLoop, attrs: WindowAttributes, shared: &mut Option<GpuShared>, text: &TextEngine, host: &mut H, shell: Shell<H>, id: u32, transparent: bool) -> Option<Self> {
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log_error!("window: {e}");
                return None;
            }
        };
        let scale = window.scale_factor();
        let size = window.inner_size();
        let first = shared.is_none();
        let mut surface = None;
        if first {
            // The device is picked against the first window's surface, so
            // it can present there; later windows share it.
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let s = instance.create_surface(Arc::clone(&window)).map_err(|e| log_error!("surface: {e}")).ok()?;
            let gpu = Gpu::with_instance(instance, Some(&s)).map_err(|e| log_error!("{e}")).ok()?;
            let images = Images::new(&gpu);
            *shared = Some(GpuShared { gpu, images });
            surface = Some(s);
        }
        let shared = shared.as_mut()?;
        let surface = match surface {
            Some(s) => s,
            None => shared.gpu.instance.create_surface(Arc::clone(&window)).map_err(|e| log_error!("surface: {e}")).ok()?,
        };
        let gfx = Gfx::new(shared, surface, size.width.max(1), size.height.max(1), text, transparent);
        if first {
            host.init_gpu(&shared.gpu, gfx.surface.format(), &mut shared.images);
        }
        log_info!("window {id}: {}x{} @ {scale:.2}x", size.width, size.height);
        let clipboard = Clipboard::new(window.display_handle().ok().map(|h| h.as_raw()), window.window_handle().ok().map(|h| h.as_raw()));
        Some(Self { id, window, gfx, shell, clipboard, events: Vec::new(), mods: Modifiers::NONE, pointer: Vec2::ZERO, scale, focused: true, cursor: CursorIcon::Default, ime: None, dirty: true, wake: None })
    }

    /// Take in one of the window system's events.
    pub fn handle(&mut self, event: &WindowEvent, shared: &GpuShared) {
        if let Some(ev) = crate::translate::window_event(event, &mut self.mods, &mut self.pointer) {
            self.events.push(ev);
            if let Some(text) = crate::translate::key_text(event, self.mods) {
                self.events.push(text);
            }
            self.dirty = true;
        }
        match event {
            WindowEvent::Resized(size) => self.gfx.resize(shared, size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.scale = *scale_factor,
            WindowEvent::Focused(f) => self.focused = *f,
            _ => {}
        }
    }

    /// Serve the clipboard's queue (another app pasting what this window
    /// copied, or a drag coming in). Once per loop turn.
    pub fn poll(&mut self) {
        self.clipboard.poll();
        let drag = self.clipboard.drag_events(self.scale);
        if !drag.is_empty() {
            self.events.extend(drag);
            self.dirty = true;
        }
    }

    /// Rebuild the UI from the pending events (possibly more than once),
    /// then draw and present.
    pub fn render(&mut self, host: &mut H, shared: &mut GpuShared, text: &mut TextEngine, draw: &mut DrawList) -> Outcome {
        let events = std::mem::take(&mut self.events);
        let ws = WindowState { maximized: self.window.is_maximized(), focused: self.focused };
        let (out, pending) = rebuild(shared, &self.gfx, host, &mut self.shell, text, draw, &events, self.scale, ws, &mut self.clipboard);
        if pending {
            // Out of rebuilds with work still pending: finish it next frame.
            self.dirty = true;
        }
        self.wake = out.wake_after.map(|s| Instant::now() + Duration::from_secs_f64(s));

        if out.cursor != self.cursor {
            self.cursor = out.cursor;
            self.window.set_cursor(cursor_icon(out.cursor));
        }
        // Input methods follow the focused text widget's caret.
        if out.ime != self.ime {
            if out.ime.is_some() != self.ime.is_some() {
                self.window.set_ime_allowed(out.ime.is_some());
            }
            if let Some(r) = out.ime {
                self.window.set_ime_cursor_area(PhysicalPosition::new(r.min.x, r.min.y), PhysicalSize::new(r.width(), r.height()));
            }
            self.ime = out.ime;
        }
        let mut outcome = Outcome { quit: out.quit, close: false, new_windows: self.shell.take_new_windows() };
        match out.window_command {
            Some(WindowCommand::Drag) => {
                let _ = self.window.drag_window();
            }
            Some(WindowCommand::Minimize) => self.window.set_minimized(true),
            Some(WindowCommand::ToggleMaximize) => self.window.set_maximized(!self.window.is_maximized()),
            Some(WindowCommand::Close) => outcome.close = true,
            Some(WindowCommand::Resize(edge)) => {
                let _ = self.window.drag_resize_window(resize_direction(edge));
            }
            None => {}
        }

        let window = Arc::clone(&self.window);
        if !draw_frame(shared, &mut self.gfx, host, text, draw, out.clear, self.id, || window.pre_present_notify()) {
            self.window.request_redraw();
        }
        outcome
    }
}

fn cursor_icon(c: CursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon as W;
    match c {
        CursorIcon::Default => W::Default,
        CursorIcon::Pointer => W::Pointer,
        CursorIcon::Text => W::Text,
        CursorIcon::EwResize => W::EwResize,
        CursorIcon::NsResize => W::NsResize,
        CursorIcon::NeswResize => W::NeswResize,
        CursorIcon::NwseResize => W::NwseResize,
        CursorIcon::Grabbing => W::Grabbing,
    }
}

fn resize_direction(e: ResizeEdge) -> winit::window::ResizeDirection {
    use winit::window::ResizeDirection as R;
    match e {
        ResizeEdge::North => R::North,
        ResizeEdge::South => R::South,
        ResizeEdge::East => R::East,
        ResizeEdge::West => R::West,
        ResizeEdge::NorthEast => R::NorthEast,
        ResizeEdge::NorthWest => R::NorthWest,
        ResizeEdge::SouthEast => R::SouthEast,
        ResizeEdge::SouthWest => R::SouthWest,
    }
}
