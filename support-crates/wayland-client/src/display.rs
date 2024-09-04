use core::ffi::*;
use core::ops::{Deref, DerefMut};

use wayland_sys::{
    wl_display, wl_display_connect, wl_display_disconnect, wl_display_dispatch,
    wl_display_dispatch_pending, wl_display_flush, wl_display_get_fd, wl_display_roundtrip,
    wl_proxy, wl_proxy_get_version, wl_proxy_marshal_flags,
};

use crate::{Interface, OwnableInterface, OwnedWlRegistry, WlRegistry};

#[repr(transparent)]
pub struct WlDisplayConnection(core::ptr::NonNull<WlDisplay>);
impl Drop for WlDisplayConnection {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe { wl_display_disconnect(self.0.as_ptr() as _) }
    }
}
impl Deref for WlDisplayConnection {
    type Target = WlDisplay;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}
impl DerefMut for WlDisplayConnection {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut() }
    }
}
impl WlDisplayConnection {
    #[inline(always)]
    pub fn new(name: Option<&CStr>) -> Option<Self> {
        let ptr = unsafe { wl_display_connect(name.map_or_else(core::ptr::null, CStr::as_ptr)) };

        core::ptr::NonNull::new(ptr as *mut WlDisplay).map(Self)
    }
}

#[repr(transparent)]
pub struct WlDisplay(wl_display);
impl WlDisplay {
    #[inline(always)]
    pub fn as_raw_ptr_mut(&mut self) -> *mut wl_display {
        self as *mut _ as _
    }

    #[inline(always)]
    pub fn get_fd(&self) -> std::os::fd::RawFd {
        unsafe { wl_display_get_fd(self as *const _ as _) }
    }

    #[inline(always)]
    pub fn get_registry(&mut self) -> OwnedWlRegistry {
        unsafe {
            let ptr = core::ptr::NonNull::new(wl_proxy_marshal_flags(
                self as *mut _ as _,
                1,
                WlRegistry::interface(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                core::ptr::null_mut::<wl_proxy>(),
            ))
            .unwrap();

            WlRegistry::take_from_proxy_ptr(ptr)
        }
    }

    #[inline(always)]
    pub fn roundtrip(&mut self) -> Result<c_int, ()> {
        let res = unsafe { wl_display_roundtrip(self as *mut _ as _) };
        if res < 0 {
            Err(())
        } else {
            Ok(res)
        }
    }

    #[inline(always)]
    pub fn dispatch(&mut self) -> Result<c_int, ()> {
        let res = unsafe { wl_display_dispatch(self as *mut _ as _) };
        if res < 0 {
            Err(())
        } else {
            Ok(res)
        }
    }

    #[inline(always)]
    pub fn dispatch_pending(&mut self) -> Result<c_int, ()> {
        let res = unsafe { wl_display_dispatch_pending(self as *mut _ as _) };

        if res < 0 {
            Err(())
        } else {
            Ok(res)
        }
    }

    #[inline(always)]
    pub fn flush(&mut self) -> Result<(), ()> {
        let res = unsafe { wl_display_flush(self as *mut _ as _) };

        if res < 0 {
            Err(())
        } else {
            Ok(())
        }
    }
}
