//! The winit application: any number of windows sharing one host and one
//! GPU, each with a shell of its own, and the rebuild → draw → present
//! cycle of each (see `win.rs`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lntrn_core::{log_error, log_info};
use lntrn_render::wgpu;
use lntrn_render::{DrawList, Gpu, Images, RenderGraph, TexId};
use lntrn_text::TextEngine;
use lntrn_ui::persist;
use lntrn_ui::{Host, NewWindow, Shell};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::frame::GpuShared;
use crate::win::Win;

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
/// How long an idle window sleeps between heartbeats while it has a system
/// clipboard to serve (see [`App::about_to_wait`]).
const IDLE_HEARTBEAT: Duration = Duration::from_secs(3600);

/// The event the loop is woken with from another thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wake;

/// Wakes the event loop from any thread: every window rebuilds as if
/// input had arrived, so a thread that produced something to show (a
/// terminal's output, a finished export) need not be polled. Cheap to
/// clone and hand around; wakes coalesce until the loop has turned.
#[derive(Clone)]
pub struct Waker {
    proxy: EventLoopProxy<Wake>,
    pending: Arc<AtomicBool>,
}

impl Waker {
    pub fn wake(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            let _ = self.proxy.send_event(Wake);
        }
    }
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Waker")
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
    /// Which window is being drawn: 0 is the main one, the rest count up
    /// as they open (see [`lntrn_ui::ShellRequest::OpenWindow`]).
    pub window: u32,
}

/// A [`Host`] that also takes part in the GPU frame. Every hook has a
/// do-nothing default; a UI-only app implements none of them.
pub trait AppHost: Host + Sized + 'static {
    /// The loop's waker, handed over once before the first window opens.
    /// Keep it for threads that have something to show. The embedded
    /// harness has no loop of its own and never calls this.
    fn waker(&mut self, waker: Waker) {
        let _ = waker;
    }
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
pub fn run<H: AppHost>(config: AppConfig, mut host: H, mut shell: Shell<H>) {
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
    let event_loop = EventLoop::<Wake>::with_user_event().build().expect("create event loop");
    // Redraw on demand: an idle app sits at 0% CPU/GPU.
    event_loop.set_control_flow(ControlFlow::Wait);
    let pending = Arc::new(AtomicBool::new(false));
    host.waker(Waker { proxy: event_loop.create_proxy(), pending: Arc::clone(&pending) });
    let mut app = App::new(config, host, shell, pending);
    if let Err(e) = event_loop.run_app(&mut app) {
        log_error!("event loop: {e}");
    }
}

struct App<H: AppHost> {
    config: AppConfig,
    /// The GPU, once the first window brought it up.
    shared: Option<GpuShared>,
    /// The windows, the main one first.
    wins: Vec<Win<H>>,
    text: TextEngine,
    draw: DrawList,
    host: H,
    /// The main window's shell, until the window exists.
    shell: Option<Shell<H>>,
    next_id: u32,
    quit: bool,
    /// A wake is on its way through the loop (see [`Waker`]).
    wake_pending: Arc<AtomicBool>,
}

impl<H: AppHost> App<H> {
    fn new(config: AppConfig, host: H, shell: Shell<H>, wake_pending: Arc<AtomicBool>) -> Self {
        let t = std::time::Instant::now();
        let text = TextEngine::new(&config.sans, &config.mono);
        log_info!("fonts: {} faces in {:.0} ms", text.face_count(), t.elapsed().as_secs_f64() * 1000.0);
        Self { config, shared: None, wins: Vec::new(), text, draw: DrawList::new(), host, shell: Some(shell), next_id: 0, quit: false, wake_pending }
    }

    /// Write preferences and the main window's layout for next time.
    fn save_state(&self) {
        if !self.config.persist {
            return;
        }
        let Some(shell) = self.wins.first().map(|w| &w.shell).or(self.shell.as_ref()) else {
            return;
        };
        let Some(dir) = persist::config_dir(&self.config.app_id) else {
            return;
        };
        if let Err(e) = persist::save(&dir.join(PREFS_FILE), &shell.prefs) {
            log_error!("saving preferences: {e}");
        }
        if let Err(e) = persist::save_text(&dir.join(LAYOUT_FILE), &shell.layout_description(&self.host)) {
            log_error!("saving layout: {e}");
        }
    }

    fn attrs(&self, title: &str, size: (f64, f64), maximized: bool) -> WindowAttributes {
        let c = &self.config;
        let attrs = Window::default_attributes().with_title(title).with_decorations(c.decorations).with_inner_size(LogicalSize::new(size.0, size.1)).with_maximized(maximized);
        // The app id pairs the window with its desktop entry and icon under
        // Wayland; the X11 class does the same job there.
        #[cfg(target_os = "linux")]
        let attrs = {
            let attrs = winit::platform::wayland::WindowAttributesExtWayland::with_name(attrs, &c.app_id, &c.app_id);
            winit::platform::x11::WindowAttributesExtX11::with_name(attrs, &c.app_id, &c.app_id)
        };
        attrs
    }

    fn open(&mut self, event_loop: &ActiveEventLoop, attrs: WindowAttributes, shell: Shell<H>) {
        match Win::open(event_loop, attrs, &mut self.shared, &self.text, &mut self.host, shell, self.next_id) {
            Some(w) => {
                self.next_id += 1;
                self.wins.push(w);
            }
            None if self.wins.is_empty() => event_loop.exit(),
            None => {}
        }
    }

    /// A window the host asked for: a shell of its own with the layout it
    /// named, the main window's preferences, and its own title.
    fn open_requested(&mut self, event_loop: &ActiveEventLoop, new: NewWindow) {
        let Some(first) = self.host.editors().first().copied() else {
            return;
        };
        let mut shell = Shell::new(first);
        if !shell.restore_layout(&self.host, &new.layout) {
            log_error!("new window: unreadable layout {:?}", new.layout);
        }
        if let Some(main) = self.wins.first() {
            shell.prefs = main.shell.prefs.clone();
        }
        shell.title = Some(new.title.clone());
        let attrs = self.attrs(&new.title, new.size.unwrap_or(self.config.size), false);
        self.open(event_loop, attrs, shell);
    }

    /// Rebuild, draw and present window `i`, then carry out what its frame
    /// asked for: preferences it changed reach every window, a close
    /// closes it (the main window's quits), new windows open.
    fn render(&mut self, event_loop: &ActiveEventLoop, i: usize) {
        let Some(shared) = self.shared.as_mut() else {
            return;
        };
        let before = self.wins[i].shell.prefs.clone();
        let outcome = self.wins[i].render(&mut self.host, shared, &mut self.text, &mut self.draw);
        if self.wins[i].shell.prefs != before {
            let prefs = self.wins[i].shell.prefs.clone();
            for (j, w) in self.wins.iter_mut().enumerate() {
                if j != i {
                    w.shell.prefs = prefs.clone();
                    w.dirty = true;
                }
            }
        }
        if outcome.quit || (outcome.close && i == 0) {
            self.quit = true;
            event_loop.exit();
            return;
        }
        if outcome.close {
            self.wins.remove(i);
        }
        for new in outcome.new_windows {
            self.open_requested(event_loop, new);
        }
    }
}

impl<H: AppHost> ApplicationHandler<Wake> for App<H> {
    /// Another thread has something to show: every window rebuilds.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: Wake) {
        self.wake_pending.store(false, Ordering::Release);
        for w in &mut self.wins {
            w.dirty = true;
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.wins.is_empty()
            && let Some(shell) = self.shell.take()
        {
            let attrs = self.attrs(&self.config.title, self.config.size, self.config.maximized);
            self.open(event_loop, attrs, shell);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(i) = self.wins.iter().position(|w| w.window.id() == id) else {
            return;
        };
        if let Some(shared) = &self.shared {
            self.wins[i].handle(&event, shared);
        }
        // A close request travels through the shell as an event, so the
        // host can keep the window and ask about unsaved work first.
        if let WindowEvent::RedrawRequested = event {
            self.render(event_loop, i);
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // An animation's wake-up: rebuild the windows it was for. The idle
        // heartbeat is not one.
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            let now = Instant::now();
            for w in &mut self.wins {
                if w.wake.is_some_and(|at| at <= now) {
                    w.wake = None;
                    w.dirty = true;
                }
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_state();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The clipboards' own queues: another app pasting what we copied,
        // or files being dragged in.
        for w in &mut self.wins {
            w.poll();
            if w.dirty {
                w.dirty = false;
                w.window.request_redraw();
            }
        }
        if self.quit {
            return;
        }
        // Sleep until input, or until the soonest running animation's next
        // frame. With a system clipboard to serve, never plain `Wait`:
        // winit skips this callback when a wake-up brought it no events of
        // its own, and another app's paste request is exactly that (it
        // lands on the clipboard's queue alone). A far deadline keeps every
        // wake-up reaching the poll above; the deadline itself costs one
        // no-op wake an hour.
        let heartbeat = self.wins.iter().any(|w| w.clipboard.available());
        let wake = self.wins.iter().filter_map(|w| w.wake).min();
        event_loop.set_control_flow(match wake {
            Some(at) => ControlFlow::WaitUntil(at),
            None if heartbeat => ControlFlow::WaitUntil(Instant::now() + IDLE_HEARTBEAT),
            None => ControlFlow::Wait,
        });
    }
}
