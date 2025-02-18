use core::ffi;
use core::ffi::CStr;
use core::fmt::Display;
use core::ptr;
use std::io;

use clap::ValueEnum;

mod boost;
pub mod cxl_shm;
pub mod cxlalloc;
pub mod lightning;

pub use boost::Boost;
pub use cxl_shm::CxlShm;
pub use cxlalloc::Backend;
use serde::Serialize;

#[derive(Clone, ValueEnum, Serialize)]
pub enum Allocator {
    Boost,
    Cxlalloc,
    CxlShm,
    Lightning,
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Allocator::Boost => "boost",
            Allocator::Cxlalloc => "cxlalloc",
            Allocator::CxlShm => "cxl-shm",
            Allocator::Lightning => "lightning",
        };

        write!(f, "{}", name)
    }
}

fn open(node: usize, name: &CStr, size: usize) -> io::Result<*mut ffi::c_void> {
    unsafe {
        let fd = match libc::shm_open(name.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o666) {
            -1 => return Err(io::Error::last_os_error()),
            fd => fd,
        };

        if libc::ftruncate(fd, size as i64) == -1 {
            return Err(io::Error::last_os_error());
        }

        let address = match libc::mmap64(
            ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED_VALIDATE,
            fd,
            0,
        ) {
            libc::MAP_FAILED => return Err(io::Error::last_os_error()),
            address => address,
        };

        mbind(node, address, size)?;
        Ok(address)
    }
}

fn unlink(name: &CStr) -> io::Result<()> {
    unsafe {
        match libc::shm_unlink(name.as_ptr()) {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

// https://github.com/numactl/numactl/blob/6c14bd59d438ebb5ef828e393e8563ba18f59cb2/syscall.c#L230-L235
fn mbind(node: usize, address: *mut ffi::c_void, size: usize) -> io::Result<()> {
    let mask = 1u64 << node;
    match unsafe {
        libc::syscall(
            libc::SYS_mbind,
            address,
            size,
            libc::MPOL_BIND | libc::MPOL_F_STATIC_NODES,
            &mask,
            64,
            // MPOL_MF_STRICT
            // https://github.com/torvalds/linux/blob/0c559323bbaabee7346c12e74b497e283aaafef5/include/uapi/linux/mempolicy.h#L48
            1,
        )
    } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}
