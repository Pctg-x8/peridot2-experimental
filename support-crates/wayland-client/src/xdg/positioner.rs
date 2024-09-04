use core::ffi::*;
use wayland_sys::{wl_interface, wl_message, wl_proxy};

use crate::Interface;

#[repr(C)]
pub enum XDGPositionerError {
    /// invalid input provided
    InvalidInput = 0,
}

#[repr(C)]
pub enum XDGPositionerAnchor {
    None = 0,
    Top = 1,
    Bottom = 2,
    Left = 3,
    Right = 4,
    TopLeft = 5,
    BottomLeft = 6,
    TopRight = 7,
    BottomRight = 8,
}

#[repr(C)]
pub enum XDGPositionerGravity {
    None = 0,
    Top = 1,
    Bottom = 2,
    Left = 3,
    Right = 4,
    TopLeft = 5,
    BottomLeft = 6,
    TopRight = 7,
    BottomRight = 8,
}

bitflags::bitflags! {
    pub struct XDGPositionerConstraintAdjustment: c_uint {
        const NONE = 0;
        const SLIDE_X = 1;
        const SLIDE_Y = 2;
        const FLIP_X = 4;
        const FLIP_Y = 8;
        const RESIZE_X = 16;
        const RESIZE_Y = 32;
    }
}

pub static XDG_POSITIONER_INTERFACE: wl_interface = wl_interface::new(
    c"xdg_positioner",
    6,
    &[
        wl_message::new(c"destroy", c"", &[]),
        wl_message::new(c"set_size", c"ii", &[core::ptr::null(); 2]),
        wl_message::new(c"set_anchor_rect", c"iiii", &[core::ptr::null(); 4]),
        wl_message::new(c"set_anchor", c"u", &[core::ptr::null()]),
        wl_message::new(c"set_gravity", c"u", &[core::ptr::null()]),
        wl_message::new(c"set_constraint_adjustment", c"u", &[core::ptr::null()]),
        wl_message::new(c"set_offset", c"ii", &[core::ptr::null(); 2]),
        wl_message::new(c"set_reactive", c"3", &[]),
        wl_message::new(c"set_parent_size", c"3ii", &[core::ptr::null(); 2]),
        wl_message::new(c"set_parent_configure", c"3u", &[core::ptr::null()]),
    ],
    &[],
);

#[repr(transparent)]
pub struct XDGPositioner(wl_proxy);
impl Interface for XDGPositioner {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        &XDG_POSITIONER_INTERFACE
    }
}
