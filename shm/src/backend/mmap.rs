use core::ffi::CStr;
use core::num::NonZeroUsize;

use crate::backend;

#[derive(Clone, Debug, Default)]
pub struct Mmap;

impl backend::Interface for Mmap {
    fn name(&self) -> &'static str {
        "mmap"
    }

    fn open(&self, _: &CStr, _: NonZeroUsize) -> crate::Result<backend::File> {
        Ok(backend::File::new(None, 0, true))
    }

    fn unlink(&self, _id: &CStr) -> crate::Result<()> {
        Ok(())
    }
}

impl From<Mmap> for backend::Concrete {
    fn from(mmap: Mmap) -> Self {
        backend::Concrete::Mmap(mmap)
    }
}
