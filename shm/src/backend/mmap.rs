use core::ffi::CStr;
use core::num::NonZeroUsize;

use crate::Page;
use crate::backend;

#[derive(Clone, Debug, Default)]
pub struct Mmap;

impl backend::Interface for Mmap {
    fn name(&self) -> &'static str {
        "mmap"
    }

    fn open(&self, _: &CStr, size: NonZeroUsize) -> crate::Result<backend::File> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(Page::SIZE)).unwrap();
        Ok(backend::File::builder()
            .size(size)
            .offset(0)
            .create(true)
            .build())
    }

    fn unlink(&self, _id: &CStr) -> crate::Result<()> {
        Ok(())
    }
}

impl From<Mmap> for backend::Backend {
    fn from(mmap: Mmap) -> Self {
        backend::Backend::Mmap(mmap)
    }
}
