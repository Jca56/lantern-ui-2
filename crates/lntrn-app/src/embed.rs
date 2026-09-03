//! The embedded harness: the same shell and frame, drawn into a window
//! somebody else owns — a plugin editor inside a DAW, a view inside
//! another toolkit. No winit: the owner hands over raw window handles and
//! feeds events in our own vocabulary (`lntrn_ui::Event`).
//!
//! ```ignore
//! let mut view = unsafe { Embedded::new(config, host, shell, display, window, w, h) }?;
//! // on each host event:  view.push(Event::PointerMoved(p));
//! // on each host tick:   if view.wants_frame() { let out = view.frame(); }
//! ```

use std::time::{Duration, Instant};

use lntrn_core::log_info;
use lntrn_math::Rect;
use lntrn_render::wgpu;
use lntrn_render::{DrawList, Gpu, GpuError, Images};
use lntrn_text::TextEngine;
use lntrn_ui::{CursorIcon, Event, Shell, WindowState};

use crate::app::AppHost;
use crate::frame::{Gfx, draw_frame, rebuild};

/// How an embedded view is made.
#[derive(Clone, Debug)]
pub struct EmbedConfig {
    /// Proportional and monospace font families.
    pub sans: String,
    pub mono: String,
    /// The owner's scale factor (physical pixels per logical pixel).
    pub scale: f64,
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self { sans: "Inter".into(), mono: "JetBrains Mono".into(), scale: 1.0 }
    }
}

/// What the owner should do after a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EmbedOutput {
    pub cursor: CursorIcon,
    /// An animation wants another frame after this many seconds.
    pub wake_after: Option<f64>,
    /// The UI asked to close (a Quit action).
    pub quit: bool,
    /// A text widget has focus and its caret is here (physical pixels):
    /// tell the owner's input method. `None` means no text focus.
    pub ime: Option<Rect>,
}

pub struct Embedded<H: AppHost> {
    gfx: Gfx,
    text: TextEngine,
    draw: DrawList,
    shell: Shell<H>,
    host: H,
    events: Vec<Event>,
    scale: f64,
    focused: bool,
    wake: Option<Instant>,
    dirty: bool,
}

impl<H: AppHost> Embedded<H> {
    /// Make a view drawing into the owner's `window`, `width` × `height`
    /// physical pixels.
    ///
    /// # Safety
    /// `display` and `window` must be valid for as long as the view lives.
    pub unsafe fn new(config: EmbedConfig, mut host: H, shell: Shell<H>, display: wgpu::rwh::RawDisplayHandle, window: wgpu::rwh::RawWindowHandle, width: u32, height: u32) -> Result<Self, GpuError> {
        let text = TextEngine::new(&config.sans, &config.mono);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        // SAFETY: the caller promises the handles outlive the view.
        let surface = unsafe { instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle { raw_display_handle: display, raw_window_handle: window }) }
            .map_err(|e| GpuError::Surface(e.to_string()))?;
        let gpu = Gpu::with_instance(instance, Some(&surface))?;
        let mut gfx = Gfx::new(gpu, surface, width.max(1), height.max(1), &text);
        host.init_gpu(&gfx.gpu, gfx.surface.format(), &mut gfx.images);
        log_info!("embedded view: {width}x{height} @ {:.2}x", config.scale);
        Ok(Self { gfx, text, draw: DrawList::new(), shell, host, events: Vec::new(), scale: config.scale, focused: true, wake: None, dirty: true })
    }

    /// Queue an input event for the next frame.
    pub fn push(&mut self, ev: Event) {
        match ev {
            Event::Focus(f) => self.focused = f,
            Event::ScaleFactor(s) => self.scale = s,
            Event::Resized { width, height } => self.gfx.resize(width.max(1), height.max(1)),
            _ => {}
        }
        self.events.push(ev);
        self.dirty = true;
    }

    /// Something needs drawing: input arrived or an animation is due.
    pub fn wants_frame(&self) -> bool {
        self.dirty || self.wake.is_some_and(|w| Instant::now() >= w)
    }

    /// Rebuild from the queued events, draw and present.
    pub fn frame(&mut self) -> EmbedOutput {
        self.dirty = false;
        self.wake = None;
        let events = std::mem::take(&mut self.events);
        let ws = WindowState { maximized: true, focused: self.focused };
        let (out, pending) = rebuild(&mut self.gfx, &mut self.host, &mut self.shell, &mut self.text, &mut self.draw, &events, self.scale, ws);
        if pending {
            self.dirty = true;
        }
        if !draw_frame(&mut self.gfx, &mut self.host, &mut self.text, &self.draw, out.clear, || {}) {
            self.dirty = true;
        }
        self.wake = out.wake_after.map(|s| Instant::now() + Duration::from_secs_f64(s));
        EmbedOutput { cursor: out.cursor, wake_after: out.wake_after, quit: out.quit, ime: out.ime }
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut H {
        self.dirty = true;
        &mut self.host
    }

    pub fn shell(&self) -> &Shell<H> {
        &self.shell
    }

    pub fn shell_mut(&mut self) -> &mut Shell<H> {
        self.dirty = true;
        &mut self.shell
    }

    pub fn gpu(&self) -> &Gpu {
        &self.gfx.gpu
    }

    pub fn images(&mut self) -> &mut Images {
        &mut self.gfx.images
    }

    pub fn text(&mut self) -> &mut TextEngine {
        &mut self.text
    }
}
