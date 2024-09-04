use wayland_sys::{
    wl_interface, wl_proxy, wl_proxy_get_version, wl_proxy_marshal_flags, WL_MARSHAL_FLAG_DESTROY,
};

use crate::{DefineStdOwnedInterface, Interface, OwnableInterface, OwnedWlCallback, WlCallback};

#[repr(C)]
pub enum WlSurfaceError {
    /// buffer scale value is invalid
    InvalidScale = 0,
    /// buffer transform value is invalid
    InvalidTransform = 1,
    /// buffer size is invalid
    InvalidSize = 2,
    /// buffer offset is invalid
    InvalidOffset = 3,
    /// surface was destroyed before its role object
    DefunctRoleObject = 4,
}

extern "C" {
    pub static wl_surface_interface: wl_interface;
}

DefineStdOwnedInterface!(pub type OwnedWlSurface = WlSurface);
impl Drop for OwnedWlSurface {
    #[inline(always)]
    fn drop(&mut self) {
        self.destroy()
    }
}

#[repr(transparent)]
pub struct WlSurface(wl_proxy);
impl Interface for WlSurface {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        unsafe { &wl_surface_interface }
    }
}
impl WlSurface {
    #[inline(always)]
    pub fn as_raw_ptr_mut(&mut self) -> *mut wl_proxy {
        self as *mut _ as _
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
    pub fn frame(&mut self) -> OwnedWlCallback {
        unsafe {
            let ptr = core::ptr::NonNull::new(wl_proxy_marshal_flags(
                self as *mut _ as _,
                3,
                WlCallback::interface(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                core::ptr::null_mut::<wl_proxy>(),
            ))
            .unwrap();

            WlCallback::take_from_proxy_ptr(ptr)
        }
    }

    #[inline(always)]
    pub fn commit(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                6,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
            );
        }
    }
}
