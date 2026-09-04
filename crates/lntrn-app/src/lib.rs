//! The winit harness: a window with the shell's own frame, GPU wiring,
//! event translation, and the rebuild → draw → present cycle. Nothing here
//! knows what a widget is.
//!
//! ```ignore
//! let shell = Shell::new(MyEditor::Main);
//! lntrn_app::run(AppConfig { title: "Mine".into(), ..Default::default() }, MyHost::new(), shell);
//! ```
//!
//! Redraw is on demand: an idle app sits at 0% CPU/GPU. A thread with
//! something to show wakes the loop through the [`Waker`] the host is
//! handed at start. A host that draws its own GPU content (a 3D view)
//! implements the hooks on [`AppHost`].
//!
//! [`Embedded`] is the same shell without winit, drawn into a window
//! somebody else owns (a plugin editor): the owner supplies raw window
//! handles and events.

mod app;
pub mod clipboard;
pub mod embed;
mod frame;
pub mod translate;
mod win;
#[cfg(target_os = "linux")]
pub mod wayland;

pub use app::{AppConfig, AppHost, RenderCx, Wake, Waker, run};
pub use embed::{EmbedConfig, EmbedOutput, Embedded};
/// Re-exported so a host can name GPU types without depending on
/// `lntrn-render` itself.
pub use lntrn_render::{self, wgpu};
