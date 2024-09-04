use wayland_sys::{wl_interface, wl_message, wl_proxy};

use crate::{wl_seat_interface, Interface};

use super::XDG_POSITIONER_INTERFACE;

#[repr(C)]
pub enum XDGPopupError {
    /// tried to grab after being mapped
    InvalidGrab = 0,
}

pub static XDG_POPUP_INTERFACE: wl_interface = wl_interface::new(
    c"xdg_popup",
    6,
    &[
        wl_message::new(c"destroy", c"", &[]),
        wl_message::new(
            c"grab",
            c"ou",
            &[unsafe { &wl_seat_interface }, core::ptr::null()],
        ),
        wl_message::new(
            c"reposition",
            c"3ou",
            &[&XDG_POSITIONER_INTERFACE, core::ptr::null()],
        ),
    ],
    &[
        wl_message::new(c"configure", c"iiii", &[core::ptr::null(); 4]),
        wl_message::new(c"popup_done", c"", &[]),
        wl_message::new(c"repositioned", c"3u", &[core::ptr::null()]),
    ],
);

#[repr(transparent)]
pub struct XDGPopup(wl_proxy);
impl Interface for XDGPopup {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        &XDG_POPUP_INTERFACE
    }
}
