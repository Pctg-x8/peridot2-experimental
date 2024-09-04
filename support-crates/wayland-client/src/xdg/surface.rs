use core::ffi::*;
use wayland_sys::{
    wl_interface, wl_message, wl_proxy, wl_proxy_add_listener, wl_proxy_get_version,
    wl_proxy_marshal_flags, WL_MARSHAL_FLAG_DESTROY,
};

use crate::{DefineStdOwnedInterface, Interface, OwnableInterface};

use super::{
    OwnedXDGToplevel, XDGToplevel, XDG_POPUP_INTERFACE, XDG_POSITIONER_INTERFACE,
    XDG_TOPLEVEL_INTERFACE,
};

#[repr(C)]
pub enum XDGSurfaceError {
    NotConstructed = 1,
    AlreadyConstructed = 2,
    UnconfiguredBuffer = 3,
    InvalidSerial = 4,
    InvalidSize = 5,
    DefunctRoleObject = 6,
}

pub static XDG_SURFACE_INTERFACE: wl_interface = wl_interface::new(
    c"xdg_surface",
    6,
    &[
        wl_message::new(c"destroy", c"", &[]),
        wl_message::new(c"get_toplevel", c"n", &[&XDG_TOPLEVEL_INTERFACE]),
        wl_message::new(
            c"get_popup",
            c"n?oo",
            &[
                &XDG_POPUP_INTERFACE,
                &XDG_SURFACE_INTERFACE,
                &XDG_POSITIONER_INTERFACE,
            ],
        ),
        wl_message::new(c"set_window_geometry", c"iiii", &[core::ptr::null(); 4]),
        wl_message::new(c"ack_configure", c"u", &[core::ptr::null()]),
    ],
    &[wl_message::new(c"configure", c"u", &[core::ptr::null()])],
);

pub trait XDGSurfaceListener {
    fn configure(&mut self, sender: &mut XDGSurface, serial: c_uint);
}
#[repr(C)]
struct ListenerFunctionPointers {
    configure: extern "C" fn(*mut c_void, *mut wl_proxy, c_uint),
}

DefineStdOwnedInterface!(pub type OwnedXDGSurface = XDGSurface);
impl Drop for OwnedXDGSurface {
    #[inline(always)]
    fn drop(&mut self) {
        self.destroy();
    }
}

#[repr(transparent)]
pub struct XDGSurface(wl_proxy);
impl Interface for XDGSurface {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        &XDG_SURFACE_INTERFACE
    }
}
impl XDGSurface {
    #[inline]
    pub fn add_listener<L: XDGSurfaceListener>(&mut self, listener: &mut L) -> Result<(), ()> {
        extern "C" fn configure<L: XDGSurfaceListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            serial: c_uint,
        ) {
            unsafe {
                (&mut *(data as *mut L)).configure(&mut *(sender as *mut XDGSurface), serial);
            }
        }
        let fps: &'static ListenerFunctionPointers = &ListenerFunctionPointers {
            configure: configure::<L>,
        };

        let res = unsafe {
            wl_proxy_add_listener(
                self as *mut _ as _,
                fps as *const _ as _,
                listener as *mut L as _,
            )
        };
        if res == 0 {
            Ok(())
        } else {
            Err(())
        }
    }

    #[inline(always)]
    fn destroy(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                0,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                WL_MARSHAL_FLAG_DESTROY,
            );
        }
    }

    #[inline(always)]
    pub fn get_toplevel(&mut self) -> OwnedXDGToplevel {
        unsafe {
            let ptr = core::ptr::NonNull::new(wl_proxy_marshal_flags(
                self as *mut _ as _,
                1,
                XDGToplevel::interface(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                core::ptr::null_mut::<wl_proxy>(),
            ))
            .unwrap();

            XDGToplevel::take_from_proxy_ptr(ptr)
        }
    }

    #[inline(always)]
    pub fn set_window_geometry(&mut self, x: c_int, y: c_int, width: c_int, height: c_int) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                3,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                x,
                y,
                width,
                height,
            );
        }
    }

    #[inline(always)]
    pub fn ack_configure(&mut self, serial: c_uint) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                4,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                serial,
            );
        }
    }
}
