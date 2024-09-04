use wayland_sys::{
    wl_interface, wl_proxy, wl_proxy_destroy, wl_proxy_get_version, wl_proxy_marshal_flags,
};

use crate::{DefineStdOwnedInterface, Interface, OwnableInterface, OwnedWlSurface, WlSurface};

extern "C" {
    pub static wl_compositor_interface: wl_interface;
}

DefineStdOwnedInterface!(pub type OwnedWlCompositor = WlCompositor);
impl Drop for OwnedWlCompositor {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe { wl_proxy_destroy(self.0.as_ptr() as _) }
    }
}

#[repr(transparent)]
pub struct WlCompositor(wl_proxy);
impl Interface for WlCompositor {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        unsafe { &wl_compositor_interface }
    }
}
impl WlCompositor {
    #[inline(always)]
    pub fn create_surface(&mut self) -> OwnedWlSurface {
        unsafe {
            let ptr = core::ptr::NonNull::new(wl_proxy_marshal_flags(
                self as *mut _ as _,
                0,
                WlSurface::interface(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                core::ptr::null_mut::<wl_proxy>(),
            ))
            .unwrap();

            WlSurface::take_from_proxy_ptr(ptr)
        }
    }
}
