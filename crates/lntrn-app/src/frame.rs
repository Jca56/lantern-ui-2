//! What both harnesses share: the GPU-side objects of a window, the
//! rebuild loop, and drawing one frame into the swapchain.

use lntrn_core::log_trace;
use lntrn_math::{Rect, Vec2};
use lntrn_render::wgpu;
use lntrn_render::{DrawList, Gpu, Images, Pass2d, RenderGraph, SurfaceTarget, TexDesc, TexId, TexturePool, clear_pass};
use lntrn_text::TextEngine;
use lntrn_ui::{Event, Shell, ShellOutput, WindowState};

use crate::app::{AppHost, RenderCx};
use crate::clipboard::Clipboard;

/// A popup closing or a value committing may ask for one more rebuild; this
/// caps how many happen back to back before we present.
pub(crate) const MAX_REBUILDS: usize = 4;

/// The GPU side of one window or embedded view.
pub(crate) struct Gfx {
    pub gpu: Gpu,
    pub surface: SurfaceTarget,
    pub pass2d: Pass2d,
    pub images: Images,
    pub pool: TexturePool,
}

impl Gfx {
    pub fn new(gpu: Gpu, surface: wgpu::Surface<'static>, width: u32, height: u32, text: &TextEngine) -> Self {
        let surface = SurfaceTarget::new(&gpu, surface, width, height);
        let images = Images::new(&gpu);
        let pass2d = Pass2d::new(&gpu, surface.format(), text.atlas(), &images);
        Self { gpu, surface, pass2d, images, pool: TexturePool::new() }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface.resize(&self.gpu, width, height);
        self.pool.trim();
    }

    pub fn window_rect(&self) -> Rect {
        let size = self.surface.size();
        Rect::from_min_size(Vec2::ZERO, Vec2::new(size[0] as f64, size[1] as f64))
    }
}

/// Rebuild the UI from `events` (possibly more than once) and resolve the
/// host's GPU-side queries between rebuilds. Returns the outputs merged
/// (`quit` from any, the first window command) and whether work is still
/// pending because the cap was reached.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebuild<H: AppHost>(gfx: &mut Gfx, host: &mut H, shell: &mut Shell<H>, text: &mut TextEngine, draw: &mut DrawList, events: &[Event], scale: f64, ws: WindowState, clipboard: &mut Clipboard) -> (ShellOutput, bool) {
    let window_rect = gfx.window_rect();
    let mut evs: &[Event] = events;
    let mut quit = false;
    let mut command = None;
    let mut out = None;
    let mut again = true;
    for _ in 0..MAX_REBUILDS {
        // A paste is coming, or the host asked: bring the system clipboard in first.
        if (evs.iter().any(Event::is_paste) || std::mem::take(&mut shell.state.clipboard_wanted))
            && let Some(text) = clipboard.read()
        {
            shell.state.clipboard = text;
        }
        draw.clear();
        let o = shell.frame(host, evs, window_rect, scale, ws, text, draw);
        again = o.rebuild_again || shell.state.clipboard_wanted;
        quit |= o.quit;
        command = command.or(o.window_command);
        evs = &[];
        // GPU-side queries resolve right away, then one more rebuild draws
        // their result in this same frame. Doing it here (not after
        // present) also means a failed swapchain acquire cannot swallow a
        // click.
        if host.after_rebuild(&gfx.gpu, &mut gfx.images, shell) {
            again = true;
        }
        out = Some(o);
        if !again {
            break;
        }
    }
    let out = out.expect("at least one rebuild");
    // A widget copied: push it out to the system.
    if shell.state.take_clipboard_dirty() && !clipboard.write(&shell.state.clipboard) {
        log_trace!("clipboard: no system clipboard, kept in-app");
    }
    (ShellOutput { quit, window_command: command, ..out }, again)
}

/// Draw the list into the swapchain and present. `false` when no frame
/// could be acquired (the caller should try again soon). `before_present`
/// runs between the queue submit and the present (winit's notify).
pub(crate) fn draw_frame<H: AppHost>(gfx: &mut Gfx, host: &mut H, text: &mut TextEngine, draw: &DrawList, clear: lntrn_math::Color, before_present: impl FnOnce()) -> bool {
    let size = gfx.surface.size();
    let Some(frame) = gfx.surface.acquire(&gfx.gpu) else {
        return false;
    };
    let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gfx.gpu.create_encoder("lantern frame");

    let (pass2d, images) = (&mut gfx.pass2d, &gfx.images);
    let mut graph = RenderGraph::new();
    let backbuffer = graph.import(&view);
    let depth = host.wants_depth().then(|| graph.transient(TexDesc::depth("depth", size[0], size[1])));
    let writes: Vec<TexId> = std::iter::once(backbuffer).chain(depth).collect();
    graph.add_node("clear", &[], &writes, move |_, enc, views| {
        clear_pass(enc, views.get(backbuffer), depth.map(|d| views.get(d)), clear);
    });
    host.render(&mut RenderCx { gpu: &gfx.gpu, graph: &mut graph, backbuffer, depth, size });
    graph.add_node("ui", &[], &[backbuffer], move |gpu, enc, views| {
        pass2d.draw(gpu, enc, views.get(backbuffer), size, draw, text.atlas_mut(), images, None);
    });
    graph.execute(&gfx.gpu, &mut gfx.pool, &mut encoder);
    gfx.pool.end_frame();

    gfx.gpu.queue.submit([encoder.finish()]);
    before_present();
    frame.present();
    log_trace!("frame: {} vertices", draw.vertex_count());
    true
}
