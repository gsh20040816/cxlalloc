pub use shm::backend::Mmap;
pub use shm::backend::Shm;
pub use shm::Numa;
pub use shm::Page;
pub use shm::Populate;

mod plain_ivshmem;
pub use plain_ivshmem::PlainIvshmem;

use core::num::NonZeroUsize;
use core::ptr::NonNull;

use bon::Builder;

#[derive(Debug)]
pub enum Driver {
    Shm(::shm::backend::Backend),
    PlainIvshmem(PlainIvshmem),
}

impl Default for Driver {
    fn default() -> Self {
        Self::Shm(Default::default())
    }
}

impl From<::shm::backend::Backend> for Driver {
    fn from(backend: ::shm::backend::Backend) -> Self {
        Self::Shm(backend)
    }
}

impl From<Mmap> for Driver {
    fn from(backend: Mmap) -> Self {
        Self::Shm(backend.into())
    }
}

impl From<Shm> for Driver {
    fn from(backend: Shm) -> Self {
        Self::Shm(backend.into())
    }
}

impl From<PlainIvshmem> for Driver {
    fn from(backend: PlainIvshmem) -> Self {
        Self::PlainIvshmem(backend)
    }
}

#[derive(Builder, Debug, Default)]
pub struct Backend {
    numa: Option<::shm::Numa>,
    populate: Option<::shm::Populate>,
    #[builder(into)]
    backend: Driver,
}

impl Backend {
    pub(super) fn numa(&self) -> Option<&::shm::Numa> {
        self.numa.as_ref()
    }

    pub(super) fn populate(&self) -> Option<::shm::Populate> {
        self.populate
    }

    pub(super) fn open(&self, id: &str, size: NonZeroUsize) -> crate::Result<File> {
        match &self.backend {
            Driver::Shm(backend) => backend.open(id, size).map(File::Shm).map_err(Into::into),
            Driver::PlainIvshmem(backend) => backend.open(id, size).map(File::PlainIvshmem),
        }
    }

    pub(crate) fn name(&self) -> &str {
        match &self.backend {
            Driver::Shm(backend) => backend.name(),
            Driver::PlainIvshmem(backend) => backend.name(),
        }
    }

    pub(crate) fn unlink(&self, id: &str) -> crate::Result<()> {
        match &self.backend {
            Driver::Shm(backend) => backend.unlink(id).map_err(Into::into),
            Driver::PlainIvshmem(backend) => backend.unlink(id),
        }
    }
}

pub(super) enum File {
    Shm(::shm::backend::File),
    PlainIvshmem(plain_ivshmem::File),
}

impl File {
    pub(super) fn is_create(&self) -> bool {
        match self {
            File::Shm(file) => file.is_create(),
            File::PlainIvshmem(file) => file.is_create(),
        }
    }

    pub(super) unsafe fn map(
        self,
        address: Option<NonNull<Page>>,
        numa: Option<Numa>,
        populate: Option<Populate>,
    ) -> crate::Result<NonNull<Page>> {
        match self {
            File::Shm(file) => unsafe {
                file.map()
                    .maybe_address(address)
                    .maybe_numa(numa)
                    .maybe_populate(populate)
                    .call()
                    .map_err(Into::into)
            },
            File::PlainIvshmem(file) => unsafe { file.map(address, numa, populate) },
        }
    }
}
