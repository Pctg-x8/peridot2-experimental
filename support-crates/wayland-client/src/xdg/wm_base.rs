use wayland_sys::{
    wl_interface, wl_message, wl_proxy, wl_proxy_add_listener, wl_proxy_get_version,
    wl_proxy_marshal_flags, WL_MARSHAL_FLAG_DESTROY,
};

use crate::{
    wl_surface_interface, DefineStdOwnedInterface, Interface, OwnableInterface, WlSurface,
};
use core::ffi::*;

use super::{OwnedXDGSurface, XDGSurface, XDG_POSITIONER_INTERFACE, XDG_SURFACE_INTERFACE};

#[repr(C)]
#[derive(Clone, Copy)]
pub enum XDGWMBaseError {
    /// given wl_surface has another role
    Role = 0,
    /// xdg_wm_base was destroyed before children
    DefunctSurfaces = 1,
    /// the client tried to map or destroy a non-topmost popup
    NotTheTopmostPopup = 2,
    /// the client specified an invalid popup parent surface
    InvalidPopupParent = 3,
    /// the client provided an invalid surface state
    InvalidSurfaceState = 4,
    /// the client provided an invalid positioner
    InvalidPositioner = 5,
    /// the client didn't respond to a ping event in time
    Unresponsive = 6,
}

pub static XDG_WM_BASE_INTERFACE: wl_interface = wl_interface::new(
    c"xdg_wm_base",
    6,
    &[
        wl_message::new(c"destroy", c"", &[]),
        wl_message::new(c"create_positioner", c"n", &[&XDG_POSITIONER_INTERFACE]),
        wl_message::new(
            c"create_xdg_surface",
            c"no",
            &[&XDG_SURFACE_INTERFACE, unsafe { &wl_surface_interface }],
        ),
        wl_message::new(c"pong", c"u", &[core::ptr::null()]),
    ],
    &[wl_message::new(c"ping", c"u", &[core::ptr::null()])],
);

pub trait XDGWMBaseListener {
    fn ping(&mut self, sender: &mut XDGWMBase, serial: c_uint);
}
#[repr(C)]
struct ListenerFunctionPointers {
    ping: extern "C" fn(*mut c_void, *mut wl_proxy, c_uint),
}

DefineStdOwnedInterface!(pub type OwnedXDGWMBase = XDGWMBase);
impl Drop for OwnedXDGWMBase {
    #[inline(always)]
    fn drop(&mut self) {
        self.destroy();
    }
}

#[repr(transparent)]
pub struct XDGWMBase(wl_proxy);
impl Interface for XDGWMBase {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        &XDG_WM_BASE_INTERFACE
    }
}
impl XDGWMBase {
    #[inline]
    pub fn add_listener<L: XDGWMBaseListener>(&mut self, listener: &mut L) -> Result<(), ()> {
        extern "C" fn ping<L: XDGWMBaseListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            serial: c_uint,
        ) {
            unsafe {
                (&mut *(data as *mut L)).ping(&mut *(sender as *mut XDGWMBase), serial);
            }
        }
        let fps: &'static ListenerFunctionPointers = &ListenerFunctionPointers { ping: ping::<L> };

        let res = unsafe {
            wl_proxy_add_listener(
                self as *mut _ as _,
                fps as *const _ as _,
                listener as *mut _ as _,
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
    pub fn create_xdg_surface(&mut self, surface: &mut WlSurface) -> OwnedXDGSurface {
        unsafe {
            let ptr = core::ptr::NonNull::new(wl_proxy_marshal_flags(
                self as *mut _ as _,
                2,
                XDGSurface::interface(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                core::ptr::null_mut::<wl_proxy>(),
                surface as *mut _ as *mut wl_proxy,
            ))
            .unwrap();

            XDGSurface::take_from_proxy_ptr(ptr)
        }
    }

    #[inline(always)]
    pub fn pong(&mut self, serial: c_uint) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                3,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                serial,
            );
        }
    }
}
