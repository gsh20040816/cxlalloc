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
use crate::Page;
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
    ) -> crate::Result<Self> {
        assert!(
            name.as_bytes()[0] == b'/',
            "Shared memory name {:?} should start with /",
            name.to_string_lossy(),
        );

        let size = size.next_multiple_of(Page::SIZE);

        if create {
            match shm_unlink(&name) {
                Ok(()) => log::info!("Unlinked stale shm object: {}", name.to_string_lossy()),
                Err(error) if error.is_not_found() => (),
                Err(error) => return Err(error),
            }
        }

        let (create, fd) = match unsafe {
            crate::try_libc!(libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o666,
            ))
        } {
            Err(error) if error.is_already_exists() => unsafe {
                let fd = crate::try_libc!(libc::shm_open(name.as_ptr(), libc::O_RDWR, 0o666))
                    .map(|fd| OwnedFd::from_raw_fd(fd))?;
                (false, fd)
            },
            Err(error) => return Err(error),
            Ok(fd) => (true, unsafe { OwnedFd::from_raw_fd(fd) }),
        };

        if create {
            unsafe {
                crate::try_libc!(libc::ftruncate64(fd.as_raw_fd(), size as i64))?;
            }
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
            libc::MAP_FAILED => {
                return Err(crate::Error::Libc {
                    name: "mmap64",
                    source: io::Error::last_os_error(),
                });
            }
            address => address,
        };

        if let (true, Some(numa)) = (create, numa) {
            Self::mbind(numa, address, size)?;
        }

        if matches!(populate, Some(Populate::Physical)) {
            Self::madvise(address, size)?;
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

    pub fn unlink(&mut self) -> crate::Result<()> {
        shm_unlink(&self.name)
    }

    // https://github.com/numactl/numactl/blob/6c14bd59d438ebb5ef828e393e8563ba18f59cb2/syscall.c#L230-L235
    #[expect(clippy::not_unsafe_ptr_arg_deref)]
    pub fn mbind(numa: Numa, address: *mut ffi::c_void, size: usize) -> crate::Result<()> {
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

        unsafe {
            crate::try_libc!(mbind_syscall(
                address,
                size as u64,
                libc::MPOL_F_STATIC_NODES | policy,
                &mask,
                64,
                // MPOL_MF_STRICT sometimes raises EIO when called concurrently for the same
                // address range, so disable for now.
                // https://github.com/torvalds/linux/blob/0c559323bbaabee7346c12e74b497e283aaafef5/include/uapi/linux/mempolicy.h#L48
                0,
            ))
        }
        .map(drop)
    }

    #[expect(clippy::not_unsafe_ptr_arg_deref)]
    pub fn madvise(address: *mut ffi::c_void, size: usize) -> crate::Result<()> {
        unsafe { crate::try_libc!(libc::madvise(address, size, libc::MADV_POPULATE_WRITE)) }?;
        Ok(())
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

fn shm_unlink(name: &CStr) -> crate::Result<()> {
    unsafe { crate::try_libc!(libc::shm_unlink(name.as_ptr())) }?;
    Ok(())
}

// https://github.com/numactl/numactl/blob/63e02235bdbcf5aa334903be2111a82b27c8c155/syscall.c#L230
unsafe fn mbind_syscall(
    address: *mut ffi::c_void,
    size: libc::c_ulong,
    mode: libc::c_int,
    mask: *const libc::c_ulong,
    maxnode: libc::c_ulong,
    flags: libc::c_uint,
) -> i64 {
    unsafe { libc::syscall(libc::SYS_mbind, address, size, mode, mask, maxnode, flags) }
}
