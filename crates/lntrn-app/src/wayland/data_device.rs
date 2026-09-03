//! The clipboard and drags over the app's own Wayland connection, the
//! way any Wayland app does it: a `wl_data_device` on the seat, a
//! `wl_keyboard` and a `wl_pointer` of our own for the serials, a
//! `wl_data_source` to give and a `wl_data_offer` to take. Everything
//! lives on a private event queue that the harness dispatches once per
//! loop turn, so nothing here ever blocks winit, and no other window is
//! involved, so focus never moves. The callbacks are in [`super::listeners`].

use std::collections::HashMap;
use std::ffi::{CStr, c_int, c_void};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::Duration;

use lntrn_core::log_info;

use super::dnd::{DndState, DragEvent, URI_LIST, encode_uri_list};
use super::ffi::{self, Interface, Lib, Proxy, Queue, dnd_action, op};
use super::listeners::*;

/// The text types we give and take, best first.
const MIMES: [&CStr; 5] = [c"text/plain;charset=utf-8", c"UTF8_STRING", c"text/plain", c"TEXT", c"STRING"];
/// The picture type we give and take.
const PNG: &CStr = c"image/png";
/// How long a paste waits for the other app to write.
const READ_TIMEOUT: Duration = Duration::from_millis(1500);
/// Pictures take longer.
const IMAGE_TIMEOUT: Duration = Duration::from_millis(4000);

/// What the listeners write into. Boxed and handed to libwayland as user
/// data, so it never moves while a proxy can still fire.
pub(super) struct Shared {
    pub lib: Lib,
    pub display: *mut c_void,
    pub queue: *mut Queue,
    pub seat: *mut Proxy,
    pub manager: *mut Proxy,
    /// The data device manager's version, which its devices, sources and
    /// offers share: 3 when the compositor has it (drag-and-drop with
    /// actions), else what it offers.
    pub version: u32,
    /// Serial of the latest keyboard event: what `set_selection` wants.
    pub serial: u32,
    /// Serial of the latest button press, and whether a button is still
    /// down: what `start_drag` wants.
    pub pointer_serial: u32,
    pub pointer_down: bool,
    /// Offers the compositor made, with the types each can give.
    pub offers: HashMap<usize, Vec<String>>,
    /// The offer that is the selection right now.
    pub selection: Option<*mut Proxy>,
    /// Files dragged in from outside, and what our own drags report.
    pub dnd: DndState,
    /// The source we set as the selection while it stands, with its data.
    pub source: Option<(*mut Proxy, *mut SourceData)>,
    /// The source of the drag we started, until it finishes or is cancelled.
    pub drag_source: Option<(*mut Proxy, *mut SourceData)>,
    /// What our source holds, so a paste of our own copy needs no pipe:
    /// text, or a PNG.
    pub owned: Option<Owned>,
}

pub(super) enum Owned {
    Text(String),
    Png(Vec<u8>),
}

/// Types a source offers with bytes of their own.
type Extra = Vec<(&'static CStr, Vec<u8>)>;

/// What one `wl_data_source` of ours carries: `bytes` for every type it
/// offers, unless `extra` names other bytes for one of them.
pub(super) struct SourceData {
    pub bytes: Vec<u8>,
    pub extra: Extra,
    pub shared: *mut Shared,
}

/// What a drag out of the window offers.
pub enum DragData {
    Text(String),
    Files(Vec<PathBuf>),
    /// A PNG, and the file it was written to, for the apps that take
    /// files rather than pictures.
    Png { png: Vec<u8>, file: Option<PathBuf> },
}

/// The clipboard of one window. Not `Send`: it belongs to the thread that
/// runs the window's event loop.
pub struct Clipboard {
    shared: *mut Shared,
    wrapper: *mut c_void,
    registry: *mut Proxy,
    keyboard: *mut Proxy,
    pointer: *mut Proxy,
    device: *mut Proxy,
    /// The window's `wl_surface`: where a drag starts from.
    surface: *mut Proxy,
}

// ---- the clipboard ------------------------------------------------------------

impl Clipboard {
    /// Join the connection behind `display` (a `wl_display*`), find the
    /// seat and the data device manager, and start listening. `surface`
    /// is the window's `wl_surface*` (null: no drags out).
    ///
    /// # Safety
    /// `display` must be a live libwayland-client `wl_display` that stays
    /// alive for as long as the clipboard does, `surface` a surface of it
    /// or null, and the clipboard must be used only from the thread that
    /// dispatches that display.
    pub unsafe fn new(display: *mut c_void, surface: *mut c_void) -> Option<Clipboard> {
        let lib = Lib::load()?;
        unsafe {
            let queue = (lib.create_queue)(display);
            if queue.is_null() {
                return None;
            }
            let wrapper = (lib.create_wrapper)(display);
            (lib.set_queue)(wrapper as *mut Proxy, queue);
            let registry = (lib.marshal)(wrapper as *mut Proxy, op::DISPLAY_GET_REGISTRY, &ffi::WL_REGISTRY, 1, 0, null_mut::<Proxy>());
            let shared = Box::into_raw(Box::new(Shared {
                lib,
                display,
                queue,
                seat: null_mut(),
                manager: null_mut(),
                version: 1,
                serial: 0,
                pointer_serial: 0,
                pointer_down: false,
                offers: HashMap::new(),
                selection: None,
                dnd: DndState::default(),
                source: None,
                drag_source: None,
                owned: None,
            }));
            (lib.add_listener)(registry, &REGISTRY_LISTENER as *const RegistryListener as *const c_void, shared as *mut c_void);
            let mut me = Clipboard { shared, wrapper, registry, keyboard: null_mut(), pointer: null_mut(), device: null_mut(), surface: surface as *mut Proxy };
            if (lib.roundtrip_queue)(display, queue) < 0 {
                return None;
            }
            let (seat, manager) = ((*shared).seat, (*shared).manager);
            if seat.is_null() || manager.is_null() {
                log_info!("clipboard: the compositor offers no seat or data device manager");
                return None;
            }
            (lib.add_listener)(seat, &SEAT_LISTENER as *const SeatListener as *const c_void, shared as *mut c_void);
            me.keyboard = (lib.marshal)(seat, op::SEAT_GET_KEYBOARD, &ffi::WL_KEYBOARD, 1, 0, null_mut::<Proxy>());
            (lib.add_listener)(me.keyboard, &KEYBOARD_LISTENER as *const KeyboardListener as *const c_void, shared as *mut c_void);
            me.pointer = (lib.marshal)(seat, op::SEAT_GET_POINTER, &ffi::WL_POINTER, 1, 0, null_mut::<Proxy>());
            (lib.add_listener)(me.pointer, &POINTER_LISTENER as *const PointerListener as *const c_void, shared as *mut c_void);
            let version = (*shared).version;
            me.device = (lib.marshal)(manager, op::DDM_GET_DATA_DEVICE, &ffi::WL_DATA_DEVICE, version, 0, null_mut::<Proxy>(), seat);
            (lib.add_listener)(me.device, &DEVICE_LISTENER as *const DeviceListener as *const c_void, shared as *mut c_void);
            // The keymap, the focus and the selection as they stand.
            (lib.roundtrip_queue)(display, queue);
            Some(me)
        }
    }

    /// Handle what the compositor sent our queue since last time, and send
    /// what we owe it. Once per loop turn.
    pub fn poll(&mut self) {
        // SAFETY: the display and queue are ours for as long as `self` is.
        unsafe {
            let s = &mut *self.shared;
            (s.lib.dispatch_queue_pending)(s.display, s.queue);
            s.dnd.pump();
            (s.lib.flush)(s.display);
        }
    }

    /// What a drag from outside, or one of ours, did since last time.
    pub fn take_drag_events(&mut self) -> Vec<DragEvent> {
        // SAFETY: `shared` lives as long as `self`.
        unsafe { std::mem::take(&mut (*self.shared).dnd.events) }
    }

    /// The selection's text, if it holds any.
    pub fn read(&mut self) -> Option<String> {
        self.poll();
        // SAFETY: `shared` lives as long as `self`.
        let shared = unsafe { &mut *self.shared };
        if let Some(Owned::Text(text)) = &shared.owned {
            return Some(text.clone());
        }
        let have = shared.offers.get(&(shared.selection? as usize))?;
        let mime = *MIMES.iter().find(|m| have.iter().any(|h| h.as_bytes() == m.to_bytes()))?;
        String::from_utf8(receive(shared, mime, READ_TIMEOUT)?).ok()
    }

    /// The selection as a PNG, if it offers one.
    pub fn read_image(&mut self) -> Option<Vec<u8>> {
        self.poll();
        // SAFETY: `shared` lives as long as `self`.
        let shared = unsafe { &mut *self.shared };
        if let Some(Owned::Png(png)) = &shared.owned {
            return Some(png.clone());
        }
        let have = shared.offers.get(&(shared.selection? as usize))?;
        have.iter().any(|h| h.as_bytes() == PNG.to_bytes()).then(|| receive(shared, PNG, IMAGE_TIMEOUT)).flatten()
    }

    /// Make `text` the selection. `false` when the window has never had
    /// keyboard focus (the compositor would not take it).
    pub fn write(&mut self, text: &str) -> bool {
        self.offer_bytes(&MIMES, text.as_bytes().to_vec(), Owned::Text(text.to_owned()))
    }

    /// Make a PNG the selection.
    pub fn write_image(&mut self, png: &[u8]) -> bool {
        self.offer_bytes(&[PNG], png.to_vec(), Owned::Png(png.to_vec()))
    }

    fn offer_bytes(&mut self, mimes: &[&'static CStr], bytes: Vec<u8>, owned: Owned) -> bool {
        self.poll();
        // SAFETY: `shared` lives as long as `self`; every proxy touched is
        // live, and the source's data outlives the source.
        unsafe {
            let shared = &mut *self.shared;
            if shared.serial == 0 {
                return false;
            }
            if let Some((old, data)) = shared.source.take() {
                destroy_source(shared, old, data);
            }
            let Some((source, data)) = self.make_source(mimes, bytes, Vec::new()) else {
                return false;
            };
            (shared.lib.marshal)(self.device, op::DEVICE_SET_SELECTION, null::<Interface>(), shared.version, 0, source, shared.serial);
            (shared.lib.flush)(shared.display);
            shared.source = Some((source, data));
            shared.owned = Some(owned);
        }
        true
    }

    /// Start dragging `data` out of the window: whatever window the
    /// pointer lets go over gets it, as a copy. The button must be down
    /// (the compositor grabs the pointer from that press). `false` when
    /// no drag can start: no press seen, or no surface to start from.
    pub fn start_drag(&mut self, data: DragData) -> bool {
        self.poll();
        // SAFETY: as for `offer_bytes`; the surface is the window's.
        unsafe {
            let shared = &mut *self.shared;
            if self.surface.is_null() || !shared.pointer_down || shared.pointer_serial == 0 {
                return false;
            }
            // A drag that never finished (a version 1 compositor says
            // nothing after the drop) goes when the next one starts.
            if let Some((old, data)) = shared.drag_source.take() {
                destroy_source(shared, old, data);
            }
            let (mimes, bytes, extra): (Vec<&'static CStr>, Vec<u8>, Extra) = match data {
                DragData::Text(text) => (MIMES.to_vec(), text.into_bytes(), Vec::new()),
                DragData::Files(paths) => (vec![URI_LIST], encode_uri_list(&paths), Vec::new()),
                DragData::Png { png, file } => {
                    let extra = file.map(|f| (URI_LIST, encode_uri_list(&[f]))).into_iter().collect();
                    (vec![PNG], png, extra)
                }
            };
            let Some((source, data)) = self.make_source(&mimes, bytes, extra) else {
                return false;
            };
            if shared.version >= 3 {
                (shared.lib.marshal)(source, op::SOURCE_SET_ACTIONS, null::<Interface>(), shared.version, 0, dnd_action::COPY);
            }
            // `start_drag(source, origin, icon, serial)`: no icon, so the
            // pointer alone shows the drag.
            (shared.lib.marshal)(self.device, op::DEVICE_START_DRAG, null::<Interface>(), shared.version, 0, source, self.surface, null_mut::<Proxy>(), shared.pointer_serial);
            (shared.lib.flush)(shared.display);
            shared.drag_source = Some((source, data));
        }
        true
    }

    /// A data source offering `mimes`, carrying `bytes` (and `extra` for
    /// the types that differ), with our listener on it.
    ///
    /// # Safety
    /// The manager is live; the returned data is freed by the listener's
    /// end events or by whoever destroys the source.
    unsafe fn make_source(&self, mimes: &[&'static CStr], bytes: Vec<u8>, extra: Extra) -> Option<(*mut Proxy, *mut SourceData)> {
        unsafe {
            let shared = &*self.shared;
            let source = (shared.lib.marshal)(shared.manager, op::DDM_CREATE_DATA_SOURCE, &ffi::WL_DATA_SOURCE, shared.version, 0, null_mut::<Proxy>());
            if source.is_null() {
                return None;
            }
            let extra_mimes: Vec<&'static CStr> = extra.iter().map(|(m, _)| *m).collect();
            let data = Box::into_raw(Box::new(SourceData { bytes, extra, shared: self.shared }));
            (shared.lib.add_listener)(source, &SOURCE_LISTENER as *const SourceListener as *const c_void, data as *mut c_void);
            for mime in mimes.iter().chain(&extra_mimes) {
                (shared.lib.marshal)(source, op::SOURCE_OFFER, null::<Interface>(), shared.version, 0, mime.as_ptr());
            }
            Some((source, data))
        }
    }
}

/// Destroy a source of ours and free its data.
///
/// # Safety
/// `source` is live and `data` is its user data, which nothing else holds.
unsafe fn destroy_source(shared: &Shared, source: *mut Proxy, data: *mut SourceData) {
    unsafe {
        (shared.lib.marshal)(source, op::SOURCE_DESTROY, null::<Interface>(), shared.version, ffi::MARSHAL_FLAG_DESTROY);
        drop(Box::from_raw(data));
    }
}

/// Ask the selection's offer for `mime` and read until the other app
/// closes its end, or `timeout`. Nothing at all means a dead offer (its
/// source went away while we were not focused to hear about it).
fn receive(shared: &mut Shared, mime: &CStr, timeout: Duration) -> Option<Vec<u8>> {
    let offer = shared.selection?;
    let (mut ours, theirs) = UnixStream::pair().ok()?;
    // SAFETY: `receive` takes a mime and an fd; libwayland dups the fd.
    unsafe {
        (shared.lib.marshal)(offer, op::OFFER_RECEIVE, null::<Interface>(), shared.version, 0, mime.as_ptr(), theirs.as_raw_fd() as c_int);
        (shared.lib.flush)(shared.display);
    }
    drop(theirs);
    let _ = ours.set_read_timeout(Some(timeout));
    let mut bytes = Vec::new();
    // A timeout leaves what arrived in `bytes`; that is the best we get.
    let _ = ours.read_to_end(&mut bytes);
    (!bytes.is_empty()).then_some(bytes)
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: proxies go before the queue and the shared state they
        // point at; nothing fires after `destroy`.
        unsafe {
            let shared = Box::from_raw(self.shared);
            let lib = shared.lib;
            let version = shared.version;
            for (source, data) in shared.source.into_iter().chain(shared.drag_source) {
                destroy_source(&shared, source, data);
            }
            for offer in shared.offers.keys() {
                (lib.marshal)(*offer as *mut Proxy, op::OFFER_DESTROY, null::<Interface>(), version, ffi::MARSHAL_FLAG_DESTROY);
            }
            for proxy in [self.device, self.keyboard, self.pointer, shared.seat, shared.manager, self.registry] {
                if !proxy.is_null() {
                    (lib.destroy)(proxy);
                }
            }
            (lib.wrapper_destroy)(self.wrapper);
            (lib.flush)(shared.display);
            (lib.queue_destroy)(shared.queue);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_char;

    use super::*;

    /// Joins the running compositor on a fresh connection (no window, so
    /// nothing shows and nothing can be copied), binds what the clipboard
    /// needs, and leaves. Needs a Wayland session:
    /// `cargo test -p lntrn-app -- --ignored`.
    #[test]
    #[ignore]
    fn joins_a_display_binds_and_leaves() {
        unsafe {
            let connect: unsafe extern "C" fn(*const c_char) -> *mut c_void = std::mem::transmute(ffi::symbol(c"wl_display_connect"));
            let disconnect: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(ffi::symbol(c"wl_display_disconnect"));
            let display = connect(null());
            assert!(!display.is_null(), "a Wayland session");
            let mut cb = Clipboard::new(display, null_mut()).expect("a seat and a data device manager");
            cb.poll();
            let shared = &*cb.shared;
            assert!(!shared.seat.is_null() && !shared.manager.is_null());
            assert!(!cb.write("x"), "no window, no focus, no serial: the selection cannot be ours");
            assert_eq!(cb.read(), None, "and nothing is offered to a client without focus");
            assert!(!cb.start_drag(DragData::Text("x".into())), "no surface, no press: no drag");
            drop(cb);
            disconnect(display);
        }
    }
}
