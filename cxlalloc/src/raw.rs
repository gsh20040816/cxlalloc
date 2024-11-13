pub(crate) mod heap;
pub(crate) mod region;

pub use heap::Builder;
pub use heap::Heap;
pub(crate) use region::Region;

use core::any;
use std::io;

// Note: we use an enum here to avoid dynamic allocation
// of a `Box<dyn Backend>` trait object. This is fine
// because the set of backends should not be extensible
// by downstream consumers.
#[derive(Clone, Debug)]
pub enum Backend {
    Mmap(backend::Mmap),
    #[cfg(feature = "backend-ivshmem")]
    Ivshmem(backend::Ivshmem),
    #[cfg(feature = "backend-shm")]
    Shm(backend::Shm),
}

impl Backend {
    pub(crate) fn allocate(&self, id: String, size: usize, reserved: usize) -> io::Result<Region> {
        self.as_backend()
            .allocate(id, size, reserved)
            .inspect(|region| {
                log::info!(
                    "Allocated {} bytes ({:#x?} - {:#x?}) using {} backend",
                    region.size(),
                    region.base().as_ptr(),
                    unsafe { region.base().as_ptr().byte_add(region.size()) },
                    any::type_name::<Self>(),
                );
            })
    }

    #[cfg_attr(not(feature = "extend"), allow(dead_code))]
    pub(crate) fn extend(&self, region: &Region) -> io::Result<()> {
        self.as_backend().extend(region)
    }

    pub(crate) fn free(&self, region: &Region) -> io::Result<()> {
        self.as_backend().free(region)
    }

    fn as_backend(&self) -> &dyn backend::Backend {
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
#[path = "raw"]
pub mod backend {
    mod mmap;

    #[cfg(feature = "backend-ivshmem")]
    mod ivshmem;

    #[cfg(feature = "backend-shm")]
    mod shm;

    pub use mmap::Mmap;

    #[cfg(feature = "backend-ivshmem")]
    pub use ivshmem::Ivshmem;

    #[cfg(feature = "backend-shm")]
    pub use shm::Shm;

    use std::io;

    use crate::raw::Region;

    // This trait is an implementation detail for requiring
    // our backend implementations to expose the same interface.
    pub(super) trait Backend: Send + Sync {
        fn allocate(&self, id: String, size: usize, reserved: usize) -> io::Result<Region>;

        fn extend(&self, region: &Region) -> io::Result<()>;

        fn free(&self, region: &Region) -> io::Result<()>;
    }
}
