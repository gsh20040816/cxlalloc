mod mmap;
mod shm;

pub use mmap::Mmap;
pub use shm::Shm;

use core::ffi;
use core::ffi::CStr;
use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;
use std::io;
use std::os::fd::AsRawFd;
use std::os::fd::OwnedFd;
use std::os::unix::prelude::RawFd;

use crate::Numa;
use crate::Page;
use crate::Populate;

// Note: we use an enum here to avoid dynamic allocation
// of a `Box<dyn Backend>` trait object. This is fine
// because the set of backends should not be extensible
// by downstream consumers.
#[derive(Debug)]
pub enum Backend {
    Mmap(Mmap),
    Shm(Shm),
}

impl Backend {
    pub fn open(&self, id: &CStr, size: NonZeroUsize) -> crate::Result<File> {
        self.as_backend().open(id, size)
    }

    pub fn name(&self) -> &str {
        self.as_backend().name()
    }

    pub fn unlink(&self, id: &CStr) -> crate::Result<()> {
        self.as_backend().unlink(id)
    }

    fn as_backend(&self) -> &dyn Interface {
        match self {
            Backend::Mmap(mmap) => mmap,
            Backend::Shm(shm) => shm,
        }
    }
}

impl Default for Backend {
    fn default() -> Self {
        Backend::Mmap(Mmap)
    }
}

// This trait is an implementation detail for requiring
// our backend implementations to expose the same interface.
pub(super) trait Interface: Send + Sync {
    fn name(&self) -> &'static str;

    fn open(&self, id: &CStr, size: NonZeroUsize) -> crate::Result<File>;

    fn unlink(&self, id: &CStr) -> crate::Result<()>;
}

pub struct File {
    fd: Option<OwnedFd>,
    size: NonZeroUsize,
    offset: i64,
    create: bool,
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_ref().map(|fd| fd.as_raw_fd()).unwrap_or(-1)
    }
}

impl File {
    /// Whether this file is newly created or already existed.
    pub fn is_create(&self) -> bool {
        self.create
    }

    pub(crate) fn flags(&self) -> libc::c_int {
        match self.fd {
            Some(_) => libc::MAP_SHARED_VALIDATE,
            None => libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
        }
    }
}

#[bon::bon]
impl File {
    #[builder]
    pub(crate) fn new(fd: Option<OwnedFd>, size: NonZeroUsize, offset: i64, create: bool) -> Self {
        Self {
            fd,
            size,
            offset,
            create,
        }
    }

    /// SAFETY: caller must ensure `address` does not overlap an existing memory region.
    #[builder]
    pub unsafe fn map(
        self,
        address: Option<NonNull<Page>>,
        numa: Option<Numa>,
        populate: Option<Populate>,
    ) -> crate::Result<NonNull<Page>> {
        let actual = match unsafe {
            libc::mmap64(
                address
                    .map(NonNull::as_ptr)
                    .unwrap_or_else(ptr::null_mut)
                    .cast(),
                self.size.get(),
                libc::PROT_READ | libc::PROT_WRITE,
                self.flags()
                    | address.map(|_| libc::MAP_FIXED).unwrap_or(0)
                    | if matches!(populate, Some(Populate::PageTable)) {
                        libc::MAP_POPULATE
                    } else {
                        0
                    },
                self.as_raw_fd(),
                self.offset,
            )
        } {
            libc::MAP_FAILED => Err(crate::Error::Libc {
                name: "mmap64",
                source: io::Error::last_os_error(),
            }),
            actual => Ok(NonNull::new(actual).unwrap().cast::<Page>()),
        }?;

        if let Some(expected) = address {
            assert_eq!(expected, actual);
        }

        if let Some(numa) = numa {
            mbind(numa, actual.as_ptr().cast(), self.size.get())?;
        }

        if matches!(populate, Some(Populate::Physical)) {
            madvise(actual.as_ptr().cast(), self.size.get())?;
        }

        Ok(actual)
    }
}

#[expect(clippy::not_unsafe_ptr_arg_deref)]
pub fn mbind(numa: Numa, address: *mut ffi::c_void, size: usize) -> crate::Result<()> {
    // https://github.com/numactl/numactl/blob/6c14bd59d438ebb5ef828e393e8563ba18f59cb2/syscall.c#L230-L235
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
        ))?;
    }

    Ok(())
}

#[expect(clippy::not_unsafe_ptr_arg_deref)]
pub fn madvise(address: *mut ffi::c_void, size: usize) -> crate::Result<()> {
    unsafe { crate::try_libc!(libc::madvise(address, size, libc::MADV_POPULATE_WRITE)) }?;
    Ok(())
}
