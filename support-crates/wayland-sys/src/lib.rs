#![allow(non_camel_case_types)]

use core::ffi::*;

mod util;
pub use util::*;

#[repr(C)]
pub struct wl_display([u8; 0]);
#[repr(C)]
pub struct wl_proxy([u8; 0]);

pub const WL_MARSHAL_FLAG_DESTROY: u32 = 1 << 0;

#[link(name = "wayland-client")]
extern "C" {
    pub fn wl_display_connect(name: *const c_char) -> *mut wl_display;
    pub fn wl_display_disconnect(display: *mut wl_display);
    pub fn wl_display_roundtrip(display: *mut wl_display) -> c_int;
    pub fn wl_display_dispatch(display: *mut wl_display) -> c_int;
    pub fn wl_display_dispatch_pending(display: *mut wl_display) -> c_int;
    pub fn wl_display_flush(display: *mut wl_display) -> c_int;
    pub fn wl_display_get_fd(display: *const wl_display) -> std::os::fd::RawFd;

    pub fn wl_proxy_marshal_flags(
        proxy: *mut wl_proxy,
        opcode: u32,
        interface: *const wl_interface,
        version: u32,
        flags: u32,
        ...
    ) -> *mut wl_proxy;
    pub fn wl_proxy_destroy(proxy: *mut wl_proxy);
    pub fn wl_proxy_add_listener(
        proxy: *mut wl_proxy,
        implementation: *const c_void,
        data: *mut c_void,
    ) -> c_int;
    pub fn wl_proxy_get_version(proxy: *const wl_proxy) -> u32;
}
