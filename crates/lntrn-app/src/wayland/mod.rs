//! What we say to Wayland ourselves, beside winit: the clipboard and
//! files dragged in from outside, over the window's own connection
//! (U018). No crate: `ffi` finds the libwayland-client winit already
//! loaded and describes the handful of objects involved; `data_device`
//! does the talking and `dnd` the drags.

mod data_device;
mod dnd;
mod ffi;
mod listeners;

pub use data_device::{Clipboard, DragData};
pub use dnd::{DragEvent, encode_uri_list, parse_uri_list};
