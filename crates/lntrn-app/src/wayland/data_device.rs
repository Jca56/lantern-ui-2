//! The clipboard over the app's own Wayland connection, the way any
//! Wayland app does it: a `wl_data_device` on the seat, a `wl_keyboard`
//! of our own for the serials, a `wl_data_source` to give and a
//! `wl_data_offer` to take. Everything lives on a private event queue
//! that the harness dispatches once per loop turn, so nothing here ever
//! blocks winit, and no other window is involved, so focus never moves.

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::net::UnixStream;
use std::ptr::{null, null_mut};
use std::time::Duration;

use lntrn_core::log_info;

use super::dnd::{DndState, DragEvent};
use super::ffi::{self, Array, Interface, Lib, Proxy, Queue, op};

/// The text types we give and take, best first.
const MIMES: [&CStr; 5] = [c"text/plain;charset=utf-8", c"UTF8_STRING", c"text/plain", c"TEXT", c"STRING"];
/// How long a paste waits for the other app to write.
const READ_TIMEOUT: Duration = Duration::from_millis(1500);

/// What the listeners write into. Boxed and handed to libwayland as user
/// data, so it never moves while a proxy can still fire.
struct Shared {
    lib: Lib,
    display: *mut c_void,
    queue: *mut Queue,
    seat: *mut Proxy,
    manager: *mut Proxy,
    /// The data device manager's version, which its devices, sources and
    /// offers share: 3 when the compositor has it (drag-and-drop with
    /// actions), else what it offers.
    version: u32,
    /// Serial of the latest keyboard event: what `set_selection` wants.
    serial: u32,
    /// Offers the compositor made, with the types each can give.
    offers: HashMap<usize, Vec<String>>,
    /// The offer that is the selection right now.
    selection: Option<*mut Proxy>,
    /// Files dragged in from outside.
    dnd: DndState,
    /// The source we set as the selection while it stands, with its data.
    source: Option<(*mut Proxy, *mut SourceData)>,
    /// What our source holds, so a paste of our own copy needs no pipe.
    owned_text: Option<String>,
}

/// What one `wl_data_source` of ours carries.
struct SourceData {
    text: String,
    shared: *mut Shared,
}

/// The clipboard of one window. Not `Send`: it belongs to the thread that
/// runs the window's event loop.
pub struct Clipboard {
    shared: *mut Shared,
    wrapper: *mut c_void,
    registry: *mut Proxy,
    keyboard: *mut Proxy,
    device: *mut Proxy,
}

// ---- listeners ----------------------------------------------------------------

#[repr(C)]
struct RegistryListener {
    global: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *const c_char, u32),
    global_remove: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
}
static REGISTRY_LISTENER: RegistryListener = RegistryListener { global: registry_global, global_remove: registry_global_remove };

unsafe extern "C" fn registry_global(data: *mut c_void, registry: *mut Proxy, name: u32, interface: *const c_char, version: u32) {
    // SAFETY: `data` is the `Shared` this registry was given; libwayland
    // hands us a valid C string.
    unsafe {
        let shared = &mut *(data as *mut Shared);
        let iface = CStr::from_ptr(interface);
        if iface == c"wl_seat" && shared.seat.is_null() {
            shared.seat = bind(&shared.lib, registry, name, &ffi::WL_SEAT, 1);
        } else if iface == c"wl_data_device_manager" && shared.manager.is_null() {
            shared.version = version.min(3);
            shared.manager = bind(&shared.lib, registry, name, &ffi::WL_DATA_DEVICE_MANAGER, shared.version);
        }
    }
}

unsafe extern "C" fn registry_global_remove(_data: *mut c_void, _registry: *mut Proxy, _name: u32) {}

/// `wl_registry.bind` at `version`.
unsafe fn bind(lib: &Lib, registry: *mut Proxy, name: u32, interface: &'static Interface, version: u32) -> *mut Proxy {
    // SAFETY: the signature is `usun`: name, interface name, version, new id.
    unsafe { (lib.marshal)(registry, op::REGISTRY_BIND, interface, version, 0, name, interface.name, version, null_mut::<Proxy>()) }
}

/// `wl_fixed_t` (24.8) to a float.
fn fixed(v: i32) -> f64 {
    f64::from(v) / 256.0
}

#[repr(C)]
struct SeatListener {
    capabilities: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
    name: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char),
}
static SEAT_LISTENER: SeatListener = SeatListener { capabilities: seat_capabilities, name: seat_name };
unsafe extern "C" fn seat_capabilities(_: *mut c_void, _: *mut Proxy, _: u32) {}
unsafe extern "C" fn seat_name(_: *mut c_void, _: *mut Proxy, _: *const c_char) {}

#[repr(C)]
struct KeyboardListener {
    keymap: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, c_int, u32),
    enter: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy, *mut Array),
    leave: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy),
    key: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, u32, u32, u32),
    modifiers: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, u32, u32, u32, u32),
    repeat_info: unsafe extern "C" fn(*mut c_void, *mut Proxy, i32, i32),
}
static KEYBOARD_LISTENER: KeyboardListener = KeyboardListener { keymap: kb_keymap, enter: kb_enter, leave: kb_leave, key: kb_key, modifiers: kb_modifiers, repeat_info: kb_repeat_info };

unsafe extern "C" fn kb_keymap(_: *mut c_void, _: *mut Proxy, _format: u32, fd: c_int, _size: u32) {
    // The keymap is winit's business; the fd is ours to close.
    // SAFETY: libwayland gave us this fd and forgets it.
    drop(unsafe { File::from_raw_fd(fd) });
}
unsafe extern "C" fn kb_enter(data: *mut c_void, _: *mut Proxy, serial: u32, _: *mut Proxy, _: *mut Array) {
    unsafe { (*(data as *mut Shared)).serial = serial };
}
unsafe extern "C" fn kb_leave(data: *mut c_void, _: *mut Proxy, serial: u32, _: *mut Proxy) {
    unsafe { (*(data as *mut Shared)).serial = serial };
}
unsafe extern "C" fn kb_key(data: *mut c_void, _: *mut Proxy, serial: u32, _: u32, _: u32, _: u32) {
    unsafe { (*(data as *mut Shared)).serial = serial };
}
unsafe extern "C" fn kb_modifiers(data: *mut c_void, _: *mut Proxy, serial: u32, _: u32, _: u32, _: u32, _: u32) {
    unsafe { (*(data as *mut Shared)).serial = serial };
}
unsafe extern "C" fn kb_repeat_info(_: *mut c_void, _: *mut Proxy, _: i32, _: i32) {}

#[repr(C)]
struct DeviceListener {
    data_offer: unsafe extern "C" fn(*mut c_void, *mut Proxy, *mut Proxy),
    enter: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy, i32, i32, *mut Proxy),
    leave: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    motion: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, i32, i32),
    drop: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    selection: unsafe extern "C" fn(*mut c_void, *mut Proxy, *mut Proxy),
}
static DEVICE_LISTENER: DeviceListener = DeviceListener { data_offer: dev_data_offer, enter: dev_enter, leave: dev_leave, motion: dev_motion, drop: dev_drop, selection: dev_selection };

unsafe extern "C" fn dev_data_offer(data: *mut c_void, _: *mut Proxy, offer: *mut Proxy) {
    // SAFETY: a fresh proxy on our queue; it gets our listener and data.
    unsafe {
        let shared = &mut *(data as *mut Shared);
        (shared.lib.add_listener)(offer, &OFFER_LISTENER as *const OfferListener as *const c_void, data);
        shared.offers.insert(offer as usize, Vec::new());
    }
}
unsafe extern "C" fn dev_enter(data: *mut c_void, _: *mut Proxy, serial: u32, _surface: *mut Proxy, x: i32, y: i32, offer: *mut Proxy) {
    // SAFETY: `data` is our `Shared`; the offer (if any) is a live proxy.
    unsafe {
        let shared = &mut *(data as *mut Shared);
        let mimes = shared.offers.get(&(offer as usize)).cloned().unwrap_or_default();
        let (lib, version) = (shared.lib, shared.version);
        shared.dnd.enter(&lib, version, serial, (fixed(x), fixed(y)), offer, &mimes);
        if shared.dnd.drag.is_none() {
            shared.offers.remove(&(offer as usize));
            destroy_offer(shared, offer);
        }
    }
}
unsafe extern "C" fn dev_leave(data: *mut c_void, _: *mut Proxy) {
    unsafe {
        let shared = &mut *(data as *mut Shared);
        let (lib, version) = (shared.lib, shared.version);
        if let Some(offer) = shared.dnd.drag.as_ref().map(|d| d.offer) {
            shared.offers.remove(&(offer as usize));
        }
        shared.dnd.leave(&lib, version);
    }
}
unsafe extern "C" fn dev_motion(data: *mut c_void, _: *mut Proxy, _time: u32, x: i32, y: i32) {
    unsafe { (*(data as *mut Shared)).dnd.motion(fixed(x), fixed(y)) };
}
unsafe extern "C" fn dev_drop(data: *mut c_void, _: *mut Proxy) {
    unsafe {
        let shared = &mut *(data as *mut Shared);
        let (lib, version) = (shared.lib, shared.version);
        if let Some(offer) = shared.dnd.drag.as_ref().map(|d| d.offer) {
            shared.offers.remove(&(offer as usize));
        }
        shared.dnd.drop(&lib, version);
    }
}
unsafe extern "C" fn dev_selection(data: *mut c_void, _: *mut Proxy, offer: *mut Proxy) {
    unsafe {
        let shared = &mut *(data as *mut Shared);
        let dragging = shared.dnd.drag.as_ref().map(|d| d.offer);
        if let Some(old) = shared.selection.take()
            && old != offer
            && dragging != Some(old)
        {
            destroy_offer(shared, old);
        }
        shared.selection = (!offer.is_null()).then_some(offer);
    }
}

unsafe fn destroy_offer(shared: &mut Shared, offer: *mut Proxy) {
    shared.offers.remove(&(offer as usize));
    // SAFETY: `destroy` is wl_data_offer's destructor; the proxy goes with it.
    unsafe { (shared.lib.marshal)(offer, op::OFFER_DESTROY, null::<Interface>(), shared.version, ffi::MARSHAL_FLAG_DESTROY) };
}

#[repr(C)]
struct OfferListener {
    offer: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char),
    source_actions: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
    action: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
}
static OFFER_LISTENER: OfferListener = OfferListener { offer: offer_offer, source_actions: offer_u32, action: offer_u32 };
unsafe extern "C" fn offer_offer(data: *mut c_void, offer: *mut Proxy, mime: *const c_char) {
    unsafe {
        let shared = &mut *(data as *mut Shared);
        let mime = CStr::from_ptr(mime).to_string_lossy().into_owned();
        if let Some(list) = shared.offers.get_mut(&(offer as usize)) {
            list.push(mime);
        }
    }
}
unsafe extern "C" fn offer_u32(_: *mut c_void, _: *mut Proxy, _: u32) {}

#[repr(C)]
struct SourceListener {
    target: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char),
    send: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char, c_int),
    cancelled: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    dnd_drop_performed: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    dnd_finished: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    action: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
}
static SOURCE_LISTENER: SourceListener = SourceListener { target: source_target, send: source_send, cancelled: source_cancelled, dnd_drop_performed: source_noop, dnd_finished: source_noop, action: source_action };
unsafe extern "C" fn source_target(_: *mut c_void, _: *mut Proxy, _: *const c_char) {}
unsafe extern "C" fn source_noop(_: *mut c_void, _: *mut Proxy) {}
unsafe extern "C" fn source_action(_: *mut c_void, _: *mut Proxy, _: u32) {}
unsafe extern "C" fn source_send(data: *mut c_void, _: *mut Proxy, _mime: *const c_char, fd: c_int) {
    // Someone pastes: write our text and close the pipe so they see the end.
    // SAFETY: `data` is this source's `SourceData`; the fd is ours to close.
    unsafe {
        let src = &*(data as *mut SourceData);
        let mut file = File::from_raw_fd(fd);
        let _ = file.write_all(src.text.as_bytes());
    }
}
unsafe extern "C" fn source_cancelled(data: *mut c_void, source: *mut Proxy) {
    // Another app took the selection: this source is done.
    unsafe {
        let src = Box::from_raw(data as *mut SourceData);
        let shared = &mut *src.shared;
        if shared.source.is_some_and(|(p, _)| p == source) {
            shared.source = None;
            shared.owned_text = None;
        }
        (shared.lib.marshal)(source, op::SOURCE_DESTROY, null::<Interface>(), shared.version, ffi::MARSHAL_FLAG_DESTROY);
        drop(src);
    }
}

// ---- the clipboard ------------------------------------------------------------

impl Clipboard {
    /// Join the connection behind `display` (a `wl_display*`), find the
    /// seat and the data device manager, and start listening.
    ///
    /// # Safety
    /// `display` must be a live libwayland-client `wl_display` that stays
    /// alive for as long as the clipboard does, and the clipboard must be
    /// used only from the thread that dispatches that display.
    pub unsafe fn new(display: *mut c_void) -> Option<Clipboard> {
        let lib = Lib::load()?;
        unsafe {
            let queue = (lib.create_queue)(display);
            if queue.is_null() {
                return None;
            }
            let wrapper = (lib.create_wrapper)(display);
            (lib.set_queue)(wrapper as *mut Proxy, queue);
            let registry = (lib.marshal)(wrapper as *mut Proxy, op::DISPLAY_GET_REGISTRY, &ffi::WL_REGISTRY, 1, 0, null_mut::<Proxy>());
            let shared = Box::into_raw(Box::new(Shared { lib, display, queue, seat: null_mut(), manager: null_mut(), version: 1, serial: 0, offers: HashMap::new(), selection: None, dnd: DndState::default(), source: None, owned_text: None }));
            (lib.add_listener)(registry, &REGISTRY_LISTENER as *const RegistryListener as *const c_void, shared as *mut c_void);
            let mut me = Clipboard { shared, wrapper, registry, keyboard: null_mut(), device: null_mut() };
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

    /// What a drag from outside did since last time.
    pub fn take_drag_events(&mut self) -> Vec<DragEvent> {
        // SAFETY: `shared` lives as long as `self`.
        unsafe { std::mem::take(&mut (*self.shared).dnd.events) }
    }

    /// The selection's text, if it holds any.
    pub fn read(&mut self) -> Option<String> {
        self.poll();
        // SAFETY: `shared` lives as long as `self`.
        let shared = unsafe { &mut *self.shared };
        if let Some(text) = &shared.owned_text {
            return Some(text.clone());
        }
        let offer = shared.selection?;
        let have = shared.offers.get(&(offer as usize))?;
        let mime = MIMES.iter().find(|m| have.iter().any(|h| h.as_bytes() == m.to_bytes()))?;
        let (mut ours, theirs) = UnixStream::pair().ok()?;
        // SAFETY: `receive` takes a mime and an fd; libwayland dups the fd.
        unsafe {
            (shared.lib.marshal)(offer, op::OFFER_RECEIVE, null::<Interface>(), shared.version, 0, mime.as_ptr(), theirs.as_raw_fd() as c_int);
            (shared.lib.flush)(shared.display);
        }
        drop(theirs);
        let _ = ours.set_read_timeout(Some(READ_TIMEOUT));
        let mut bytes = Vec::new();
        // A timeout leaves what arrived in `bytes`; that is the best we get.
        let _ = ours.read_to_end(&mut bytes);
        // Nothing at all means a dead offer (its source went away while
        // we were not focused to hear about it): keep what we have.
        if bytes.is_empty() {
            return None;
        }
        String::from_utf8(bytes).ok()
    }

    /// Make `text` the selection. `false` when the window has never had
    /// keyboard focus (the compositor would not take it).
    pub fn write(&mut self, text: &str) -> bool {
        self.poll();
        // SAFETY: `shared` lives as long as `self`; every proxy touched is
        // live, and the source's data outlives the source.
        unsafe {
            let shared = &mut *self.shared;
            if shared.serial == 0 {
                return false;
            }
            if let Some((old, data)) = shared.source.take() {
                (shared.lib.marshal)(old, op::SOURCE_DESTROY, null::<Interface>(), shared.version, ffi::MARSHAL_FLAG_DESTROY);
                drop(Box::from_raw(data));
            }
            let source = (shared.lib.marshal)(shared.manager, op::DDM_CREATE_DATA_SOURCE, &ffi::WL_DATA_SOURCE, shared.version, 0, null_mut::<Proxy>());
            if source.is_null() {
                return false;
            }
            let data = Box::into_raw(Box::new(SourceData { text: text.to_owned(), shared: self.shared }));
            (shared.lib.add_listener)(source, &SOURCE_LISTENER as *const SourceListener as *const c_void, data as *mut c_void);
            for mime in MIMES {
                (shared.lib.marshal)(source, op::SOURCE_OFFER, null::<Interface>(), shared.version, 0, mime.as_ptr());
            }
            (shared.lib.marshal)(self.device, op::DEVICE_SET_SELECTION, null::<Interface>(), shared.version, 0, source, shared.serial);
            (shared.lib.flush)(shared.display);
            shared.source = Some((source, data));
            shared.owned_text = Some(text.to_owned());
        }
        true
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: proxies go before the queue and the shared state they
        // point at; nothing fires after `destroy`.
        unsafe {
            let shared = Box::from_raw(self.shared);
            let lib = shared.lib;
            let version = shared.version;
            if let Some((source, data)) = shared.source {
                (lib.marshal)(source, op::SOURCE_DESTROY, null::<Interface>(), version, ffi::MARSHAL_FLAG_DESTROY);
                drop(Box::from_raw(data));
            }
            for offer in shared.offers.keys() {
                (lib.marshal)(*offer as *mut Proxy, op::OFFER_DESTROY, null::<Interface>(), version, ffi::MARSHAL_FLAG_DESTROY);
            }
            for proxy in [self.device, self.keyboard, shared.seat, shared.manager, self.registry] {
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
            let mut cb = Clipboard::new(display).expect("a seat and a data device manager");
            cb.poll();
            let shared = &*cb.shared;
            assert!(!shared.seat.is_null() && !shared.manager.is_null());
            assert!(!cb.write("x"), "no window, no focus, no serial: the selection cannot be ours");
            assert_eq!(cb.read(), None, "and nothing is offered to a client without focus");
            drop(cb);
            disconnect(display);
        }
    }
}
