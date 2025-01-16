pub mod backend;
mod builder;
pub(crate) mod region;

pub use backend::Backend;
pub use builder::Builder;
pub(crate) use region::Page;
use region::Region;
pub(crate) use region::Reservation;

use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::ffi;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use std::io;

use crate::allocator;
use crate::heap;
use crate::huge;
use crate::size;
use crate::slab;
use crate::thread;
use crate::view;
use crate::Allocator;
use crate::Data;
use crate::Heap;
use crate::Huge;
use crate::Slab;

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
    pub(crate) shared: region::Fixed,

    // - Local persistent roots: # threads
    // - Small heap
    //   - Unsized free list: # threads
    //   - Sized free lists: # sizes * # threads
    // - Huge heap
    //   - Descriptor lists: # threads
    pub(crate) owned: region::Fixed,

    // Slab metadata regions
    pub(crate) slab_small: region::Sequential,

    // Data regions, must be contiguous
    pub(crate) data_small: region::Sequential,
    pub(crate) data_huge: region::Random,

    /// Initial capacity
    pub(crate) capacity: u32,

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
            (NonZeroUsize::new(layout.pad_to_align().size()).unwrap(), offsets)
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
            free,
        }: Builder,
    ) -> io::Result<Raw> {
        log::info!(
            "Requesting heap with \
            backend = {}, \
            size = {}, \
            thread_count = {}",
            backend.name(),
            size,
            thread_count,
        );

        let slab_count = size.next_multiple_of(crate::SIZE_SLAB) / crate::SIZE_SLAB;
        let id = region::Id::new(id);

        let (shared_size, _) = Self::shared();
        // FIXME: support extension for huge allocation region?
        let shared = region::Fixed::new(&backend, id.with_suffix("shared"), shared_size)?;

        let (owned_size, _) = Self::owned();
        let owned = region::Fixed::new(&backend, id.with_suffix("owned"), owned_size)?;

        let slab_small_size = Slab::<size::Small>::layout(slab_count)
            .ok()
            .map(|layout| layout.size())
            .and_then(NonZeroUsize::new)
            .unwrap();
        let slab_small_reservation = Reservation::new(Reservation::TIB)?;
        let slab_small = region::Sequential::new(
            &backend,
            id.with_suffix("ss"),
            slab_small_reservation,
            slab_small_size,
        )?;

        // Data regions must be contiguous to support applications that rely on offset pointers.
        let data_reservation =
            Reservation::new(Reservation::TIB.saturating_add(Reservation::TIB.get()))?;

        let (data_small_reservation, data_huge_reservation) =
            data_reservation.split(Reservation::TIB);

        let data_small_size = Data::<size::Small>::layout(slab_count)
            .ok()
            .map(|layout| layout.size())
            .and_then(NonZeroUsize::new)
            .unwrap();
        let data_small = region::Sequential::new(
            &backend,
            id.with_suffix("ds"),
            data_small_reservation,
            data_small_size,
        )?;

        let data_huge = region::Random::new(id.with_suffix("dh"), data_huge_reservation)?;

        Ok(Self {
            backend,
            shared,
            owned,
            slab_small,
            data_small,
            data_huge,
            capacity: slab_count as u32,
            free,
        })
    }

    pub fn allocator<S, O>(&self, id: thread::Id) -> Allocator<S, O> {
        unsafe { Allocator::new(self.unfocused().focus(id)) }
    }

    pub fn map(&self, address: *mut ffi::c_void) -> bool {
        let Some(address) = NonNull::new(address) else {
            return false;
        };

        let allocator = self.unfocused::<(), ()>();

        if let Some(offset) = allocator.huge.data.checked_pointer_to_offset(address) {
            allocator.huge.map_offset(&allocator.small.data, offset);
            true
        } else if let Some(offset) = allocator.small.data.checked_pointer_to_offset(address) {
            allocator
                .small
                .map_offset(&self.backend, &self.slab_small, &self.data_small, offset)
                .unwrap();
            true
        } else {
            false
        }
    }

    fn unfocused<S, O>(&self) -> allocator::Allocator<view::Unfocus, S, O> {
        let (_, shared_offsets) = Self::shared();
        let (_, owned_offsets) = Self::owned();
        let shared = self.shared.address().as_ptr();
        let owned = self.owned.address().as_ptr();
        unsafe {
            // Several issues here:
            // - Calls layout code at runtime. Ideally the layout information could be
            //   a const, but some APIs (Layout::extend, Layout::pad_to_align) aren't
            //   const yet.
            // - Offsets aren't statically checked to match their memory regions.
            // - Indexes into offset arrays aren't statically checked to match their struct type.
            // - This module maybe shouldn't need to know about `thread::Array<UnsafeCell<...>>`?
            allocator::Allocator::new(
                (),
                shared
                    .wrapping_byte_add(shared_offsets[0])
                    .cast::<allocator::Shared<S>>()
                    .as_ref()
                    .unwrap(),
                owned
                    .wrapping_byte_add(owned_offsets[0])
                    .cast::<thread::Array<UnsafeCell<allocator::Owned<O>>>>()
                    .as_ref()
                    .unwrap(),
                Heap::<view::Unfocus, size::Small>::new(
                    self.capacity,
                    shared
                        .wrapping_byte_add(shared_offsets[1])
                        .cast::<heap::Shared<size::Small>>()
                        .as_ref()
                        .unwrap(),
                    owned
                        .wrapping_byte_add(owned_offsets[1])
                        .cast::<thread::Array<UnsafeCell<heap::Owned<size::Small>>>>()
                        .as_ref()
                        .unwrap(),
                    Slab::new(slab::Slice::from_raw(self.slab_small.address().cast())),
                    Data::<size::Small>::new(self.data_small.address()),
                ),
                Huge::new(
                    &self.backend,
                    &self.data_huge,
                    shared
                        .wrapping_byte_add(shared_offsets[2])
                        .cast::<huge::Shared>()
                        .as_ref()
                        .unwrap(),
                    owned
                        .wrapping_byte_add(owned_offsets[2])
                        .cast::<thread::Array<huge::Owned>>()
                        .as_ref()
                        .unwrap(),
                    Data::<size::Huge>::new(self.data_huge.address()),
                ),
            )
        }
    }

    pub fn is_clean(&self) -> bool {
        self.regions().any(Region::is_clean)
    }

    fn shared() -> (NonZeroUsize, Vec<usize>) {
        layout!(
            allocator::Shared<()>,
            heap::Shared<size::Small>,
            huge::Shared,
        )
    }

    fn owned() -> (NonZeroUsize, Vec<usize>) {
        layout!(
            thread::Array<UnsafeCell<allocator::Owned<()>>>,
            thread::Array<UnsafeCell<heap::Owned<size::Small>>>,
            thread::Array<huge::Owned>,
        )
    }

    fn regions(&self) -> impl Iterator<Item = &dyn Region> {
        [
            &self.shared as &dyn Region,
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

        todo!()
    }
}
