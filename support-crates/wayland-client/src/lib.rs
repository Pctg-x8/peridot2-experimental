mod display;
pub use display::*;
mod registry;
pub use registry::*;
mod compositor;
pub use compositor::*;
mod surface;
pub use surface::*;
mod callback;
pub use callback::*;
mod seat;
pub use seat::*;
mod output;
pub use output::*;

mod xdg;
pub use xdg::*;

pub use wayland_sys::wl_array;

use wayland_sys::{wl_interface, wl_proxy};

pub trait Interface {
    fn interface() -> &'static wl_interface;
}

pub trait OwnableInterface: Interface {
    type OwnedType;

    unsafe fn take_from_proxy_ptr(ptr: core::ptr::NonNull<wl_proxy>) -> Self::OwnedType;
}

#[macro_export]
macro_rules! DefineStdOwnedInterface {
    ($vis: vis type $owned_ty: ident = $ref_ty: ty) => {
        #[repr(transparent)]
        $vis struct $owned_ty(core::ptr::NonNull<$ref_ty>);
        impl core::ops::Deref for $owned_ty {
            type Target = $ref_ty;

            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                unsafe { self.0.as_ref() }
            }
        }
        impl core::ops::DerefMut for $owned_ty {
            #[inline(always)]
            fn deref_mut(&mut self) -> &mut Self::Target {
                unsafe { self.0.as_mut() }
            }
        }
        impl crate::OwnableInterface for $ref_ty {
            type OwnedType = $owned_ty;

            #[inline(always)]
            unsafe fn take_from_proxy_ptr(ptr: core::ptr::NonNull<wl_proxy>) -> Self::OwnedType {
                $owned_ty(ptr.cast())
            }
        }
    }
}
