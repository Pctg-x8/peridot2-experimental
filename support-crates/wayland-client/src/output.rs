use wayland_sys::{wl_interface, wl_proxy};

use crate::Interface;

extern "C" {
    pub static wl_output_interface: wl_interface;
}

#[repr(transparent)]
pub struct WlOutput(wl_proxy);
impl Interface for WlOutput {
    #[inline(always)]
    fn interface() -> &'static wl_interface {
        unsafe { &wl_output_interface }
    }
}
