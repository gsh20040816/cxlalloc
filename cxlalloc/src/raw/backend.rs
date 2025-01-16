#[path = "mmap.rs"]
mod mmap;

#[cfg(feature = "backend-ivshmem")]
#[path = "ivshmem.rs"]
mod ivshmem;

#[cfg(feature = "backend-shm")]
#[path = "shm.rs"]
mod shm;

pub use mmap::Mmap;

#[cfg(feature = "backend-ivshmem")]
pub use ivshmem::Ivshmem;

#[cfg(feature = "backend-shm")]
pub use shm::Shm;

use core::num::NonZeroUsize;
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::fd::BorrowedFd;

use crate::raw::Region;
use crate::raw::Reservation;

// Note: we use an enum here to avoid dynamic allocation
// of a `Box<dyn Backend>` trait object. This is fine
// because the set of backends should not be extensible
// by downstream consumers.
#[derive(Debug)]
pub enum Backend {
    Mmap(Mmap),
    #[cfg(feature = "backend-ivshmem")]
    Ivshmem(Ivshmem),
    #[cfg(feature = "backend-shm")]
    Shm(Shm),
}

impl Backend {
    pub(crate) fn allocate(
        &self,
        id: String,
        reservation: Option<Reservation>,
        size: usize,
    ) -> io::Result<Region> {
        let backend = self.as_backend();
        backend
            .allocate(id.clone(), reservation, size)
            .inspect(|region| {
                log::info!(
                    "Allocated {:#x} bytes for {} ({:#x?} - {:#x?}) using {} backend",
                    region.size(),
                    id,
                    region.address().as_ptr(),
                    region.address().as_ptr().wrapping_byte_add(region.size()),
                    backend.name(),
                );
            })
    }

    pub(crate) fn map(&self, region: &Region, offset: usize, size: NonZeroUsize) -> io::Result<()> {
        self.as_backend().map(region, offset, size)
    }

    pub(crate) fn unmap(&self, region: &Region) -> io::Result<()> {
        self.as_backend().unmap(region)
    }

    pub(crate) fn free(&self, region: &Region) -> io::Result<()> {
        self.as_backend().free(region)
    }

    pub(crate) fn name(&self) -> &str {
        self.as_backend().name()
    }

    fn as_backend(&self) -> &dyn Impl {
        match self {
            Backend::Mmap(mmap) => mmap,
            #[cfg(feature = "backend-ivshmem")]
            Backend::Ivshmem(ivshmem) => ivshmem,
            #[cfg(feature = "backend-shm")]
            Backend::Shm(shm) => shm,
        }
    }
}

/// Specific backend implementations.
//
// This trait is an implementation detail for requiring
// our backend implementations to expose the same interface.
trait Impl: Send + Sync {
    fn name(&self) -> &'static str;

    fn allocate(
        &self,
        id: String,
        reservation: Option<Reservation>,
        size: usize,
    ) -> io::Result<Region>;

    fn map(&self, region: &Region, offset: usize, size: NonZeroUsize) -> io::Result<()>;

    fn unmap(&self, region: &Region) -> io::Result<()>;

    fn free(&self, region: &Region) -> io::Result<()>;
}

#[derive(Copy, Clone)]
pub(super) struct File<'fd> {
    fd: Option<BorrowedFd<'fd>>,
    pub(super) offset: i64,
    pub(super) clean: bool,
}

impl<'fd> File<'fd> {
    fn new(fd: BorrowedFd<'fd>, offset: i64, clean: bool) -> Self {
        Self {
            fd: Some(fd),
            offset,
            clean,
        }
    }

    pub(super) fn fd(&self) -> i32 {
        self.fd.map(|fd| fd.as_raw_fd()).unwrap_or(-1)
    }

    pub(super) fn flags(&self) -> libc::c_int {
        match self.fd {
            Some(_) => libc::MAP_SHARED_VALIDATE,
            None => libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
        }
    }
}

impl Default for File<'static> {
    fn default() -> Self {
        File {
            fd: None,
            offset: 0,
            clean: true,
        }
    }
}
