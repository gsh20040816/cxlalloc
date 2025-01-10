use std::io;

use crate::raw::backend;
use crate::raw::Backend;
use crate::raw::Region;
use crate::SIZE_SLAB;

/// This type represents sole ownership of an initialized backing store
/// for the heap.
pub struct Raw {
    pub(crate) backend: Backend,

    pub(crate) hwcc: Region,
    pub(crate) swcc: Region,

    // Slab metadata regions
    pub(crate) small_slab: Region,

    // Data regions, must be contiguous
    pub(crate) small_data: Region,
    pub(crate) huge_data: Region,

    /// Initial capacity
    pub(crate) capacity: u32,

    /// The process identifier and count are used to coordinate
    /// between heap extension threads, which must mmap exactly
    /// once per process as opposed to once per thread.
    pub(crate) process_id: usize,
    pub(crate) process_count: usize,

    /// Free on drop
    free: bool,
}

/// # Safety
///
/// The memory regions are mapped for the entire process, so
/// the pointers remain valid when transferred to a different thread.
unsafe impl Send for Raw {}

/// # Safety
///
/// The only (public) way to interact with a [`Raw`] is through
/// a [`crate::Heap`] or [`crate::Allocator`], which expose
/// thread-safe methods.
unsafe impl Sync for Raw {}

impl Raw {
    fn new(
        id: &str,
        Builder {
            backend,
            size,
            thread_count,
            process_id,
            process_count,
            free,
        }: Builder,
    ) -> io::Result<Raw> {
        log::info!(
            "Requesting heap with \
            backend = {}, \
            size = {}, \
            thread_count = {}, \
            process_id = {}, \
            process_count = {}",
            backend.as_backend().name(),
            size,
            thread_count,
            process_id,
            process_count,
        );

        // memory layout
        //
        // HWcc
        // +------------------+------------+-------------------+-----------+------------+
        // | persistent root  | help array | small global free | huge next | huge slots |
        // +------------------+------------+-------------------+-----------+------------+
        //
        // SWcc
        //
        todo!()
    }

    pub fn is_clean(&self) -> bool {
        // Can only be dirty if all regions were created successfully before
        self.owned.is_clean() || self.shared.is_clean() || self.data.is_clean()
    }

    #[allow(unused)]
    pub(crate) fn extend(&self) -> io::Result<()> {
        self.backend.extend(&self.owned)?;
        self.backend.extend(&self.shared)?;
        self.backend.extend(&self.data)?;
        Ok(())
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        if let Err(error) = self.backend.unmap(&self.shared) {
            log::error!("Failed to unmap shared region: {:?}", error);
        }

        if let Err(error) = self.backend.unmap(&self.owned) {
            log::error!("Failed to unmap owned region: {:?}", error);
        }

        if let Err(error) = self.backend.unmap(&self.data) {
            log::error!("Failed to unmap data region: {:?}", error);
        }

        if !self.free {
            return;
        }

        if let Err(error) = self.backend.free(&self.shared) {
            log::error!("Failed to free shared region: {:?}", error);
        }

        if let Err(error) = self.backend.free(&self.owned) {
            log::error!("Failed to free owned region: {:?}", error);
        }

        if let Err(error) = self.backend.free(&self.data) {
            log::error!("Failed to free data region: {:?}", error);
        }
    }
}

pub struct Builder {
    backend: Backend,
    size: usize,
    thread_count: usize,
    process_id: usize,
    process_count: usize,
    free: bool,
}

impl Builder {
    pub fn build(self, id: &str) -> io::Result<Raw> {
        Raw::new(id, self)
    }

    pub fn backend<B: Into<Backend>>(mut self, backend: B) -> Self {
        self.backend = backend.into();
        self
    }

    pub fn size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    pub fn thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    pub fn process_id(mut self, process_id: usize) -> Self {
        self.process_id = process_id;
        self
    }

    pub fn process_count(mut self, process_count: usize) -> Self {
        self.process_count = process_count;
        self
    }

    pub fn free(mut self, free: bool) -> Self {
        self.free = free;
        self
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            backend: Backend::Mmap(backend::Mmap),
            size: 64 * SIZE_SLAB,
            thread_count: 1,
            process_id: 0,
            process_count: 1,
            free: false,
        }
    }
}
