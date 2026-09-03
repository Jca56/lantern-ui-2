//! The winit application: window, GPU wiring, event translation, and the
//! rebuild → draw → present cycle.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lntrn_core::{log_error, log_info, log_trace};
use lntrn_math::{Rect, Vec2};
use lntrn_render::wgpu;
use lntrn_render::{DrawList, Gpu, Pass2d, RenderGraph, SurfaceTarget, TexDesc, TexId, TexturePool, clear_pass};
use lntrn_text::TextEngine;
use lntrn_ui::{CursorIcon, Event, Host, Modifiers, ResizeEdge, Shell, WindowCommand, WindowState};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// A popup closing or a value committing may ask for one more rebuild; this
/// caps how many happen back to back before we present.
const MAX_REBUILDS: usize = 4;

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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { title: "Lantern".into(), app_id: "lantern-app".into(), size: (1600.0, 1000.0), maximized: true, decorations: false, sans: "Inter".into(), mono: "JetBrains Mono".into() }
    }
}

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
    /// The GPU exists: make pipelines and buffers.
    fn init_gpu(&mut self, gpu: &Gpu, format: wgpu::TextureFormat) {
        let _ = (gpu, format);
    }
    /// After each rebuild, before the frame is drawn: resolve GPU-side
    /// queries (picking) against the shell. Return `true` to rebuild again
    /// so what changed shows in this same frame.
    fn after_rebuild(&mut self, gpu: &Gpu, shell: &mut Shell<Self>) -> bool {
        let _ = (gpu, shell);
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

/// Open the window and run until it closes.
pub fn run<H: AppHost>(config: AppConfig, host: H, shell: Shell<H>) {
    lntrn_core::log::init();
    let event_loop = EventLoop::new().expect("create event loop");
    // Redraw on demand: an idle app sits at 0% CPU/GPU.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(config, host, shell);
    if let Err(e) = event_loop.run_app(&mut app) {
        log_error!("event loop: {e}");
    }
}

struct Gfx {
    window: Arc<Window>,
    gpu: Gpu,
    surface: SurfaceTarget,
    pass2d: Pass2d,
    pool: TexturePool,
    cursor: CursorIcon,
}

struct App<H: AppHost> {
    config: AppConfig,
    gfx: Option<Gfx>,
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
        Self { config, gfx: None, text, draw: DrawList::new(), shell, host, events: Vec::new(), mods: Modifiers::NONE, pointer: Vec2::ZERO, scale: 1.0, dirty: true, wake: None, focused: true, quit: false }
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
        let surface = SurfaceTarget::new(&gpu, surface, size.width, size.height);
        let pass2d = Pass2d::new(&gpu, surface.format(), self.text.atlas());
        self.host.init_gpu(&gpu, surface.format());
        log_info!("window: {}x{} @ {:.2}x", size.width, size.height, self.scale);
        self.gfx = Some(Gfx { window, gpu, surface, pass2d, pool: TexturePool::new(), cursor: CursorIcon::Default });
    }

    /// Rebuild the UI from the pending events (possibly more than once),
    /// then draw and present.
    fn render(&mut self) {
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        let size = gfx.surface.size();
        let window_rect = Rect::from_min_size(Vec2::ZERO, Vec2::new(size[0] as f64, size[1] as f64));
        let events = std::mem::take(&mut self.events);

        let ws = WindowState { maximized: gfx.window.is_maximized(), focused: self.focused };
        let mut out = None;
        let mut evs: &[Event] = &events;
        let mut command = None;
        let mut again = true;
        for _ in 0..MAX_REBUILDS {
            self.draw.clear();
            let o = self.shell.frame(&mut self.host, evs, window_rect, self.scale, ws, &mut self.text, &mut self.draw);
            again = o.rebuild_again;
            command = command.or(o.window_command);
            if o.quit {
                self.quit = true;
            }
            evs = &[];
            // GPU-side queries resolve right away, then one more rebuild
            // draws their result in this same frame. Doing it here (not after
            // present) also means a failed swapchain acquire cannot swallow
            // a click.
            if self.host.after_rebuild(&gfx.gpu, &mut self.shell) {
                again = true;
            }
            out = Some(o);
            if !again {
                break;
            }
        }
        if again {
            // Out of rebuilds with work still pending: finish it next frame.
            self.dirty = true;
        }
        let out = out.expect("at least one rebuild");
        self.wake = out.wake_after.map(|s| Instant::now() + Duration::from_secs_f64(s));

        if out.cursor != gfx.cursor {
            gfx.cursor = out.cursor;
            gfx.window.set_cursor(cursor_icon(out.cursor));
        }
        match command {
            Some(WindowCommand::Drag) => {
                let _ = gfx.window.drag_window();
            }
            Some(WindowCommand::Minimize) => gfx.window.set_minimized(true),
            Some(WindowCommand::ToggleMaximize) => gfx.window.set_maximized(!gfx.window.is_maximized()),
            Some(WindowCommand::Close) => self.quit = true,
            Some(WindowCommand::Resize(edge)) => {
                let _ = gfx.window.drag_resize_window(resize_direction(edge));
            }
            None => {}
        }

        let Some(frame) = gfx.surface.acquire(&gfx.gpu) else {
            gfx.window.request_redraw();
            return;
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gfx.gpu.create_encoder("lantern frame");

        let (pass2d, draw, text) = (&mut gfx.pass2d, &self.draw, &mut self.text);
        let clear = out.clear;
        let mut graph = RenderGraph::new();
        let backbuffer = graph.import(&view);
        let depth = self.host.wants_depth().then(|| graph.transient(TexDesc::depth("depth", size[0], size[1])));
        let writes: Vec<TexId> = std::iter::once(backbuffer).chain(depth).collect();
        graph.add_node("clear", &[], &writes, move |_, enc, views| {
            clear_pass(enc, views.get(backbuffer), depth.map(|d| views.get(d)), clear);
        });
        self.host.render(&mut RenderCx { gpu: &gfx.gpu, graph: &mut graph, backbuffer, depth, size });
        graph.add_node("ui", &[], &[backbuffer], move |gpu, enc, views| {
            pass2d.draw(gpu, enc, views.get(backbuffer), size, draw, text.atlas_mut(), None);
        });
        graph.execute(&gfx.gpu, &mut gfx.pool, &mut encoder);
        gfx.pool.end_frame();

        gfx.gpu.queue.submit([encoder.finish()]);
        gfx.window.pre_present_notify();
        frame.present();
        log_trace!("frame: {} vertices", self.draw.vertex_count());
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
        if self.gfx.is_none() {
            self.init_gfx(event_loop);
            self.dirty = true;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(ev) = crate::translate::window_event(&event, &mut self.mods, &mut self.pointer) {
            self.events.push(ev);
            if let Some(text) = crate::translate::key_text(&event) {
                self.events.push(text);
            }
            self.dirty = true;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = self.gfx.as_mut() {
                    g.surface.resize(&g.gpu, size.width, size.height);
                    g.pool.trim();
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.dirty && let Some(g) = &self.gfx {
            self.dirty = false;
            g.window.request_redraw();
        }
        // Sleep until input, or until the running animation's next frame.
        event_loop.set_control_flow(match self.wake {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }
}
