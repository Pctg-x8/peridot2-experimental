//! wayland-util.h

use core::ffi::*;

#[repr(C)]
pub struct wl_interface {
    pub name: *const c_char,
    pub version: c_int,
    pub method_count: c_int,
    pub methods: *const wl_message,
    pub event_count: c_int,
    pub events: *const wl_message,
}
unsafe impl Sync for wl_interface {}
impl wl_interface {
    #[inline]
    pub const fn new(
        name: &'static CStr,
        version: c_int,
        methods: &'static [wl_message],
        events: &'static [wl_message],
    ) -> Self {
        Self {
            name: name.as_ptr(),
            version,
            method_count: methods.len() as _,
            methods: methods.as_ptr(),
            event_count: events.len() as _,
            events: events.as_ptr(),
        }
    }
}

#[repr(C)]
pub struct wl_message {
    pub name: *const c_char,
    pub signature: *const c_char,
    pub types: *const *const wl_interface,
}
unsafe impl Sync for wl_message {}
impl wl_message {
    #[inline]
    pub const fn new(
        name: &'static CStr,
        signature: &'static CStr,
        types: &'static [*const wl_interface],
    ) -> Self {
        Self {
            name: name.as_ptr(),
            signature: signature.as_ptr(),
            types: types.as_ptr(),
        }
    }
}

#[repr(C)]
pub struct wl_array {
    pub size: usize,
    pub alloc: usize,
    pub data: *mut c_void,
}
impl wl_array {
    #[inline(always)]
    pub const unsafe fn as_slice_of<T>(&self) -> &[T] {
        core::slice::from_raw_parts(self.data as *const T, self.size / core::mem::size_of::<T>())
    }

    #[inline(always)]
    pub unsafe fn as_mut_slice_of<T>(&mut self) -> &mut [T] {
        core::slice::from_raw_parts_mut(self.data as *mut T, self.size / core::mem::size_of::<T>())
    }

    #[inline(always)]
    pub unsafe fn iter_of<'a, T: 'a>(&'a self) -> impl Iterator<Item = &'a T> {
        self.as_slice_of::<T>().iter()
    }

    #[inline(always)]
    pub unsafe fn iter_mut_of<'a, T: 'a>(&'a mut self) -> impl Iterator<Item = &'a mut T> {
        self.as_mut_slice_of::<T>().iter_mut()
    }
}
