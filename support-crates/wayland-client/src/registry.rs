use core::ffi::*;
use std::ops::{Deref, DerefMut};
use wayland_sys::{
    wl_interface, wl_proxy, wl_proxy_add_listener, wl_proxy_destroy, wl_proxy_marshal_flags,
};

use crate::{Interface, OwnableInterface};

pub trait WlRegistryListener {
    fn global(&mut self, sender: &mut WlRegistry, name: u32, interface: &CStr, version: u32);
    fn global_remove(&mut self, sender: &mut WlRegistry, name: u32);
}
#[repr(C)]
struct ListenerFunctionPointers {
    global: extern "C" fn(*mut c_void, *mut wl_proxy, u32, *const c_char, u32),
    global_remove: extern "C" fn(*mut c_void, *mut wl_proxy, u32),
}

extern "C" {
    pub static wl_registry_interface: wl_interface;
}

#[repr(transparent)]
pub struct OwnedWlRegistry(core::ptr::NonNull<WlRegistry>);
impl Drop for OwnedWlRegistry {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            wl_proxy_destroy(self.0.as_ptr() as _);
        }
    }
}
impl Deref for OwnedWlRegistry {
    type Target = WlRegistry;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}
impl DerefMut for OwnedWlRegistry {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut() }
    }
}

#[repr(transparent)]
pub struct WlRegistry(wl_proxy);
impl Interface for WlRegistry {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        unsafe { &wl_registry_interface }
    }
}
impl OwnableInterface for WlRegistry {
    type OwnedType = OwnedWlRegistry;

    #[inline]
    unsafe fn take_from_proxy_ptr(ptr: core::ptr::NonNull<wl_proxy>) -> Self::OwnedType {
        OwnedWlRegistry(ptr.cast())
    }
}
impl WlRegistry {
    #[inline]
    pub fn add_listener<L: WlRegistryListener>(&mut self, listener: &mut L) -> Result<(), ()> {
        extern "C" fn global<L: WlRegistryListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            name: u32,
            interface: *const c_char,
            version: u32,
        ) {
            unsafe {
                (&mut *(data as *mut L)).global(
                    &mut *(sender as *mut WlRegistry),
                    name,
                    CStr::from_ptr(interface),
                    version,
                );
            }
        }
        extern "C" fn global_remove<L: WlRegistryListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            name: u32,
        ) {
            unsafe {
                (&mut *(data as *mut L)).global_remove(&mut *(sender as *mut WlRegistry), name);
            }
        }
        let fps: &'static ListenerFunctionPointers = &ListenerFunctionPointers {
            global: global::<L>,
            global_remove: global_remove::<L>,
        };

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

    #[inline]
    pub unsafe fn bind_untyped(
        &mut self,
        name: u32,
        interface: &wl_interface,
        version: u32,
    ) -> *mut wl_proxy {
        wl_proxy_marshal_flags(
            self as *mut _ as _,
            0,
            interface,
            version,
            0,
            name,
            interface.name,
            version,
            core::ptr::null_mut::<wl_proxy>(),
        )
    }

    #[inline(always)]
    pub fn bind<T: OwnableInterface>(&mut self, name: u32, version: u32) -> T::OwnedType {
        unsafe {
            let ptr =
                core::ptr::NonNull::new(self.bind_untyped(name, T::interface(), version)).unwrap();

            T::take_from_proxy_ptr(ptr)
        }
    }
}
