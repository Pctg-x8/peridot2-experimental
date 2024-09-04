use core::ffi::*;

use wayland_sys::{wl_interface, wl_proxy, wl_proxy_add_listener, wl_proxy_destroy};

use crate::{DefineStdOwnedInterface, Interface};

extern "C" {
    pub static wl_callback_interface: wl_interface;
}

pub trait WlCallbackListener {
    fn done(&mut self, sender: &mut WlCallback, callback_data: u32);
}
#[repr(C)]
struct ListenerFunctionPointers {
    done: extern "C" fn(*mut c_void, *mut wl_proxy, u32),
}

DefineStdOwnedInterface!(pub type OwnedWlCallback = WlCallback);
impl Drop for OwnedWlCallback {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe { wl_proxy_destroy(self.0.as_ptr() as _) }
    }
}

#[repr(transparent)]
pub struct WlCallback(wl_proxy);
impl Interface for WlCallback {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        unsafe { &wl_callback_interface }
    }
}
impl WlCallback {
    #[inline]
    pub fn add_listener<L: WlCallbackListener>(&mut self, listener: &mut L) -> Result<(), ()> {
        extern "C" fn done<L: WlCallbackListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            callback_data: u32,
        ) {
            unsafe {
                (&mut *(data as *mut L)).done(&mut *(sender as *mut WlCallback), callback_data)
            }
        }
        let fps: &'static ListenerFunctionPointers = &ListenerFunctionPointers { done: done::<L> };

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
}
