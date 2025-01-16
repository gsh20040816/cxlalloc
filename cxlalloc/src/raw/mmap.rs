use core::num::NonZeroUsize;
use std::io;

use crate::raw;
use crate::raw::backend;
use crate::raw::region::Reservation;
use crate::raw::Region;

#[derive(Clone, Debug, Default)]
pub struct Mmap;

impl backend::Impl for Mmap {
    fn name(&self) -> &'static str {
        "mmap"
    }

    fn allocate(
        &self,
        id: String,
        reservation: Option<Reservation>,
        size: usize,
    ) -> io::Result<Region> {
        Region::new(id, backend::File::default(), reservation, size)
    }

    fn map(&self, region: &Region, offset: usize, size: NonZeroUsize) -> io::Result<()> {
        region.map(backend::File::default(), offset, size)
    }

    fn unmap(&self, region: &Region) -> io::Result<()> {
        region.unmap()
    }

    fn free(&self, _: &Region) -> io::Result<()> {
        Ok(())
    }
}

impl From<Mmap> for raw::Backend {
    fn from(mmap: Mmap) -> Self {
        raw::Backend::Mmap(mmap)
    }
}
