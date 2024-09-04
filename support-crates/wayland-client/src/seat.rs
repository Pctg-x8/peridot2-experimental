use wayland_sys::{wl_interface, wl_proxy};

use crate::Interface;

extern "C" {
    pub static wl_seat_interface: wl_interface;
}

#[repr(transparent)]
pub struct WlSeat(wl_proxy);
impl Interface for WlSeat {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        unsafe { &wl_seat_interface }
    }
}
