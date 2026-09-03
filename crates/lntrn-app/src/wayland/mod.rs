//! What we say to Wayland ourselves, beside winit: the clipboard, over
//! the window's own connection (U018). No crate: `ffi` finds the
//! libwayland-client winit already loaded and describes the handful of
//! objects involved; `data_device` does the talking.

mod data_device;
mod ffi;

pub use data_device::Clipboard;
