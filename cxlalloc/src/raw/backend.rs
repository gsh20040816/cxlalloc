#[path = "mmap.rs"]
mod mmap;

#[cfg(feature = "backend-ivshmem")]
#[path = "ivshmem.rs"]
mod ivshmem;

#[cfg(feature = "backend-shm")]
#[path = "shm.rs"]
mod shm;

pub use ::shm::Numa;
pub use ::shm::Populate;
pub use mmap::Mmap;

#[cfg(feature = "backend-ivshmem")]
pub use ivshmem::Ivshmem;

#[cfg(feature = "backend-shm")]
pub use shm::Shm;

use core::num::NonZeroUsize;
use core::ops::Deref;
use std::os::fd::AsRawFd as _;
use std::os::fd::OwnedFd;

use bon::Builder;

use crate::raw::region;

#[derive(Builder, Debug, Default)]
pub struct Backend {
    numa: Option<::shm::Numa>,
    populate: Option<::shm::Populate>,
    #[builder(into)]
    kind: Kind,
}

impl Backend {
    pub(super) fn numa(&self) -> Option<&::shm::Numa> {
        self.numa.as_ref()
    }

    pub(super) fn populate(&self) -> Option<::shm::Populate> {
        self.populate
    }
}

impl Deref for Backend {
    type Target = Kind;
    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

// Note: we use an enum here to avoid dynamic allocation
// of a `Box<dyn Backend>` trait object. This is fine
// because the set of backends should not be extensible
// by downstream consumers.
#[derive(Debug)]
pub enum Kind {
    Mmap(Mmap),
    #[cfg(feature = "backend-ivshmem")]
    Ivshmem(Ivshmem),
    #[cfg(feature = "backend-shm")]
    Shm(Shm),
}

impl Kind {
    pub(super) fn allocate(&self, id: region::Id, size: NonZeroUsize) -> crate::Result<File> {
        self.as_backend().allocate(id, size)
    }

    pub(super) fn name(&self) -> &str {
        self.as_backend().name()
    }

    pub(super) fn unlink(&self, id: &region::Id) -> crate::Result<()> {
        self.as_backend().unlink(id)
    }

    fn as_backend(&self) -> &dyn Impl {
        match self {
            Kind::Mmap(mmap) => mmap,
            #[cfg(feature = "backend-ivshmem")]
            Kind::Ivshmem(ivshmem) => ivshmem,
            #[cfg(feature = "backend-shm")]
            Kind::Shm(shm) => shm,
        }
    }
}

impl Default for Kind {
    fn default() -> Self {
        Kind::Mmap(Mmap)
    }
}

/// Specific backend implementations.
//
// This trait is an implementation detail for requiring
// our backend implementations to expose the same interface.
pub(super) trait Impl: Send + Sync {
    fn name(&self) -> &'static str;

    fn allocate(&self, id: region::Id, size: NonZeroUsize) -> crate::Result<File>;

    fn unlink(&self, id: &region::Id) -> crate::Result<()>;
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
