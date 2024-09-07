#![cfg(unix)]

use std::os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd};

#[repr(transparent)]
pub struct EventFD(OwnedFd);
impl AsFd for EventFD {
    #[inline(always)]
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl AsRawFd for EventFD {
    #[inline(always)]
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.0.as_raw_fd()
    }
}
impl FromRawFd for EventFD {
    #[inline(always)]
    unsafe fn from_raw_fd(fd: std::os::unix::prelude::RawFd) -> Self {
        Self(std::os::fd::OwnedFd::from_raw_fd(fd))
    }
}
impl IntoRawFd for EventFD {
    #[inline(always)]
    fn into_raw_fd(self) -> std::os::unix::prelude::RawFd {
        self.0.into_raw_fd()
    }
}
impl EventFD {
    pub fn new(init: u32, flags: u32) -> Self {
        unsafe { Self::from_raw_fd(eventfd(init as _, flags as _)) }
    }

    pub fn add(&self, value: u64) -> std::io::Result<()> {
        let r = unsafe {
            write(
                self.0.as_raw_fd(),
                &value as *const _ as _,
                core::mem::size_of::<u64>(),
            )
        };

        if r < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn take(&self) -> std::io::Result<u64> {
        let mut buf = 0u64;
        let r = unsafe {
            read(
                self.0.as_raw_fd(),
                &mut buf as *mut _ as _,
                core::mem::size_of::<u64>(),
            )
        };

        if r < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(buf)
        }
    }
}

extern "C" {
    fn eventfd(count: core::ffi::c_uint, flags: core::ffi::c_uint) -> std::os::fd::RawFd;
    fn read(fd: std::os::fd::RawFd, buf: *mut core::ffi::c_void, count: usize) -> isize;
    fn write(fd: std::os::fd::RawFd, buf: *const core::ffi::c_void, count: usize) -> isize;
}
