use std::io;

use crate::raw;
use crate::raw::backend;
use crate::raw::Backend;
use crate::raw::Region;
use crate::region;
use crate::thread;
use crate::Allocator;
use crate::SIZE_SLAB;

#[cfg(not(feature = "operation-expand"))]
pub type Heap = Inner;

#[cfg(feature = "operation-expand")]
pub type Heap = std::sync::Arc<Inner>;

/// This type represents sole ownership of an initialized backing store
/// for the heap. If heap expansion is enabled, we need to use an
/// [`std::sync::Arc`] internally to pass the store to the background
/// heap expansion thread, but the public interface remains the same.
pub struct Inner {
    pub(crate) backend: Backend,
    pub(crate) shared: Region,
    pub(crate) owned: Region,
    pub(crate) data: Region,

    /// Initial capacity
    pub(crate) capacity: usize,

    /// The process identifier and count are used to coordinate
    /// between heap expansion threads, which must mmap exactly
    /// once per process as opposed to once per thread.
    #[cfg_attr(not(feature = "operation-expand"), allow(dead_code))]
    process_id: usize,

    #[cfg_attr(not(feature = "operation-expand"), allow(dead_code))]
    process_count: usize,
}

/// # Safety
///
/// The memory regions are mapped for the entire process, so
/// the pointers remain valid when transferred to a different thread.
unsafe impl Send for Inner {}

/// # Safety
///
/// The only (public) way to interact with a [`Raw`] is through
/// a [`crate::Heap`] or [`crate::Allocator`], which expose
/// thread-safe methods.
unsafe impl Sync for Inner {}

impl Inner {
    fn new(
        id: &str,
        Builder {
            backend,
            size,
            thread_count,
            process_id,
            process_count,
        }: Builder,
    ) -> io::Result<Heap> {
        log::info!(
            "Requesting heap with \
            backend = {:?}, \
            size = {}, \
            thread_count = {}, \
            process_id = {}, \
            process_count = {}",
            backend,
            size,
            thread_count,
            process_id,
            process_count,
        );

        let id = raw::region::Id::new(id);
        let slab_count = size.next_multiple_of(SIZE_SLAB) / SIZE_SLAB;

        // TODO: If heap expansion is enabled, ensure that the shared and owned
        // region will be page size aligned, so we can mmap new regions
        // with MAP_FIXED at contiguous addresses.
        let shared_layout = region::meta::Shared::layout(slab_count);
        let shared = backend.allocate(id.with_suffix("shared"), shared_layout.size())?;

        let owned_layout = region::meta::Owned::layout(slab_count);
        let owned = backend.allocate(id.with_suffix("owned"), owned_layout.size())?;

        let data_layout = region::Data::layout(slab_count);
        let data = backend.allocate(id.with_suffix("slab"), data_layout.size())?;

        log::info!(
            "Constructing heap with aligned size {}",
            slab_count * SIZE_SLAB
        );

        Ok(Self {
            backend,
            shared,
            owned,
            data,
            capacity: slab_count,
            process_id,
            process_count,
        }
        .into())
    }

    pub fn allocator(&self, id: thread::Id) -> Allocator {
        // TODO: safety?
        unsafe { Allocator::from_raw(self, id) }
    }

    pub fn heap(&self) -> crate::Heap {
        // TODO: safety?
        unsafe { crate::Heap::from_raw(self) }
    }
}

#[cfg(feature = "operation-expand")]
impl Inner {
    pub(crate) fn process_id(&self) -> usize {
        self.process_id
    }

    pub(crate) fn process_count(&self) -> usize {
        self.process_count
    }

    pub(crate) fn expand(&self) -> io::Result<()> {
        self.backend.expand(&self.owned)?;
        self.backend.expand(&self.shared)?;
        self.backend.expand(&self.data)?;
        Ok(())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Err(error) = self.backend.free(&self.shared) {
            log::error!("Failed to free metadata region: {:?}", error);
        }

        if let Err(error) = self.backend.free(&self.owned) {
            log::error!("Failed to free descriptor region: {:?}", error);
        }

        if let Err(error) = self.backend.free(&self.data) {
            log::error!("Failed to free slab region: {:?}", error);
        }
    }
}

pub struct Builder {
    backend: Backend,
    size: usize,
    thread_count: usize,
    process_id: usize,
    process_count: usize,
}

impl Builder {
    pub fn build(self, id: &str) -> io::Result<Heap> {
        Inner::new(id, self)
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
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            backend: Backend::Mmap(backend::Mmap),
            size: 64 * SIZE_SLAB,
            thread_count: 1,
            process_id: 0,
            process_count: 1,
        }
    }
}
