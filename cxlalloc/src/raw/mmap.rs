use core::num::NonZeroUsize;
use std::io;

use crate::raw::backend;
use crate::raw::region;

#[derive(Clone, Debug, Default)]
pub struct Mmap;

impl backend::Impl for Mmap {
    fn name(&self) -> &'static str {
        "mmap"
    }

    fn allocate(&self, _: region::Id, _: NonZeroUsize) -> io::Result<backend::File> {
        Ok(backend::File::default())
    }
}

impl From<Mmap> for backend::Kind {
    fn from(mmap: Mmap) -> Self {
        backend::Kind::Mmap(mmap)
    }
}
