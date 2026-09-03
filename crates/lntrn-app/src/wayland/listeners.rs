//! What libwayland calls back into: the registry, seat, keyboard, pointer,
//! data device, data offers and our data sources report here, into the
//! [`Shared`] state every proxy was given as user data.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::fs::File;
use std::io::Write;
use std::os::fd::FromRawFd;
use std::ptr::{null, null_mut};

use super::data_device::{Shared, SourceData};
use super::dnd::DragEvent;
use super::ffi::{self, Array, Interface, Lib, Proxy, op};


#[repr(C)]
pub(super) struct RegistryListener {
    global: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *const c_char, u32),
    global_remove: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
}
pub(super) static REGISTRY_LISTENER: RegistryListener = RegistryListener { global: registry_global, global_remove: registry_global_remove };

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
pub(super) struct SeatListener {
    capabilities: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
    name: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char),
}
pub(super) static SEAT_LISTENER: SeatListener = SeatListener { capabilities: seat_capabilities, name: seat_name };
unsafe extern "C" fn seat_capabilities(_: *mut c_void, _: *mut Proxy, _: u32) {}
unsafe extern "C" fn seat_name(_: *mut c_void, _: *mut Proxy, _: *const c_char) {}

#[repr(C)]
pub(super) struct KeyboardListener {
    keymap: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, c_int, u32),
    enter: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy, *mut Array),
    leave: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy),
    key: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, u32, u32, u32),
    modifiers: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, u32, u32, u32, u32),
    repeat_info: unsafe extern "C" fn(*mut c_void, *mut Proxy, i32, i32),
}
pub(super) static KEYBOARD_LISTENER: KeyboardListener = KeyboardListener { keymap: kb_keymap, enter: kb_enter, leave: kb_leave, key: kb_key, modifiers: kb_modifiers, repeat_info: kb_repeat_info };

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

/// A pointer of our own, for the one thing the clipboard's keyboard
/// cannot give: the serial of the button press a drag starts from.
#[repr(C)]
pub(super) struct PointerListener {
    enter: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy, i32, i32),
    leave: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy),
    motion: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, i32, i32),
    button: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, u32, u32, u32),
    axis: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, u32, i32),
}
pub(super) static POINTER_LISTENER: PointerListener = PointerListener { enter: ptr_enter, leave: ptr_leave, motion: ptr_motion, button: ptr_button, axis: ptr_axis };
unsafe extern "C" fn ptr_enter(_: *mut c_void, _: *mut Proxy, _: u32, _: *mut Proxy, _: i32, _: i32) {}
unsafe extern "C" fn ptr_leave(_: *mut c_void, _: *mut Proxy, _: u32, _: *mut Proxy) {}
unsafe extern "C" fn ptr_motion(_: *mut c_void, _: *mut Proxy, _: u32, _: i32, _: i32) {}
unsafe extern "C" fn ptr_axis(_: *mut c_void, _: *mut Proxy, _: u32, _: u32, _: i32) {}
unsafe extern "C" fn ptr_button(data: *mut c_void, _: *mut Proxy, serial: u32, _time: u32, _button: u32, state: u32) {
    unsafe {
        let shared = &mut *(data as *mut Shared);
        shared.pointer_down = state == 1;
        if shared.pointer_down {
            shared.pointer_serial = serial;
        }
    }
}

#[repr(C)]
pub(super) struct DeviceListener {
    data_offer: unsafe extern "C" fn(*mut c_void, *mut Proxy, *mut Proxy),
    enter: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *mut Proxy, i32, i32, *mut Proxy),
    leave: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    motion: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, i32, i32),
    drop: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    selection: unsafe extern "C" fn(*mut c_void, *mut Proxy, *mut Proxy),
}
pub(super) static DEVICE_LISTENER: DeviceListener = DeviceListener { data_offer: dev_data_offer, enter: dev_enter, leave: dev_leave, motion: dev_motion, drop: dev_drop, selection: dev_selection };

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

pub(super) unsafe fn destroy_offer(shared: &mut Shared, offer: *mut Proxy) {
    shared.offers.remove(&(offer as usize));
    // SAFETY: `destroy` is wl_data_offer's destructor; the proxy goes with it.
    unsafe { (shared.lib.marshal)(offer, op::OFFER_DESTROY, null::<Interface>(), shared.version, ffi::MARSHAL_FLAG_DESTROY) };
}

#[repr(C)]
pub(super) struct OfferListener {
    offer: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char),
    source_actions: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
    action: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
}
pub(super) static OFFER_LISTENER: OfferListener = OfferListener { offer: offer_offer, source_actions: offer_u32, action: offer_u32 };
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
pub(super) struct SourceListener {
    target: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char),
    send: unsafe extern "C" fn(*mut c_void, *mut Proxy, *const c_char, c_int),
    cancelled: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    dnd_drop_performed: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    dnd_finished: unsafe extern "C" fn(*mut c_void, *mut Proxy),
    action: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
}
pub(super) static SOURCE_LISTENER: SourceListener = SourceListener { target: source_target, send: source_send, cancelled: source_cancelled, dnd_drop_performed: source_drop_performed, dnd_finished: source_finished, action: source_action };
unsafe extern "C" fn source_target(_: *mut c_void, _: *mut Proxy, _: *const c_char) {}
unsafe extern "C" fn source_action(_: *mut c_void, _: *mut Proxy, _: u32) {}
unsafe extern "C" fn source_send(data: *mut c_void, _: *mut Proxy, mime: *const c_char, fd: c_int) {
    // Someone pastes or takes a drop: write what they asked for and close
    // the pipe so they see the end.
    // SAFETY: `data` is this source's `SourceData`; the fd is ours to close.
    unsafe {
        let src = &*(data as *mut SourceData);
        let mime = CStr::from_ptr(mime);
        let bytes = src.extra.iter().find(|(m, _)| *m == mime).map_or(&src.bytes, |(_, b)| b);
        let mut file = File::from_raw_fd(fd);
        let _ = file.write_all(bytes);
    }
}
unsafe extern "C" fn source_cancelled(data: *mut c_void, source: *mut Proxy) {
    // Another app took the selection, or the drag ended nowhere: this
    // source is done.
    unsafe {
        let src = Box::from_raw(data as *mut SourceData);
        let shared = &mut *src.shared;
        if shared.source.is_some_and(|(p, _)| p == source) {
            shared.source = None;
            shared.owned = None;
        }
        if shared.drag_source.is_some_and(|(p, _)| p == source) {
            shared.drag_source = None;
            shared.dnd.events.push(DragEvent::Ended { dropped: false });
        }
        (shared.lib.marshal)(source, op::SOURCE_DESTROY, null::<Interface>(), shared.version, ffi::MARSHAL_FLAG_DESTROY);
        drop(src);
    }
}
unsafe extern "C" fn source_drop_performed(data: *mut c_void, _: *mut Proxy) {
    // The drop happened; the data may still be asked for. Before version
    // 3 nothing more is said, so this is as much of an end as there is.
    unsafe {
        let shared = &mut *(*(data as *mut SourceData)).shared;
        if shared.version < 3 {
            shared.dnd.events.push(DragEvent::Ended { dropped: true });
        }
    }
}
unsafe extern "C" fn source_finished(data: *mut c_void, source: *mut Proxy) {
    // The target has what it took: the drag is over.
    unsafe {
        let src = Box::from_raw(data as *mut SourceData);
        let shared = &mut *src.shared;
        if shared.drag_source.is_some_and(|(p, _)| p == source) {
            shared.drag_source = None;
        }
        shared.dnd.events.push(DragEvent::Ended { dropped: true });
        (shared.lib.marshal)(source, op::SOURCE_DESTROY, null::<Interface>(), shared.version, ffi::MARSHAL_FLAG_DESTROY);
        drop(src);
    }
}
