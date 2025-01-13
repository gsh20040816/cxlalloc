pub mod backend;
mod builder;
pub(crate) mod region;

pub use backend::Backend;
pub use builder::Builder;
pub(crate) use region::Region;

use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::ptr;
use core::ptr::NonNull;
use std::io;

use crate::raw::region::RESERVATION;
use crate::size;
use crate::slab;
use crate::thread;
use crate::view;

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

/// Compute size and offsets for a sequence of types in memory.
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
            backend.name(),
            size,
            thread_count,
            process_id,
            process_count,
        );

        let slab_count = size.next_multiple_of(crate::SIZE_SLAB) / crate::SIZE_SLAB;

        let (shared_size, _) = Self::shared();
        // FIXME: support extension for huge allocation region?
        let shared = backend.allocate(format!("{id}-shared"), None, shared_size, None)?;

        let (owned_size, _) = Self::owned();
        let owned = backend.allocate(format!("{id}-owned"), None, owned_size, None)?;

        let slab_small_size = view::Slab::<size::Small>::layout(slab_count)
            .unwrap()
            .size();
        let slab_small =
            backend.allocate(format!("{id}-ss"), None, slab_small_size, Some(RESERVATION))?;

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

        let data_small_size = view::Data::<size::Small>::layout(slab_count)
            .unwrap()
            .size();
        let data_small = backend.allocate(
            format!("{id}-ds"),
            NonNull::new(address),
            data_small_size,
            None,
        )?;

        let data_huge = backend.allocate(
            format!("{id}-dh"),
            NonNull::new(address.wrapping_byte_add(RESERVATION.get())),
            0,
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

    pub fn allocator(&self, id: thread::Id) -> crate::Allocator {
        let (_, shared_offsets) = Self::shared();
        let (_, owned_offsets) = Self::owned();
        let shared = self.shared.base().as_ptr();
        let owned = self.owned.base().as_ptr();
        unsafe {
            // Several issues here:
            // - Calls layout code at runtime. Ideally the layout information could be
            //   a const, but some APIs (Layout::extend, Layout::pad_to_align) aren't
            //   const yet.
            // - Offsets aren't statically checked to match their memory regions.
            // - Indexes into offset arrays aren't statically checked to match their struct type.
            // - This module maybe shouldn't need to know about `thread::Array<UnsafeCell<...>>`?
            view::Allocator::new(
                id,
                shared
                    .wrapping_byte_add(shared_offsets[0])
                    .cast::<view::allocator::Shared>()
                    .as_ref()
                    .unwrap(),
                owned
                    .wrapping_byte_add(owned_offsets[0])
                    .cast::<thread::Array<UnsafeCell<view::allocator::Owned>>>()
                    .as_ref()
                    .unwrap(),
                view::Heap::<view::Unfocus, size::Small>::new(
                    self.capacity,
                    shared
                        .wrapping_byte_add(shared_offsets[1])
                        .cast::<view::heap::Shared<size::Small>>()
                        .as_ref()
                        .unwrap(),
                    owned
                        .wrapping_byte_add(owned_offsets[1])
                        .cast::<thread::Array<UnsafeCell<view::heap::Owned<size::Small>>>>()
                        .as_ref()
                        .unwrap(),
                    view::Slab::new(slab::Slice::from_raw(self.slab_small.base().cast())),
                    view::Data::<size::Small>::new(self.data_small.base()),
                ),
                view::Huge::new(
                    shared
                        .wrapping_byte_add(shared_offsets[2])
                        .cast::<view::huge::Shared>()
                        .as_ref()
                        .unwrap(),
                    owned
                        .wrapping_byte_add(owned_offsets[2])
                        .cast::<thread::Array<view::huge::Owned>>()
                        .as_ref()
                        .unwrap(),
                    view::Data::<size::Huge>::new(self.data_huge.base()),
                ),
            )
            .focus(id)
            .into()
        }
    }

    pub fn is_clean(&self) -> bool {
        self.regions().any(Region::is_clean)
    }

    fn shared() -> (usize, Vec<usize>) {
        layout!(
            view::allocator::Shared,
            view::heap::Shared<size::Small>,
            view::huge::Shared,
        )
    }

    fn owned() -> (usize, Vec<usize>) {
        layout!(
            thread::Array<UnsafeCell<view::allocator::Owned>>,
            thread::Array<UnsafeCell<view::heap::Owned<size::Small>>>,
            thread::Array<view::huge::Owned>,
        )
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
