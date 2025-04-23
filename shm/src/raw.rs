use core::num::NonZeroUsize;
use core::ptr::NonNull;
use std::ffi;
use std::ffi::CString;

use bon::bon;

use crate::Numa;
use crate::Page;
use crate::Populate;
use crate::backend::Interface as _;

pub struct Raw {
    pub(crate) name: CString,
    pub(crate) size: NonZeroUsize,
    pub(crate) address: NonNull<Page>,
}

#[bon]
impl Raw {
    #[builder]
    pub fn new(
        name: CString,
        size: usize,
        #[builder(default)] create: bool,
        numa: Option<Numa>,
        populate: Option<Populate>,
    ) -> crate::Result<Self> {
        let backend = crate::Backend::Shm(crate::backend::Shm);

        if create {
            match backend.unlink(&name) {
                Ok(()) => log::info!("Unlinked stale shm object: {}", name.to_string_lossy()),
                Err(error) if error.is_not_found() => (),
                Err(error) => return Err(error),
            }
        }

        let size = NonZeroUsize::new(size).unwrap();
        let file = backend.open(&name, size)?;
        let address = unsafe {
            file.map()
                .maybe_numa(numa)
                .maybe_populate(populate)
                .call()?
        };

        Ok(Self {
            name,
            size,
            address,
        })
    }
}

impl Raw {
    pub fn address(&self) -> NonNull<Page> {
        self.address
    }

    pub fn size(&self) -> NonZeroUsize {
        self.size
    }

    pub fn unlink(&mut self) -> crate::Result<()> {
        crate::backend::Shm.unlink(&self.name)
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        if let Err(error) = unsafe {
            crate::try_libc!(libc::munmap(
                self.address.as_ptr().cast::<ffi::c_void>(),
                self.size.get()
            ))
        } {
            panic!(
                "Failed to munmap {:#x?} ({:#x}): {:?}",
                self.address, self.size, error
            );
        }
    }
}
