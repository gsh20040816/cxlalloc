#[cfg(feature = "backend-mmap")]
mod mmap;
#[cfg(feature = "backend-mmap")]
pub use mmap::Mmap;

#[cfg(feature = "backend-shm")]
pub use shm::Shm;
#[cfg(feature = "backend-shm")]
mod shm;

use core::ffi::CStr;
use core::num::NonZeroUsize;
use core::ops::Deref;
use std::os::fd::AsRawFd;
use std::os::fd::OwnedFd;
use std::os::unix::prelude::RawFd;

use bon::Builder;

use crate::Numa;
use crate::Populate;

#[derive(Builder, Debug, Default)]
pub struct Backend {
    numa: Option<Numa>,
    populate: Option<Populate>,
    #[builder(into)]
    kind: Concrete,
}

impl Backend {
    pub fn numa(&self) -> Option<&Numa> {
        self.numa.as_ref()
    }

    pub fn populate(&self) -> Option<Populate> {
        self.populate
    }
}

impl Deref for Backend {
    type Target = Concrete;
    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

// Note: we use an enum here to avoid dynamic allocation
// of a `Box<dyn Backend>` trait object. This is fine
// because the set of backends should not be extensible
// by downstream consumers.
#[derive(Debug)]
pub enum Concrete {
    #[cfg(feature = "backend-mmap")]
    Mmap(Mmap),
    #[cfg(feature = "backend-shm")]
    Shm(Shm),
}

impl Concrete {
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
            #[cfg(feature = "backend-mmap")]
            Concrete::Mmap(mmap) => mmap,
            #[cfg(feature = "backend-shm")]
            Concrete::Shm(shm) => shm,
        }
    }
}

impl Default for Concrete {
    fn default() -> Self {
        Concrete::Mmap(Mmap)
    }
}

/// Specific backend implementations.
//
// This trait is an implementation detail for requiring
// our backend implementations to expose the same interface.
pub(super) trait Interface: Send + Sync {
    fn name(&self) -> &'static str;

    fn open(&self, id: &CStr, size: NonZeroUsize) -> crate::Result<File>;

    fn unlink(&self, id: &CStr) -> crate::Result<()>;
}

pub struct File {
    fd: Option<OwnedFd>,
    offset: i64,
    create: bool,
}

impl File {
    pub(crate) fn new(fd: Option<OwnedFd>, offset: i64, create: bool) -> Self {
        Self { fd, offset, create }
    }

    pub(crate) fn flags(&self) -> libc::c_int {
        match self.fd {
            Some(_) => libc::MAP_SHARED_VALIDATE,
            None => libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
        }
    }
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_ref().map(|fd| fd.as_raw_fd()).unwrap_or(-1)
    }
}
