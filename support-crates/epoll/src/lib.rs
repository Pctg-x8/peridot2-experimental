#![cfg(unix)]

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

#[repr(transparent)]
pub struct Epoll(OwnedFd);
impl Epoll {
    #[inline(always)]
    pub fn new(reserve_count: usize) -> Self {
        Self(unsafe { OwnedFd::from_raw_fd(epoll_create(reserve_count as _)) })
    }

    #[inline]
    pub fn add(
        &mut self,
        fd: std::os::fd::RawFd,
        event_id: u32,
        event_data: EpollData,
    ) -> std::io::Result<()> {
        let res = unsafe {
            epoll_ctl(
                self.0.as_raw_fd(),
                EPOLL_CTL_ADD,
                fd,
                &mut epoll_event {
                    events: event_id,
                    data: event_data.make_native(),
                },
            )
        };

        if res == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    #[inline]
    pub fn delete(&mut self, fd: std::os::fd::RawFd) -> std::io::Result<()> {
        // Linux 2.6.9以前にも対応する必要がある場合はevents引数にダミーの値を渡す必要がある(が、さすがにもう無視してもいい気も......)
        // https://manpages.ubuntu.com/manpages/focal/ja/man2/epoll_ctl.2.html

        let res =
            unsafe { epoll_ctl(self.0.as_raw_fd(), EPOLL_CTL_DEL, fd, core::ptr::null_mut()) };

        if res == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    #[inline]
    pub fn modify(
        &mut self,
        fd: std::os::fd::RawFd,
        event_id: u32,
        event_data: EpollData,
    ) -> std::io::Result<()> {
        let res = unsafe {
            epoll_ctl(
                self.0.as_raw_fd(),
                EPOLL_CTL_MOD,
                fd,
                &mut epoll_event {
                    events: event_id,
                    data: event_data.make_native(),
                },
            )
        };

        if res == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    #[inline]
    pub fn wait(
        &mut self,
        events: &mut [epoll_event],
        timeout: Option<core::ffi::c_int>,
    ) -> std::io::Result<usize> {
        let res = unsafe {
            epoll_wait(
                self.0.as_raw_fd(),
                events.as_mut_ptr(),
                events.len() as _,
                timeout.unwrap_or(-1),
            )
        };

        if res < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res as _)
        }
    }
}

pub enum EpollData {
    Pointer(*mut core::ffi::c_void),
    FileDescriptor(std::os::fd::RawFd),
    Uint32(u32),
    Uint64(u64),
}
impl EpollData {
    #[inline]
    const fn make_native(&self) -> epoll_data_t {
        match self {
            &Self::Pointer(ptr) => epoll_data_t { ptr },
            &Self::FileDescriptor(fd) => epoll_data_t { fd },
            &Self::Uint32(x) => epoll_data_t { r#u32: x },
            &Self::Uint64(x) => epoll_data_t { r#u64: x },
        }
    }
}

const EPOLL_CTL_ADD: core::ffi::c_int = 1;
const EPOLL_CTL_DEL: core::ffi::c_int = 2;
const EPOLL_CTL_MOD: core::ffi::c_int = 3;

pub const EPOLLIN: u32 = 0x001;
pub const EPOLLET: u32 = 1u32 << 31;

#[repr(C)]
#[allow(non_camel_case_types)]
pub union epoll_data_t {
    pub ptr: *mut core::ffi::c_void,
    pub fd: std::os::fd::RawFd,
    pub r#u32: u32,
    pub r#u64: u64,
}

#[repr(C, packed)]
#[allow(non_camel_case_types)]
pub struct epoll_event {
    pub events: u32,
    pub data: epoll_data_t,
}

extern "C" {
    fn epoll_create(size: core::ffi::c_int) -> std::os::fd::RawFd;
    fn epoll_ctl(
        epfd: std::os::fd::RawFd,
        op: core::ffi::c_int,
        fd: std::os::fd::RawFd,
        event: *mut epoll_event,
    ) -> core::ffi::c_int;
    fn epoll_wait(
        epfd: std::os::fd::RawFd,
        events: *mut epoll_event,
        maxevents: core::ffi::c_int,
        timeout: core::ffi::c_int,
    ) -> core::ffi::c_int;
}
