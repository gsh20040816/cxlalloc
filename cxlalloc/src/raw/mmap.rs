use core::num::NonZeroUsize;
use std::io;

use crate::raw;
use crate::raw::backend;

#[derive(Clone, Debug, Default)]
pub struct Mmap;

impl backend::Impl for Mmap {
    fn name(&self) -> &'static str {
        "mmap"
    }

    fn allocate(&self, _: String, _: NonZeroUsize) -> io::Result<backend::File> {
        Ok(backend::File::default())
    }

    fn free(&self, _: String) -> io::Result<()> {
        Ok(())
    }
}

impl From<Mmap> for raw::Backend {
    fn from(mmap: Mmap) -> Self {
        raw::Backend::Mmap(mmap)
    }
}
