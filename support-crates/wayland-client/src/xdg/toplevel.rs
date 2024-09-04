use crate::{
    wl_output_interface, wl_seat_interface, DefineStdOwnedInterface, Interface, WlOutput, WlSeat,
};
use core::ffi::*;
use wayland_sys::{
    wl_array, wl_interface, wl_message, wl_proxy, wl_proxy_add_listener, wl_proxy_get_version,
    wl_proxy_marshal_flags, WL_MARSHAL_FLAG_DESTROY,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub enum XDGToplevelError {
    /// provided value is not a valid variant of the resize_edge enum
    InvalidResizeEdge = 0,
    /// invalid parent toplevel
    InvalidParent = 1,
    /// client provided an invalid min or max size
    InvalidSize = 2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub enum XDGToplevelResizeEdge {
    None = 0,
    Top = 1,
    Bottom = 2,
    Left = 4,
    TopLeft = 5,
    BottomLeft = 6,
    Right = 8,
    TopRight = 9,
    BottomRight = 10,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub enum XDGToplevelState {
    Maximized = 1,
    Fullscreen = 2,
    Resizing = 3,
    Activated = 4,
    /// since v2
    TiledLeft = 5,
    /// since v2
    TiledRight = 6,
    /// since v2
    TiledTop = 7,
    /// since v2
    TiledBottom = 8,
    /// since v6
    Suspeneded = 9,
}

/// since v5
#[repr(C)]
#[derive(Clone, Copy)]
pub enum XDGToplevelWMCapabilities {
    /// show_window_menu is available
    WindowMenu = 1,
    /// set_maximized and unset_maximized are available
    Maximize = 2,
    /// set_fullscreen and unset_fullscreen are available
    Fullscreen = 3,
    /// set_minimized is available
    Minimize = 4,
}

pub static XDG_TOPLEVEL_INTERFACE: wl_interface = wl_interface::new(
    c"xdg_toplevel",
    6,
    &[
        wl_message::new(c"destroy", c"", &[]),
        wl_message::new(c"set_parent", c"?o", &[&XDG_TOPLEVEL_INTERFACE]),
        wl_message::new(c"set_title", c"s", &[core::ptr::null()]),
        wl_message::new(c"set_app_id", c"s", &[core::ptr::null()]),
        wl_message::new(
            c"show_window_menu",
            c"ouii",
            &[
                unsafe { &wl_seat_interface },
                core::ptr::null(),
                core::ptr::null(),
                core::ptr::null(),
            ],
        ),
        wl_message::new(
            c"move",
            c"ou",
            &[unsafe { &wl_seat_interface }, core::ptr::null()],
        ),
        wl_message::new(
            c"resize",
            c"ouu",
            &[
                unsafe { &wl_seat_interface },
                core::ptr::null(),
                core::ptr::null(),
            ],
        ),
        wl_message::new(c"set_max_size", c"ii", &[core::ptr::null(); 2]),
        wl_message::new(c"set_min_size", c"ii", &[core::ptr::null(); 2]),
        wl_message::new(c"set_maximized", c"", &[]),
        wl_message::new(c"unset_maximized", c"", &[]),
        wl_message::new(c"set_fullscreen", c"?o", &[unsafe { &wl_output_interface }]),
        wl_message::new(c"unset_fullscreen", c"", &[]),
        wl_message::new(c"set_minimized", c"", &[]),
    ],
    &[
        wl_message::new(c"configure", c"iia", &[core::ptr::null(); 3]),
        wl_message::new(c"close", c"", &[]),
        wl_message::new(c"configure_bounds", c"4ii", &[core::ptr::null(); 2]),
        wl_message::new(c"wm_capabilities", c"5a", &[core::ptr::null()]),
    ],
);

pub trait XDGToplevelListener {
    fn configure(
        &mut self,
        sender: &mut XDGToplevel,
        width: c_int,
        height: c_int,
        states: &mut wl_array,
    );
    fn close(&mut self, sender: &mut XDGToplevel);
    fn configure_bounds(&mut self, sender: &mut XDGToplevel, width: c_int, height: c_int);
    fn wm_capabilities(&mut self, sender: &mut XDGToplevel, capabilities: &mut wl_array);
}
#[repr(C)]
struct ListenerFunctionPointers {
    configure: extern "C" fn(*mut c_void, *mut wl_proxy, c_int, c_int, *mut wl_array),
    close: extern "C" fn(*mut c_void, *mut wl_proxy),
    configure_bounds: extern "C" fn(*mut c_void, *mut wl_proxy, c_int, c_int),
    wm_capabilities: extern "C" fn(*mut c_void, *mut wl_proxy, *mut wl_array),
}

DefineStdOwnedInterface!(pub type OwnedXDGToplevel = XDGToplevel);
impl Drop for OwnedXDGToplevel {
    #[inline(always)]
    fn drop(&mut self) {
        self.destroy();
    }
}

#[repr(transparent)]
pub struct XDGToplevel(wl_proxy);
impl Interface for XDGToplevel {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        &XDG_TOPLEVEL_INTERFACE
    }
}
impl XDGToplevel {
    #[inline]
    pub fn add_listener<L: XDGToplevelListener>(&mut self, listener: &mut L) -> Result<(), ()> {
        extern "C" fn configure<L: XDGToplevelListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            width: c_int,
            height: c_int,
            states: *mut wl_array,
        ) {
            unsafe {
                (&mut *(data as *mut L)).configure(
                    &mut *(sender as *mut XDGToplevel),
                    width,
                    height,
                    &mut *states,
                );
            }
        }
        extern "C" fn close<L: XDGToplevelListener>(data: *mut c_void, sender: *mut wl_proxy) {
            unsafe {
                (&mut *(data as *mut L)).close(&mut *(sender as *mut XDGToplevel));
            }
        }
        extern "C" fn configure_bounds<L: XDGToplevelListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            width: c_int,
            height: c_int,
        ) {
            unsafe {
                (&mut *(data as *mut L)).configure_bounds(
                    &mut *(sender as *mut XDGToplevel),
                    width,
                    height,
                );
            }
        }
        extern "C" fn wm_capabilities<L: XDGToplevelListener>(
            data: *mut c_void,
            sender: *mut wl_proxy,
            capabilities: *mut wl_array,
        ) {
            unsafe {
                (&mut *(data as *mut L))
                    .wm_capabilities(&mut *(sender as *mut XDGToplevel), &mut *capabilities)
            }
        }
        let fps: &'static ListenerFunctionPointers = &ListenerFunctionPointers {
            configure: configure::<L>,
            close: close::<L>,
            configure_bounds: configure_bounds::<L>,
            wm_capabilities: wm_capabilities::<L>,
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
    pub fn set_parent(&mut self, parent: Option<&mut XDGToplevel>) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                1,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                parent.map_or_else(core::ptr::null_mut, |x| x as *mut _ as *mut wl_proxy),
            );
        }
    }

    #[inline(always)]
    pub fn set_title(&mut self, title: &CStr) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                2,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                title.as_ptr(),
            );
        }
    }

    #[inline(always)]
    pub fn set_app_id(&mut self, app_id: &CStr) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                3,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                app_id.as_ptr(),
            );
        }
    }

    #[inline(always)]
    pub fn show_window_menu(&mut self, seat: &mut WlSeat, serial: c_uint, x: c_int, y: c_int) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                4,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                seat as *mut _ as *mut wl_proxy,
                serial,
                x,
                y,
            );
        }
    }

    #[inline(always)]
    pub fn r#move(&mut self, seat: &mut WlSeat, serial: c_uint) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                5,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                seat as *mut _ as *mut wl_proxy,
                serial,
            );
        }
    }

    #[inline(always)]
    pub fn resize(&mut self, seat: &mut WlSeat, serial: c_uint, edges: XDGToplevelResizeEdge) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                6,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                seat as *mut _ as *mut wl_proxy,
                serial,
                edges as c_uint,
            );
        }
    }

    #[inline(always)]
    pub fn set_max_size(&mut self, width: c_int, height: c_int) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                7,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                width,
                height,
            );
        }
    }

    #[inline(always)]
    pub fn set_min_size(&mut self, width: c_int, height: c_int) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                8,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                width,
                height,
            );
        }
    }

    #[inline(always)]
    pub fn set_maximized(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                9,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
            );
        }
    }

    #[inline(always)]
    pub fn unset_maximized(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                10,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
            );
        }
    }

    #[inline(always)]
    pub fn set_fullscreen(&mut self, output: Option<&mut WlOutput>) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                11,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
                output.map_or_else(core::ptr::null_mut::<wl_proxy>, |x| x as *mut _ as _),
            );
        }
    }

    #[inline(always)]
    pub fn unset_fullscreen(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                12,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
            );
        }
    }

    #[inline(always)]
    pub fn set_minimized(&mut self) {
        unsafe {
            wl_proxy_marshal_flags(
                self as *mut _ as _,
                13,
                core::ptr::null(),
                wl_proxy_get_version(self as *mut _ as _),
                0,
            );
        }
    }
}
