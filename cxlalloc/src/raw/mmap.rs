use std::io;

use crate::raw;
use crate::raw::backend::Backend;
use crate::raw::region;
use crate::raw::Region;

#[derive(Clone, Debug, Default)]
pub struct Mmap;

impl Backend for Mmap {
    fn allocate(&self, id: region::Id, size: usize) -> io::Result<Region> {
        Region::new(id, size, None)
    }

    fn extend(&self, region: &Region) -> io::Result<()> {
        let epoch = region.advance_epoch();
        let (address, size, _) = region.epoch_to_metadata(epoch);
        region.extend(address, size, None).map(drop)
    }

    fn free(&self, region: &Region) -> io::Result<()> {
        region.unmap()
    }
}

impl From<Mmap> for raw::Backend {
    fn from(mmap: Mmap) -> Self {
        raw::Backend::Mmap(mmap)
    }
}
