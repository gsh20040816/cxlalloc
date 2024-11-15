use std::io;
use std::sync::Mutex;

use crate::huge;
use crate::raw;
use crate::raw::backend;
use crate::raw::region::RESERVATION;
use crate::raw::Backend;
use crate::raw::Region;
use crate::region;
use crate::thread;
use crate::Allocator;
use crate::SIZE_SLAB;

pub type Heap = Inner;

/// This type represents sole ownership of an initialized backing store
/// for the heap. If heap extension is enabled, we need to use an
/// [`std::sync::Arc`] internally to pass the store to the background
/// heap extension thread, but the public interface remains the same.
pub struct Inner {
    pub(crate) backend: Backend,
    pub(crate) shared: Region,
    pub(crate) owned: Region,
    pub(crate) data: Region,

    /// Initial capacity
    pub(crate) capacity: u32,

    /// The process identifier and count are used to coordinate
    /// between heap extension threads, which must mmap exactly
    /// once per process as opposed to once per thread.
    pub(crate) process_id: usize,
    pub(crate) process_count: usize,

    pub(crate) state: Mutex<huge::Dram>,

    /// Free on drop
    free: bool,
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
            free,
        }: Builder,
    ) -> io::Result<Heap> {
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

        let slab_count = size.next_multiple_of(SIZE_SLAB) / SIZE_SLAB;

        // TODO: If heap extension is enabled, ensure that the shared and owned
        // region will be page size aligned, so we can mmap new regions
        // with MAP_FIXED at contiguous addresses.
        let shared_layout = region::Shared::layout(slab_count);
        let shared = backend.allocate(
            format!("{id}-shared"),
            None,
            shared_layout.size(),
            raw::region::RESERVATION,
        )?;

        let owned_layout = region::Owned::layout(slab_count);
        let owned = backend.allocate(
            format!("{id}-owned"),
            None,
            owned_layout.size(),
            raw::region::RESERVATION,
        )?;

        let data_layout = region::Data::layout(slab_count);
        let data = backend.allocate(
            format!("{id}-data"),
            None,
            data_layout.size(),
            raw::region::RESERVATION * 2,
        )?;

        // Note: not calling `munmap` here and mapping
        // with `MAP_FIXED` in the huge allocator seems to
        // cause a SEGFAULT, even though it seems equivalent
        // to unmapping here and mapping with `MAP_FIXED_NOREPLACE`?
        //
        // In particular, the no-replace mappings aren't failing...
        unsafe {
            libc::munmap(
                data.base()
                    .as_ptr()
                    .cast::<libc::c_void>()
                    .wrapping_byte_add(raw::region::RESERVATION),
                RESERVATION,
            );
        }

        log::info!(
            "Constructing heap with aligned size {}",
            slab_count * SIZE_SLAB
        );

        let raw = Self {
            backend,
            shared,
            owned,
            data,
            capacity: slab_count.try_into().unwrap(),
            state: Mutex::default(),
            process_id,
            process_count,
            free,
        };

        if !raw.is_clean() {
            raw.heap().replay_log(true);
        }

        Ok(raw)
    }

    pub fn allocator(&self, id: thread::Id) -> Allocator {
        // TODO: safety?
        unsafe { Allocator::from_raw(self, id) }
    }

    pub fn heap(&self) -> crate::Heap {
        // TODO: safety?
        unsafe { crate::Heap::from_raw(self) }
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

impl Drop for Inner {
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
