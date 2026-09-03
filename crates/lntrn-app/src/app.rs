//! The winit application: window, GPU wiring, event translation, and the
//! rebuild → draw → present cycle.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lntrn_core::{log_error, log_info};
use lntrn_math::{Rect, Vec2};
use lntrn_render::wgpu;
use lntrn_render::{DrawList, Gpu, Images, RenderGraph, TexId};
use lntrn_text::TextEngine;
use lntrn_ui::persist;
use lntrn_ui::{CursorIcon, Event, Host, Modifiers, ResizeEdge, Shell, WindowCommand, WindowState};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::{Window, WindowId};

use crate::clipboard::Clipboard;
use crate::frame::{Gfx, draw_frame, rebuild};

/// How the window is made.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// The compositor's title for the window.
    pub title: String,
    /// Wayland app id / X11 class: the name of the binary and its desktop
    /// entry, so the compositor can find the icon.
    pub app_id: String,
    /// Initial inner size in logical pixels.
    pub size: (f64, f64),
    pub maximized: bool,
    /// Ask the compositor for a frame. The shell draws its own title bar
    /// either way; leave this off unless the compositor insists.
    pub decorations: bool,
    /// Proportional and monospace font families.
    pub sans: String,
    pub mono: String,
    /// Keep preferences and the area layout in the app's config directory
    /// (`~/.config/<app_id>/`) between runs.
    pub persist: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { title: "Lantern".into(), app_id: "lantern-app".into(), size: (1600.0, 1000.0), maximized: true, decorations: false, sans: "Inter".into(), mono: "JetBrains Mono".into(), persist: true }
    }
}

const PREFS_FILE: &str = "prefs.bin";
const LAYOUT_FILE: &str = "layout.txt";

/// What a host's [`AppHost::render`] gets to add GPU passes with.
pub struct RenderCx<'f, 'a> {
    pub gpu: &'a Gpu,
    pub graph: &'a mut RenderGraph<'f>,
    /// The swapchain image, already cleared.
    pub backbuffer: TexId,
    /// A cleared depth buffer, when [`AppHost::wants_depth`] says so.
    pub depth: Option<TexId>,
    /// Window size in physical pixels.
    pub size: [u32; 2],
}

/// A [`Host`] that also takes part in the GPU frame. Every hook has a
/// do-nothing default; a UI-only app implements none of them.
pub trait AppHost: Host + Sized + 'static {
    /// The GPU exists: make pipelines and buffers, upload pictures.
    fn init_gpu(&mut self, gpu: &Gpu, format: wgpu::TextureFormat, images: &mut Images) {
        let _ = (gpu, format, images);
    }
    /// After each rebuild, before the frame is drawn: resolve GPU-side
    /// queries (picking) against the shell, upload pictures that arrived.
    /// Return `true` to rebuild again so what changed shows in this same
    /// frame.
    fn after_rebuild(&mut self, gpu: &Gpu, images: &mut Images, shell: &mut Shell<Self>) -> bool {
        let _ = (gpu, images, shell);
        false
    }
    /// Whether [`RenderCx::depth`] should exist.
    fn wants_depth(&self) -> bool {
        false
    }
    /// Add render-graph nodes drawn between the clear and the UI pass.
    fn render<'f>(&'f mut self, cx: &mut RenderCx<'f, '_>) {
        let _ = cx;
    }
}

/// Open the window and run until it closes. With `persist` on, saved
/// preferences and layout are loaded first and written back on exit.
pub fn run<H: AppHost>(config: AppConfig, host: H, mut shell: Shell<H>) {
    lntrn_core::log::init();
    if config.persist && let Some(dir) = persist::config_dir(&config.app_id) {
        if persist::load(&dir.join(PREFS_FILE), &mut shell.prefs) {
            log_info!("loaded preferences from {}", dir.display());
        }
        if let Some(text) = persist::load_text(&dir.join(LAYOUT_FILE)) {
            if shell.restore_layout(&host, &text) {
                log_info!("restored layout: {text}");
            } else {
                log_error!("ignoring unreadable layout file in {}", dir.display());
            }
        }
    }
    let event_loop = EventLoop::new().expect("create event loop");
    // Redraw on demand: an idle app sits at 0% CPU/GPU.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(config, host, shell);
    if let Err(e) = event_loop.run_app(&mut app) {
        log_error!("event loop: {e}");
    }
}

struct Win {
    window: Arc<Window>,
    gfx: Gfx,
    cursor: CursorIcon,
    /// Where the input method was last told the caret is.
    ime: Option<Rect>,
    clipboard: Clipboard,
}

struct App<H: AppHost> {
    config: AppConfig,
    win: Option<Win>,
    text: TextEngine,
    draw: DrawList,
    shell: Shell<H>,
    host: H,
    /// Events since the last rebuild, in order.
    events: Vec<Event>,
    mods: Modifiers,
    pointer: Vec2,
    scale: f64,
    /// Something happened; rebuild before the loop goes back to sleep.
    dirty: bool,
    /// An animation asked to be woken at this time.
    wake: Option<Instant>,
    focused: bool,
    quit: bool,
}

impl<H: AppHost> App<H> {
    fn new(config: AppConfig, host: H, shell: Shell<H>) -> Self {
        let t = std::time::Instant::now();
        let text = TextEngine::new(&config.sans, &config.mono);
        log_info!("fonts: {} faces in {:.0} ms", text.face_count(), t.elapsed().as_secs_f64() * 1000.0);
        Self { config, win: None, text, draw: DrawList::new(), shell, host, events: Vec::new(), mods: Modifiers::NONE, pointer: Vec2::ZERO, scale: 1.0, dirty: true, wake: None, focused: true, quit: false }
    }

    /// Write preferences and the layout for next time.
    fn save_state(&self) {
        if !self.config.persist {
            return;
        }
        let Some(dir) = persist::config_dir(&self.config.app_id) else {
            return;
        };
        if let Err(e) = persist::save(&dir.join(PREFS_FILE), &self.shell.prefs) {
            log_error!("saving preferences: {e}");
        }
        if let Err(e) = persist::save_text(&dir.join(LAYOUT_FILE), &self.shell.layout_description(&self.host)) {
            log_error!("saving layout: {e}");
        }
    }

    fn init_gfx(&mut self, event_loop: &ActiveEventLoop) {
        let c = &self.config;
        let attrs = Window::default_attributes().with_title(&c.title).with_decorations(c.decorations).with_inner_size(LogicalSize::new(c.size.0, c.size.1)).with_maximized(c.maximized);
        // The app id pairs the window with its desktop entry and icon under
        // Wayland; the X11 class does the same job there.
        #[cfg(target_os = "linux")]
        let attrs = {
            let attrs = winit::platform::wayland::WindowAttributesExtWayland::with_name(attrs, &c.app_id, &c.app_id);
            winit::platform::x11::WindowAttributesExtX11::with_name(attrs, &c.app_id, &c.app_id)
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        self.scale = window.scale_factor();
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(Arc::clone(&window)).expect("create surface");
        let gpu = match Gpu::with_instance(instance, Some(&surface)) {
            Ok(g) => g,
            Err(e) => {
                log_error!("{e}");
                event_loop.exit();
                return;
            }
        };
        let mut gfx = Gfx::new(gpu, surface, size.width, size.height, &self.text);
        self.host.init_gpu(&gfx.gpu, gfx.surface.format(), &mut gfx.images);
        log_info!("window: {}x{} @ {:.2}x", size.width, size.height, self.scale);
        let clipboard = Clipboard::new(window.display_handle().ok().map(|h| h.as_raw()));
        self.win = Some(Win { window, gfx, cursor: CursorIcon::Default, ime: None, clipboard });
    }

    /// Rebuild the UI from the pending events (possibly more than once),
    /// then draw and present.
    fn render(&mut self) {
        let Some(win) = self.win.as_mut() else {
            return;
        };
        let events = std::mem::take(&mut self.events);
        let ws = WindowState { maximized: win.window.is_maximized(), focused: self.focused };
        let (out, pending) = rebuild(&mut win.gfx, &mut self.host, &mut self.shell, &mut self.text, &mut self.draw, &events, self.scale, ws, &mut win.clipboard);
        if pending {
            // Out of rebuilds with work still pending: finish it next frame.
            self.dirty = true;
        }
        if out.quit {
            self.quit = true;
        }
        self.wake = out.wake_after.map(|s| Instant::now() + Duration::from_secs_f64(s));

        if out.cursor != win.cursor {
            win.cursor = out.cursor;
            win.window.set_cursor(cursor_icon(out.cursor));
        }
        // Input methods follow the focused text widget's caret.
        if out.ime != win.ime {
            if out.ime.is_some() != win.ime.is_some() {
                win.window.set_ime_allowed(out.ime.is_some());
            }
            if let Some(r) = out.ime {
                win.window.set_ime_cursor_area(PhysicalPosition::new(r.min.x, r.min.y), PhysicalSize::new(r.width(), r.height()));
            }
            win.ime = out.ime;
        }
        match out.window_command {
            Some(WindowCommand::Drag) => {
                let _ = win.window.drag_window();
            }
            Some(WindowCommand::Minimize) => win.window.set_minimized(true),
            Some(WindowCommand::ToggleMaximize) => win.window.set_maximized(!win.window.is_maximized()),
            Some(WindowCommand::Close) => self.quit = true,
            Some(WindowCommand::Resize(edge)) => {
                let _ = win.window.drag_resize_window(resize_direction(edge));
            }
            None => {}
        }

        let window = Arc::clone(&win.window);
        if !draw_frame(&mut win.gfx, &mut self.host, &mut self.text, &self.draw, out.clear, || window.pre_present_notify()) {
            win.window.request_redraw();
        }
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

impl<H: AppHost> ApplicationHandler for App<H> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_none() {
            self.init_gfx(event_loop);
            self.dirty = true;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(ev) = crate::translate::window_event(&event, &mut self.mods, &mut self.pointer) {
            self.events.push(ev);
            if let Some(text) = crate::translate::key_text(&event, self.mods) {
                self.events.push(text);
            }
            self.dirty = true;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(w) = self.win.as_mut() {
                    w.gfx.resize(size.width, size.height);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.scale = scale_factor,
            WindowEvent::Focused(f) => self.focused = f,
            WindowEvent::RedrawRequested => {
                self.render();
                if self.quit {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.wake = None;
            self.dirty = true;
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_state();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The clipboard's own queue: another app pasting what we copied.
        if let Some(w) = &mut self.win {
            w.clipboard.poll();
        }
        if self.dirty && let Some(w) = &self.win {
            self.dirty = false;
            w.window.request_redraw();
        }
        // Sleep until input, or until the running animation's next frame.
        event_loop.set_control_flow(match self.wake {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }
}
