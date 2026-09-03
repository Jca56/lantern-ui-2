//! Files dragged in from outside, over the same data device as the
//! clipboard. On enter we accept `text/uri-list`, negotiate a copy, and
//! start reading the list right away (the protocol allows it before the
//! drop), so the files are known while the drag is still hovering; on
//! drop they go out as [`DragEvent::Dropped`], on leave as
//! [`DragEvent::Left`]. Positions come with every motion, in the
//! surface's logical pixels.

use std::ffi::CStr;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::ptr::null;
use std::time::Duration;

use super::ffi::{self, Interface, Lib, Proxy, dnd_action, op};

const URI_LIST: &CStr = c"text/uri-list";
/// How long a drop waits for the rest of a list still being read.
const DROP_TIMEOUT: Duration = Duration::from_millis(1000);

/// What a drag did, for the harness to turn into events.
#[derive(Clone, Debug, PartialEq)]
pub enum DragEvent {
    /// The pointer, in logical surface pixels.
    Moved(f64, f64),
    /// The files being dragged, once the list has been read.
    Hovered(Vec<PathBuf>),
    Left,
    Dropped(Vec<PathBuf>),
}

/// A drag over the window that offers files.
pub(super) struct Drag {
    pub offer: *mut Proxy,
    /// The list, read in the background while the drag hovers.
    reader: Option<UnixStream>,
    bytes: Vec<u8>,
    /// The files, once the list is in.
    paths: Option<Vec<PathBuf>>,
}

/// The drag-and-drop side of the shared state.
#[derive(Default)]
pub(super) struct DndState {
    pub drag: Option<Drag>,
    pub events: Vec<DragEvent>,
}

impl DndState {
    /// A drag came in over our surface with `offer` (or none) at `pos`.
    /// `mimes` are what the offer can give. Accepts when there is a file
    /// list to be had; otherwise tells the compositor we take nothing.
    ///
    /// # Safety
    /// `offer` is a live proxy of ours (or null).
    pub unsafe fn enter(&mut self, lib: &Lib, version: u32, serial: u32, pos: (f64, f64), offer: *mut Proxy, mimes: &[String]) {
        let (x, y) = pos;
        self.drag = None;
        if offer.is_null() {
            return;
        }
        let wanted = mimes.iter().any(|m| m.as_bytes() == URI_LIST.to_bytes());
        // SAFETY: `accept` is `u?s`, `set_actions` is `uu`, `receive` is `sh`.
        unsafe {
            if !wanted {
                (lib.marshal)(offer, op::OFFER_ACCEPT, null::<Interface>(), version, 0, serial, null::<u8>());
                if version >= 3 {
                    (lib.marshal)(offer, op::OFFER_SET_ACTIONS, null::<Interface>(), version, 0, 0u32, 0u32);
                }
                return;
            }
            (lib.marshal)(offer, op::OFFER_ACCEPT, null::<Interface>(), version, 0, serial, URI_LIST.as_ptr());
            if version >= 3 {
                (lib.marshal)(offer, op::OFFER_SET_ACTIONS, null::<Interface>(), version, 0, dnd_action::COPY | dnd_action::MOVE, dnd_action::COPY);
            }
            let reader = UnixStream::pair().ok().and_then(|(ours, theirs)| {
                (lib.marshal)(offer, op::OFFER_RECEIVE, null::<Interface>(), version, 0, URI_LIST.as_ptr(), theirs.as_raw_fd());
                ours.set_nonblocking(true).ok().map(|_| ours)
            });
            self.drag = Some(Drag { offer, reader, bytes: Vec::new(), paths: None });
        }
        self.events.push(DragEvent::Moved(x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if self.drag.is_some() {
            self.events.push(DragEvent::Moved(x, y));
        }
    }

    /// Read what has arrived of the list without waiting; once it is all
    /// in, the files are known and reported as hovering.
    pub fn pump(&mut self) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if let Some(reader) = drag.reader.as_mut() {
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => {
                        drag.reader = None;
                        break;
                    }
                    Ok(n) => drag.bytes.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        drag.reader = None;
                        break;
                    }
                }
            }
        }
        if drag.reader.is_none() && drag.paths.is_none() {
            let paths = parse_uri_list(&drag.bytes);
            self.events.push(DragEvent::Hovered(paths.clone()));
            drag.paths = Some(paths);
        }
    }

    /// The drag left without dropping.
    ///
    /// # Safety
    /// The drag's offer is still live.
    pub unsafe fn leave(&mut self, lib: &Lib, version: u32) {
        if let Some(drag) = self.drag.take() {
            self.events.push(DragEvent::Left);
            unsafe { destroy(lib, version, drag.offer) };
        }
    }

    /// The files were let go over us: finish reading the list if it is
    /// still on its way, report the drop, and close the offer.
    ///
    /// # Safety
    /// The drag's offer is still live.
    pub unsafe fn drop(&mut self, lib: &Lib, version: u32) {
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        if let Some(mut reader) = drag.reader.take() {
            let _ = reader.set_nonblocking(false);
            let _ = reader.set_read_timeout(Some(DROP_TIMEOUT));
            let _ = reader.read_to_end(&mut drag.bytes);
        }
        let paths = drag.paths.take().unwrap_or_else(|| parse_uri_list(&drag.bytes));
        // SAFETY: `finish` has no arguments; the offer goes after it.
        unsafe {
            if version >= 3 && !paths.is_empty() {
                (lib.marshal)(drag.offer, op::OFFER_FINISH, null::<Interface>(), version, 0);
            }
            destroy(lib, version, drag.offer);
        }
        if !paths.is_empty() {
            self.events.push(DragEvent::Dropped(paths));
        } else {
            self.events.push(DragEvent::Left);
        }
    }
}

unsafe fn destroy(lib: &Lib, version: u32, offer: *mut Proxy) {
    // SAFETY: `destroy` is wl_data_offer's destructor.
    unsafe { (lib.marshal)(offer, op::OFFER_DESTROY, null::<Interface>(), version, ffi::MARSHAL_FLAG_DESTROY) };
}

/// The `file://` entries of a `text/uri-list`, as paths. Comments and
/// other schemes are skipped; percent-escapes are decoded.
pub fn parse_uri_list(bytes: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let rest = l.strip_prefix("file://")?;
            // `file:///path` or `file://host/path`: keep from the first slash.
            let path = if rest.starts_with('/') { rest } else { &rest[rest.find('/')?..] };
            Some(PathBuf::from(percent_decode(path)))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_lists_become_paths() {
        let list = b"# dragged from the file manager\r\nfile:///home/alva/Pictures/a%20b.png\r\nfile://localhost/tmp/c.txt\r\nhttps://example.com/no\r\n\r\nfile:///caf%C3%A9.jpg\n";
        let paths = parse_uri_list(list);
        assert_eq!(paths, vec![PathBuf::from("/home/alva/Pictures/a b.png"), PathBuf::from("/tmp/c.txt"), PathBuf::from("/café.jpg")]);
        assert!(parse_uri_list(b"").is_empty());
        assert_eq!(percent_decode("100%"), "100%", "a stray percent stays");
        assert_eq!(percent_decode("%2"), "%2");
    }
}
