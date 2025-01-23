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
use std::os::fd::OwnedFd;

use crate::raw::region;

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
    pub(super) fn allocate(&self, id: region::Id, size: NonZeroUsize) -> io::Result<File> {
        let backend = self.as_backend();
        backend.allocate(id, size)
    }

    pub(super) fn name(&self) -> &str {
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

    fn allocate(&self, id: region::Id, size: NonZeroUsize) -> io::Result<File>;
}

pub(super) struct File {
    fd: Option<OwnedFd>,
    pub(super) offset: i64,
    pub(super) clean: bool,
}

impl File {
    #[cfg_attr(
        not(any(feature = "backend-shm", feature = "backend-ivshmem")),
        expect(unused)
    )]
    fn new(fd: OwnedFd, offset: i64, clean: bool) -> Self {
        Self {
            fd: Some(fd),
            offset,
            clean,
        }
    }

    pub(super) fn fd(&self) -> i32 {
        self.fd.as_ref().map(|fd| fd.as_raw_fd()).unwrap_or(-1)
    }

    pub(super) fn flags(&self) -> libc::c_int {
        match self.fd {
            Some(_) => libc::MAP_SHARED_VALIDATE,
            None => libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
        }
    }
}

impl Default for File {
    fn default() -> Self {
        File {
            fd: None,
            offset: 0,
            clean: true,
        }
    }
}
