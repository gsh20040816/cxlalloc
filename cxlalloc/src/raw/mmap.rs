use core::ffi;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use std::io;

use crate::raw;
use crate::raw::backend;
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
        address: Option<NonNull<ffi::c_void>>,
        size: usize,
        reserved: Option<NonZeroUsize>,
    ) -> io::Result<Region> {
        Region::new(id, address, size, reserved, None)
    }

    fn extend(&self, region: &Region) -> io::Result<()> {
        let epoch = region.advance_epoch();
        let (address, size, _) = region.epoch_to_metadata(epoch);
        region.extend(address, size, None).map(drop)
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
