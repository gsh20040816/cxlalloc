use core::alloc::Layout;
use core::ptr;
use core::ptr::NonNull;
use std::io;

use crate::raw::backend;
use crate::raw::region::RESERVATION;
use crate::raw::Backend;
use crate::raw::Region;
use crate::size;
use crate::view;
use crate::SIZE_SLAB;

/// This type represents sole ownership of an initialized backing store
/// for the heap.
pub struct Raw {
    pub(crate) backend: Backend,

    // - Global persistent root: 1
    // - Help array: # threads
    // - Small heap
    //   - Global stack: 1
    //   - Bump pointer: 1
    // - Huge heap
    //   - Next slot: 1
    //   - Slot array: # huge allocations (extend)
    pub(crate) shared: Region,

    // - Local persistent roots: # threads
    // - Small heap
    //   - Unsized free list: # threads
    //   - Sized free lists: # sizes * # threads
    // - Huge heap
    //   - Descriptor lists: # threads
    pub(crate) owned: Region,

    // Slab metadata regions
    pub(crate) slab_small: Region,

    // Data regions, must be contiguous
    pub(crate) data_small: Region,
    pub(crate) data_huge: Region,

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

        let slab_count = size.next_multiple_of(crate::SIZE_SLAB) / crate::SIZE_SLAB;

        macro_rules! layout {
            ($head:ty $(, $tail:ty)* $(,)?) => {
                {
                    let mut offsets = vec![0];
                    let mut layout = Layout::new::<$head>();
                    for field in [$(Layout::new::<$tail>()),*] {
                        let (next, offset) = layout.extend(field).unwrap();
                        layout = next;
                        offsets.push(offset);
                    }
                    (layout.pad_to_align().size(), offsets)
                }
            };
        }

        let (shared_size, _) = layout!(
            view::allocator::Shared,
            view::heap::Shared<size::Small>,
            view::huge::Shared,
        );

        // FIXME: support extension for huge allocation region?
        let shared = backend.allocate(String::from("shared"), None, shared_size, None)?;

        let (owned_size, _) = layout!(
            view::allocator::Owned,
            view::heap::Owned<size::Small>,
            view::huge::Owned,
        );

        let owned = backend.allocate(String::from("owned"), None, owned_size, None)?;

        let slab_small_size = view::Slab::<size::Small>::layout(slab_count)
            .unwrap()
            .size();
        let slab_small =
            backend.allocate(String::from("ss"), None, slab_small_size, Some(RESERVATION))?;

        // This is hacky, but data regions must be contiguous to support
        // applications that rely on offset pointers.
        let address = match unsafe {
            libc::mmap64(
                ptr::null_mut(),
                RESERVATION.get() * 2,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        } {
            libc::MAP_FAILED => return Err(io::Error::last_os_error()),
            address => address,
        };

        let data_small_size = view::Data::layout(slab_count).unwrap().size();
        let data_small = backend.allocate(
            String::from("ds"),
            NonNull::new(address),
            data_small_size,
            None,
        )?;

        let data_huge = backend.allocate(
            String::from("dh"),
            NonNull::new(address.wrapping_byte_add(RESERVATION.get())),
            // FIXME: struct for this?
            RESERVATION.get(),
            None,
        )?;

        Ok(Self {
            backend,
            shared,
            owned,
            slab_small,
            data_small,
            data_huge,
            capacity: slab_count as u32,
            process_id,
            process_count,
            free,
        })
    }

    pub fn is_clean(&self) -> bool {
        self.regions().any(Region::is_clean)
    }

    fn regions(&self) -> impl Iterator<Item = &Region> {
        [
            &self.shared,
            &self.owned,
            &self.slab_small,
            &self.data_small,
            &self.data_huge,
        ]
        .into_iter()
    }

    #[allow(unused)]
    pub(crate) fn extend(&self) -> io::Result<()> {
        todo!()
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        self.regions().for_each(|region| match region.unmap() {
            Ok(()) => (),
            Err(error) => log::error!("Failed to unmap {} region: {:?}", region.id(), error),
        });

        if !self.free {
            return;
        }

        self.regions()
            .for_each(|region| match self.backend.free(region) {
                Ok(()) => (),
                Err(error) => log::error!("Failed to free {} region: {:?}", region.id(), error),
            });
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
