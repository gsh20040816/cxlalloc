use core::ffi::CStr;
use core::mem;
use core::ptr;
use std::ffi;
use std::ffi::CString;
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;

use bon::bon;

use crate::Numa;
use crate::PAGE;
use crate::Populate;

pub struct Raw {
    pub(crate) name: CString,
    pub(crate) size: usize,
    pub(crate) address: *mut ffi::c_void,
}

#[bon]
impl Raw {
    #[builder]
    pub fn new(
        numa: Option<Numa>,
        name: CString,
        size: usize,
        #[builder(default)] create: bool,
        populate: Option<Populate>,
    ) -> io::Result<Raw> {
        assert!(
            name.as_bytes()[0] == b'/',
            "Shared memory name {:?} should start with /",
            name.to_string_lossy(),
        );

        let size = size.next_multiple_of(PAGE);

        if create {
            match Self::unlink_inner(&name) {
                Ok(()) => log::info!("Unlinked stale shm object: {}", name.to_string_lossy()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }

        let (create, fd) = match unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o666,
            )
        } {
            -1 => {
                match io::Error::last_os_error() {
                    error if error.kind() == io::ErrorKind::AlreadyExists => (),
                    error => return Err(error),
                }

                match unsafe { libc::shm_open(name.as_ptr(), libc::O_RDWR, 0o666) } {
                    -1 => return Err(io::Error::last_os_error()),
                    fd => (false, unsafe { OwnedFd::from_raw_fd(fd) }),
                }
            }
            fd => (true, unsafe { OwnedFd::from_raw_fd(fd) }),
        };

        if create && unsafe { libc::ftruncate(fd.as_raw_fd(), size as i64) } == -1 {
            return Err(io::Error::last_os_error());
        }

        let address = match unsafe {
            libc::mmap64(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED_VALIDATE
                    | if matches!(populate, Some(Populate::PageTable)) {
                        libc::MAP_POPULATE
                    } else {
                        0
                    },
                fd.as_raw_fd(),
                0,
            )
        } {
            libc::MAP_FAILED => return Err(io::Error::last_os_error()),
            address => address,
        };

        if let (true, Some(numa)) = (create, numa) {
            unsafe {
                Self::mbind(numa, address, size)?;
            }
        }

        if matches!(populate, Some(Populate::Physical)) {
            unsafe {
                Self::madvise(address, size)?;
            }
        }

        Ok(Self {
            name,
            size,
            address,
        })
    }
}

impl Raw {
    pub fn address(&self) -> *const ffi::c_void {
        self.address
    }

    pub fn address_mut(&self) -> *mut ffi::c_void {
        self.address
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn unmap(mut self) -> io::Result<()> {
        let address = mem::replace(&mut self.address, ptr::null_mut());
        match unsafe { libc::munmap(address, self.size) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    pub fn unlink(&mut self) -> io::Result<()> {
        Self::unlink_inner(&self.name)
    }

    fn unlink_inner(name: &CStr) -> io::Result<()> {
        if unsafe { libc::shm_unlink(name.as_ptr()) } == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    // https://github.com/numactl/numactl/blob/6c14bd59d438ebb5ef828e393e8563ba18f59cb2/syscall.c#L230-L235
    pub unsafe fn mbind(numa: Numa, address: *mut ffi::c_void, size: usize) -> io::Result<()> {
        let (policy, mask) = match numa {
            Numa::Bind { node } => (libc::MPOL_BIND, 1u64 << node),
            Numa::Interleave { nodes } => (
                libc::MPOL_INTERLEAVE,
                nodes
                    .into_iter()
                    .map(|node| 1u64 << node)
                    .fold(0, |l, r| l | r),
            ),
        };

        match unsafe {
            libc::syscall(
                libc::SYS_mbind,
                address,
                size,
                libc::MPOL_F_STATIC_NODES | policy,
                &mask,
                64,
                // MPOL_MF_STRICT sometimes raises EIO when called concurrently for the same
                // address range, so disable for now.
                // https://github.com/torvalds/linux/blob/0c559323bbaabee7346c12e74b497e283aaafef5/include/uapi/linux/mempolicy.h#L48
                0,
            )
        } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub unsafe fn madvise(address: *mut ffi::c_void, size: usize) -> io::Result<()> {
        match unsafe { libc::madvise(address, size, libc::MADV_POPULATE_WRITE) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        if self.address.is_null() {
            return;
        }

        unsafe {
            if libc::munmap(self.address, self.size) == -1 {
                panic!(
                    "Failed to munmap {:#x?} ({:#x}): {:?}",
                    self.address,
                    self.size,
                    io::Error::last_os_error()
                );
            }
        }
    }
}
