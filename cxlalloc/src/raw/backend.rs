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

use core::ffi;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use std::io;

use crate::raw::Region;

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
        address: Option<NonNull<ffi::c_void>>,
        size: usize,
        reserved: Option<NonZeroUsize>,
    ) -> io::Result<Region> {
        let backend = self.as_backend();
        backend
            .allocate(id, address, size, reserved)
            .inspect(|region| {
                log::info!(
                    "Allocated {} bytes ({:#x?} - {:#x?}) using {} backend",
                    region.size(),
                    region.base().as_ptr(),
                    region.base().as_ptr().wrapping_byte_add(region.size()),
                    backend.name(),
                );
            })
    }

    #[cfg_attr(not(feature = "extend"), allow(dead_code))]
    pub(crate) fn extend(&self, region: &Region) -> io::Result<()> {
        self.as_backend().extend(region)
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
        address: Option<NonNull<ffi::c_void>>,
        size: usize,
        reserved: Option<NonZeroUsize>,
    ) -> io::Result<Region>;

    fn extend(&self, region: &Region) -> io::Result<()>;

    fn unmap(&self, region: &Region) -> io::Result<()>;

    fn free(&self, region: &Region) -> io::Result<()>;
}
