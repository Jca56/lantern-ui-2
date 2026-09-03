//! The little of libwayland-client the clipboard speaks: the library
//! winit already opened, found again with `dlopen` and called through
//! function pointers, plus the interface tables (name, signature, types)
//! libwayland needs to marshal the objects we use. Everything is bound
//! at version 1. The tables are checked against their signatures when
//! they are built, at compile time.
//!
//! Safety: this is the raw C API. `data_device` keeps the invariants:
//! every proxy lives while its listener can fire, and the user data a
//! listener gets outlives the proxy it belongs to.

use std::ffi::{CStr, c_char, c_int, c_void};

#[repr(C)]
pub struct Message {
    pub name: *const c_char,
    pub signature: *const c_char,
    pub types: *const *const Interface,
}
unsafe impl Sync for Message {}

#[repr(C)]
pub struct Interface {
    pub name: *const c_char,
    pub version: c_int,
    pub method_count: c_int,
    pub methods: *const Message,
    pub event_count: c_int,
    pub events: *const Message,
}
unsafe impl Sync for Interface {}

/// `wl_array`: an event argument we only ever ignore.
#[repr(C)]
pub struct Array {
    pub size: usize,
    pub alloc: usize,
    pub data: *mut c_void,
}

pub enum Proxy {}
pub enum Queue {}

/// The flag a destructor request is marshalled with.
pub const MARSHAL_FLAG_DESTROY: u32 = 1;

pub type MarshalFn = unsafe extern "C" fn(*mut Proxy, u32, *const Interface, u32, u32, ...) -> *mut Proxy;

/// The functions we call, resolved once.
#[derive(Clone, Copy)]
pub struct Lib {
    pub create_queue: unsafe extern "C" fn(*mut c_void) -> *mut Queue,
    pub queue_destroy: unsafe extern "C" fn(*mut Queue),
    pub dispatch_queue_pending: unsafe extern "C" fn(*mut c_void, *mut Queue) -> c_int,
    pub roundtrip_queue: unsafe extern "C" fn(*mut c_void, *mut Queue) -> c_int,
    pub flush: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub create_wrapper: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub wrapper_destroy: unsafe extern "C" fn(*mut c_void),
    pub set_queue: unsafe extern "C" fn(*mut Proxy, *mut Queue),
    pub marshal: MarshalFn,
    pub add_listener: unsafe extern "C" fn(*mut Proxy, *const c_void, *mut c_void) -> c_int,
    pub destroy: unsafe extern "C" fn(*mut Proxy),
}

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_NOW: c_int = 2;
const RTLD_NOLOAD: c_int = 4;

/// A symbol of libwayland-client, the copy the process already has open
/// (winit's) when there is one. Null when the library or the symbol is
/// not there.
pub(crate) fn symbol(name: &CStr) -> *mut c_void {
    let lib = c"libwayland-client.so.0";
    // SAFETY: dlopen/dlsym with valid C strings.
    unsafe {
        let mut handle = dlopen(lib.as_ptr(), RTLD_NOW | RTLD_NOLOAD);
        if handle.is_null() {
            handle = dlopen(lib.as_ptr(), RTLD_NOW);
        }
        if handle.is_null() {
            return std::ptr::null_mut();
        }
        dlsym(handle, name.as_ptr())
    }
}

impl Lib {
    /// The functions we need, or `None` when libwayland-client is not
    /// there to be had.
    // Each transmute's target is the field it fills: the signature is
    // written once, on the struct.
    #[allow(clippy::missing_transmute_annotations)]
    pub fn load() -> Option<Lib> {
        // SAFETY: the symbols are cast to the signatures libwayland-client
        // 1.20+ declares them with.
        unsafe {
            macro_rules! sym {
                ($s:literal) => {{
                    let p = symbol($s);
                    if p.is_null() {
                        return None;
                    }
                    std::mem::transmute(p)
                }};
            }
            Some(Lib {
                create_queue: sym!(c"wl_display_create_queue"),
                queue_destroy: sym!(c"wl_event_queue_destroy"),
                dispatch_queue_pending: sym!(c"wl_display_dispatch_queue_pending"),
                roundtrip_queue: sym!(c"wl_display_roundtrip_queue"),
                flush: sym!(c"wl_display_flush"),
                create_wrapper: sym!(c"wl_proxy_create_wrapper"),
                wrapper_destroy: sym!(c"wl_proxy_wrapper_destroy"),
                set_queue: sym!(c"wl_proxy_set_queue"),
                marshal: sym!(c"wl_proxy_marshal_flags"),
                add_listener: sym!(c"wl_proxy_add_listener"),
                destroy: sym!(c"wl_proxy_destroy"),
            })
        }
    }
}

// ---- interface tables --------------------------------------------------------

/// The per-argument interface pointers of one message (null for anything
/// that is not an object).
pub struct Types<const N: usize>(pub [*const Interface; N]);
unsafe impl<const N: usize> Sync for Types<N> {}

/// How many arguments a signature has (`?` marks nullable, digits a
/// "since" version; neither is an argument).
const fn arg_count(sig: &CStr) -> usize {
    let bytes = sig.to_bytes();
    let mut i = 0;
    let mut n = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'?' && !b.is_ascii_digit() {
            n += 1;
        }
        i += 1;
    }
    n
}

/// A message whose `types` must list one entry per argument; anything
/// else fails to compile.
const fn message<const N: usize>(name: &'static CStr, signature: &'static CStr, types: &'static Types<N>) -> Message {
    assert!(arg_count(signature) == N, "a message's types must match its signature");
    Message { name: name.as_ptr(), signature: signature.as_ptr(), types: types.0.as_ptr() }
}

const fn interface(name: &'static CStr, methods: &'static [Message], events: &'static [Message]) -> Interface {
    Interface { name: name.as_ptr(), version: 1, method_count: methods.len() as c_int, methods: methods.as_ptr(), event_count: events.len() as c_int, events: events.as_ptr() }
}

const NULL: *const Interface = std::ptr::null();
static T0: Types<0> = Types([]);
static T1: Types<1> = Types([NULL]);
static T2: Types<2> = Types([NULL; 2]);
static T3: Types<3> = Types([NULL; 3]);
static T4: Types<4> = Types([NULL; 4]);
static T5: Types<5> = Types([NULL; 5]);

static REGISTRY_METHODS: [Message; 1] = [message(c"bind", c"usun", &T4)];
static REGISTRY_EVENTS: [Message; 2] = [message(c"global", c"usu", &T3), message(c"global_remove", c"u", &T1)];
pub static WL_REGISTRY: Interface = interface(c"wl_registry", &REGISTRY_METHODS, &REGISTRY_EVENTS);

static SEAT_KEYBOARD_T: Types<1> = Types([&WL_KEYBOARD as *const Interface]);
static SEAT_METHODS: [Message; 3] = [message(c"get_pointer", c"n", &T1), message(c"get_keyboard", c"n", &SEAT_KEYBOARD_T), message(c"get_touch", c"n", &T1)];
static SEAT_EVENTS: [Message; 2] = [message(c"capabilities", c"u", &T1), message(c"name", c"s", &T1)];
pub static WL_SEAT: Interface = interface(c"wl_seat", &SEAT_METHODS, &SEAT_EVENTS);

static KEYBOARD_METHODS: [Message; 1] = [message(c"release", c"", &T0)];
static KEYBOARD_EVENTS: [Message; 6] = [
    message(c"keymap", c"uhu", &T3),
    message(c"enter", c"uoa", &T3),
    message(c"leave", c"uo", &T2),
    message(c"key", c"uuuu", &T4),
    message(c"modifiers", c"uuuuu", &T5),
    message(c"repeat_info", c"ii", &T2),
];
pub static WL_KEYBOARD: Interface = interface(c"wl_keyboard", &KEYBOARD_METHODS, &KEYBOARD_EVENTS);

static DDM_SOURCE_T: Types<1> = Types([&WL_DATA_SOURCE as *const Interface]);
static DDM_DEVICE_T: Types<2> = Types([&WL_DATA_DEVICE as *const Interface, &WL_SEAT as *const Interface]);
static DDM_METHODS: [Message; 2] = [message(c"create_data_source", c"n", &DDM_SOURCE_T), message(c"get_data_device", c"no", &DDM_DEVICE_T)];
pub static WL_DATA_DEVICE_MANAGER: Interface = interface(c"wl_data_device_manager", &DDM_METHODS, &[]);

static DEVICE_DRAG_T: Types<4> = Types([&WL_DATA_SOURCE as *const Interface, NULL, NULL, NULL]);
static DEVICE_SELECTION_T: Types<2> = Types([&WL_DATA_SOURCE as *const Interface, NULL]);
static DEVICE_OFFER_T: Types<1> = Types([&WL_DATA_OFFER as *const Interface]);
static DEVICE_ENTER_T: Types<5> = Types([NULL, NULL, NULL, NULL, &WL_DATA_OFFER as *const Interface]);
static DEVICE_METHODS: [Message; 3] = [message(c"start_drag", c"?oo?ou", &DEVICE_DRAG_T), message(c"set_selection", c"?ou", &DEVICE_SELECTION_T), message(c"release", c"", &T0)];
static DEVICE_EVENTS: [Message; 6] = [
    message(c"data_offer", c"n", &DEVICE_OFFER_T),
    message(c"enter", c"uoff?o", &DEVICE_ENTER_T),
    message(c"leave", c"", &T0),
    message(c"motion", c"uff", &T3),
    message(c"drop", c"", &T0),
    message(c"selection", c"?o", &DEVICE_OFFER_T),
];
pub static WL_DATA_DEVICE: Interface = interface(c"wl_data_device", &DEVICE_METHODS, &DEVICE_EVENTS);

static SOURCE_METHODS: [Message; 3] = [message(c"offer", c"s", &T1), message(c"destroy", c"", &T0), message(c"set_actions", c"u", &T1)];
static SOURCE_EVENTS: [Message; 6] = [
    message(c"target", c"?s", &T1),
    message(c"send", c"sh", &T2),
    message(c"cancelled", c"", &T0),
    message(c"dnd_drop_performed", c"", &T0),
    message(c"dnd_finished", c"", &T0),
    message(c"action", c"u", &T1),
];
pub static WL_DATA_SOURCE: Interface = interface(c"wl_data_source", &SOURCE_METHODS, &SOURCE_EVENTS);

static OFFER_METHODS: [Message; 5] = [message(c"accept", c"u?s", &T2), message(c"receive", c"sh", &T2), message(c"destroy", c"", &T0), message(c"finish", c"", &T0), message(c"set_actions", c"uu", &T2)];
static OFFER_EVENTS: [Message; 3] = [message(c"offer", c"s", &T1), message(c"source_actions", c"u", &T1), message(c"action", c"u", &T1)];
pub static WL_DATA_OFFER: Interface = interface(c"wl_data_offer", &OFFER_METHODS, &OFFER_EVENTS);

/// Request opcodes we send.
pub mod op {
    pub const DISPLAY_GET_REGISTRY: u32 = 1;
    pub const REGISTRY_BIND: u32 = 0;
    pub const SEAT_GET_KEYBOARD: u32 = 1;
    pub const DDM_CREATE_DATA_SOURCE: u32 = 0;
    pub const DDM_GET_DATA_DEVICE: u32 = 1;
    pub const DEVICE_SET_SELECTION: u32 = 1;
    pub const SOURCE_OFFER: u32 = 0;
    pub const SOURCE_DESTROY: u32 = 1;
    pub const OFFER_RECEIVE: u32 = 1;
    pub const OFFER_DESTROY: u32 = 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_count_arguments() {
        assert_eq!(arg_count(c""), 0);
        assert_eq!(arg_count(c"usun"), 4);
        assert_eq!(arg_count(c"?oo?ou"), 4);
        assert_eq!(arg_count(c"3u"), 1);
        assert_eq!(WL_DATA_DEVICE.event_count, 6);
        assert_eq!(WL_DATA_DEVICE.version, 1);
        // The tables link up: a data offer event creates a wl_data_offer.
        let offer_types = unsafe { *DEVICE_EVENTS[0].types };
        assert!(std::ptr::eq(offer_types, &WL_DATA_OFFER));
        let name = unsafe { CStr::from_ptr(WL_DATA_OFFER.name) };
        assert_eq!(name, c"wl_data_offer");
    }
}
