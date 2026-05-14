pub mod backend;
pub(crate) mod region;

pub use backend::Backend;

pub use raw_builder::State as BuilderState;
pub(crate) use region::Page;
use region::Region;
pub(crate) use region::Reservation;
pub use RawBuilder as Builder;

use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::ffi;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use std::ffi::CString;
use std::fs;
use std::io;

use bon::bon;

use crate::allocator;
use crate::heap;
use crate::huge;
use crate::size;
use crate::size::Bracket;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::Allocator;
use crate::Data;
use crate::Heap;
use crate::Huge;
use crate::RESERVE_LARGE_SLABS;
use crate::Slab;
use crate::BATCH_BUMP_POP;
use crate::BATCH_GLOBAL_PUSH;
use crate::COUNT_CACHE_SLAB;
use crate::RESERVE_SMALL_SLABS;

/// This type represents sole ownership of an initialized backing store
/// for the heap.
pub struct Raw {
    pub(crate) backend: Backend,

    // - Global persistent root: 1
    // - Help array: # threads
    // - Small and large heaps
    //   - Global stack: 1
    //   - Bump pointer: 1
    // - Huge heap
    //   - Next slot: 1
    //   - Slot array: # huge allocations (extend)
    pub(crate) shared: region::Fixed,

    // - Local persistent roots: # threads
    // - Small and large heaps
    //   - Unsized free list: # threads
    //   - Sized free lists: # sizes * # threads
    // - Huge heap
    //   - Descriptor lists: # threads
    pub(crate) owned: region::Fixed,

    // Slab metadata regions
    pub(crate) local_small: region::Sequential,
    pub(crate) local_large: region::Sequential,
    pub(crate) remote_small: region::Sequential,
    pub(crate) remote_large: region::Sequential,

    // Data regions, must be contiguous
    pub(crate) data_small: region::Sequential,
    pub(crate) data_large: region::Sequential,
    pub(crate) data_huge: region::Random,

    small_slab_capacity: u32,
    large_slab_capacity: u32,

    stat: stat::process::Recorder,

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

#[bon]
impl Raw {
    #[builder]
    pub fn new(
        #[builder(finish_fn)] id: &str,
        #[builder(default, into)] backend: Backend,
        #[builder(default)] size_small: usize,
        #[builder(default)] size_large: usize,
        #[builder(default)] reserve_large: usize,
        #[builder(default = 1)] thread_count: usize,
        #[builder(default)] free: bool,
        fixed_base: Option<usize>,
        cache_local: Option<usize>,
        batch_global: Option<usize>,
        batch_bump: Option<usize>,
    ) -> crate::Result<Raw> {
        log::info!(
            "Requesting heap with \
            backend = {}, \
            size_small = {}, \
            size_large = {}, \
            thread_count = {}",
            backend.name(),
            size_small,
            size_large,
            thread_count,
        );

        if let Some(cache_local) = cache_local {
            COUNT_CACHE_SLAB.store(cache_local, Ordering::Relaxed);
        }

        if let Some(batch_global) = batch_global {
            BATCH_GLOBAL_PUSH.store(batch_global, Ordering::Relaxed);
        }

        if let Some(batch_bump) = batch_bump {
            BATCH_BUMP_POP.store(batch_bump, Ordering::Relaxed);
        }
        RESERVE_LARGE_SLABS.store(
            reserve_large.next_multiple_of(size::Large::SIZE_SLAB) / size::Large::SIZE_SLAB,
            Ordering::Relaxed,
        );
        RESERVE_SMALL_SLABS.store(
            reserve_large.next_multiple_of(size::Small::SIZE_SLAB) / size::Small::SIZE_SLAB,
            Ordering::Relaxed,
        );

        let id = region::Id::new(id);
        let fixed_base = fixed_base
            .map(|base| {
                NonNull::new(base as *mut region::Page).ok_or(crate::Error::Mmap(
                    std::io::Error::from_raw_os_error(libc::EINVAL),
                ))
            })
            .transpose()?;
        if let Some(base) = fixed_base {
            region::reserve_address_space(base, region::Reservation::SIZE.get() * 9)?;
        }
        let reservation_at = |index: usize, size: NonZeroUsize| {
            fixed_base.map(|base| unsafe {
                region::fixed_reservation(
                    base.byte_add(region::Reservation::SIZE.get() * index),
                    size,
                )
            })
        };

        // FIXME: support extension for huge allocation region?
        let (shared_size, _) = Self::shared();

        #[cfg(feature = "cxl-mcas")]
        let shared = region::Fixed::new_mcas(id.with_suffix("s"), shared_size)?;
        #[cfg(not(feature = "cxl-mcas"))]
        let shared = match reservation_at(0, region::Reservation::SIZE) {
            Some(reservation) => {
                region::Fixed::new_at(&backend, id.with_suffix("s"), shared_size, reservation.start())?
            }
            None => region::Fixed::new(&backend, id.with_suffix("s"), shared_size)?,
        };

        let (owned_size, _) = Self::owned();
        let owned = match reservation_at(1, region::Reservation::SIZE) {
            Some(reservation) => {
                region::Fixed::new_at(&backend, id.with_suffix("o"), owned_size, reservation.start())?
            }
            None => region::Fixed::new(&backend, id.with_suffix("o"), owned_size)?,
        };

        let (small_lazy, small) = match NonZeroUsize::new(
            size_small.next_multiple_of(size::Small::SIZE_SLAB) / size::Small::SIZE_SLAB,
        )
        .map(|count| Heap::<view::Unfocus, size::Small>::layout(count).unwrap())
        {
            None => (true, Default::default()),
            Some(layout) => (false, layout),
        };
        let small_slab_capacity =
            (small.data.get() / size::Small::SIZE_SLAB).min(u32::MAX as usize) as u32;

        let local_small_reservation = match reservation_at(2, region::Reservation::SIZE) {
            Some(reservation) => reservation,
            None => Reservation::new()?,
        };
        let local_small = region::Sequential::new(
            &backend,
            id.with_suffix("ls"),
            local_small_reservation,
            small.locals,
            small_lazy,
        )?;

        #[cfg(feature = "cxl-mcas")]
        let remote_small = region::Sequential::new_mcas(id.with_suffix("rs"), small.remotes)?;
        #[cfg(not(feature = "cxl-mcas"))]
        let remote_small = region::Sequential::new(
            &backend,
            id.with_suffix("rs"),
            match reservation_at(3, region::Reservation::SIZE) {
                Some(reservation) => reservation,
                None => Reservation::new()?,
            },
            small.remotes,
            small_lazy,
        )?;

        let (large_lazy, large) = match NonZeroUsize::new(
            size_large
                .saturating_add(reserve_large)
                .next_multiple_of(size::Large::SIZE_SLAB)
                / size::Large::SIZE_SLAB,
        )
        .map(|count| Heap::<view::Unfocus, size::Large>::layout(count).unwrap())
        {
            None => (true, Default::default()),
            Some(layout) => (false, layout),
        };
        let large_slab_capacity =
            (large.data.get() / size::Large::SIZE_SLAB).min(u32::MAX as usize) as u32;

        let local_large_reservation = match reservation_at(4, region::Reservation::SIZE) {
            Some(reservation) => reservation,
            None => Reservation::new()?,
        };
        let local_large = region::Sequential::new(
            &backend,
            id.with_suffix("ll"),
            local_large_reservation,
            large.locals,
            large_lazy,
        )?;

        // FIXME: large allocations are not integrated with mCAS
        let remote_large_reservation = match reservation_at(5, region::Reservation::SIZE) {
            Some(reservation) => reservation,
            None => Reservation::new()?,
        };
        let remote_large = region::Sequential::new(
            &backend,
            id.with_suffix("rl"),
            remote_large_reservation,
            large.remotes,
            large_lazy,
        )?;

        let [data_small_reservation, data_large_reservation, data_huge_reservation] =
            match fixed_base {
                Some(_) => [
                    reservation_at(6, small.data).unwrap(),
                    reservation_at(7, large.data).unwrap(),
                    reservation_at(8, region::Reservation::SIZE).unwrap(),
                ],
                None => Reservation::new_contiguous()?,
            };

        let data_small = region::Sequential::new(
            &backend,
            id.with_suffix("ds"),
            data_small_reservation,
            small.data,
            small_lazy,
        )?;

        let data_large = region::Sequential::new(
            &backend,
            id.with_suffix("dl"),
            data_large_reservation,
            large.data,
            large_lazy,
        )?;

        let data_huge = region::Random::new(id.with_suffix("dh"), data_huge_reservation)?;

        Ok(Self {
            backend,
            shared,
            owned,
            local_small,
            local_large,
            remote_small,
            remote_large,
            data_small,
            data_large,
            data_huge,
            small_slab_capacity,
            large_slab_capacity,
            stat: stat::process::Recorder::default(),
            free,
        })
    }
}

impl Raw {
    pub fn unlink(id: &str, backend: &Backend) -> crate::Result<()> {
        if backend.name() == "shm" {
            let prefix = format!("{id}-");
            for entry in fs::read_dir("/dev/shm")? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if !name.starts_with(&prefix) {
                    continue;
                }
                let c_name = CString::new(format!("/{name}")).map_err(|error| {
                    crate::Error::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
                })?;
                unsafe {
                    match libc::shm_unlink(c_name.as_ptr()) {
                        0 => (),
                        _ if io::Error::last_os_error().kind() == io::ErrorKind::NotFound => (),
                        _ => return Err(crate::Error::ShmUnlink(io::Error::last_os_error())),
                    }
                }
            }
            return Ok(());
        }

        let id = region::Id::new(id);
        for suffix in ["s", "o"] {
            let _ = backend.unlink(id.with_suffix(suffix).as_str());
        }
        Ok(())
    }

    pub fn allocator<S, O>(&self, id: thread::Id) -> Allocator<S, O> {
        unsafe { Allocator::new(self.unfocused().focus(id, true)) }
    }

    pub fn report(&self) -> impl Iterator<Item = stat::Report> + '_ {
        self.stat.report()
    }

    pub fn map(&self, id: thread::Id, address: *mut ffi::c_void) -> bool {
        let Some(address) = NonNull::new(address) else {
            return false;
        };

        let allocator = unsafe { Allocator::<(), ()>::new(self.unfocused().focus(id, false)) };

        let context = crate::allocator::Context {
            id,
            help: &allocator.shared.help,
            owned: allocator.owned,
        };

        match allocator.small.try_map(
            &self.backend,
            &self.local_small,
            &self.remote_small,
            &self.data_small,
            &context,
            address,
        ) {
            Ok(()) => {
                self.stat.record(stat::process::Event::FaultSmall);
                return true;
            }
            Err(crate::Error::OutOfBounds) => (),
            Err(error) => panic!("Failed to extend small heap at {address:x?}: {error}"),
        }

        match allocator.large.try_map(
            &self.backend,
            &self.local_large,
            &self.remote_large,
            &self.data_large,
            &context,
            address,
        ) {
            Ok(()) => {
                self.stat.record(stat::process::Event::FaultLarge);
                return true;
            }
            Err(crate::Error::OutOfBounds) => (),
            Err(error) => panic!("Failed to extend large heap at {address:x?}: {error}"),
        }

        match allocator.huge.try_map(&allocator.small.data, id, address) {
            Ok(()) => {
                self.stat.record(stat::process::Event::FaultHuge);
                return true;
            }
            Err(crate::Error::OutOfBounds) => (),
            Err(error) => panic!("Failed to map huge allocation at {address:x?}: {error}"),
        }

        false
    }

    fn unfocused<S, O>(&self) -> allocator::Allocator<view::Unfocus, S, O> {
        let (_, shared_offsets) = Self::shared();
        let (_, owned_offsets) = Self::owned();
        let shared = self.shared.address().as_ptr();
        let owned = self.owned.address().as_ptr();
        unsafe {
            // Note: calls layout code at runtime. Ideally the layout information could be
            // a const, but some APIs (Layout::extend, Layout::pad_to_align) aren't
            // const yet.
            allocator::Allocator::new(
                (),
                shared
                    .wrapping_byte_add(shared_offsets[0])
                    .cast::<allocator::Shared<S>>()
                    .as_ref()
                    .unwrap(),
                owned
                    .wrapping_byte_add(owned_offsets[0])
                    .cast::<thread::Array<UnsafeCell<allocator::Owned>>>()
                    .as_ref()
                    .unwrap(),
                Heap::<view::Unfocus, size::Small>::new(
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
                    Slab::new(
                        slab::Slice::from_raw(self.local_small.address().cast()),
                        slab::Slice::from_raw(self.remote_small.address().cast()),
                    ),
                    Data::<size::Small>::new(self.data_small.address(), self.small_slab_capacity),
                ),
                Heap::<view::Unfocus, size::Large>::new(
                    shared
                        .wrapping_byte_add(shared_offsets[2])
                        .cast::<heap::Shared<size::Large>>()
                        .as_ref()
                        .unwrap(),
                    owned
                        .wrapping_byte_add(owned_offsets[2])
                        .cast::<thread::Array<UnsafeCell<heap::Owned<size::Large>>>>()
                        .as_ref()
                        .unwrap(),
                    Slab::new(
                        slab::Slice::from_raw(self.local_large.address().cast()),
                        slab::Slice::from_raw(self.remote_large.address().cast()),
                    ),
                    Data::<size::Large>::new(self.data_large.address(), self.large_slab_capacity),
                ),
                Huge::new(
                    &self.backend,
                    &self.data_huge,
                    shared
                        .wrapping_byte_add(shared_offsets[3])
                        .cast::<huge::Shared>()
                        .as_ref()
                        .unwrap(),
                    owned
                        .wrapping_byte_add(owned_offsets[3])
                        .cast::<thread::Array<huge::Owned>>()
                        .as_ref()
                        .unwrap(),
                    Data::<size::Huge>::new(self.data_huge.address(), u32::MAX),
                ),
            )
        }
    }

    pub fn is_clean(&self) -> bool {
        self.regions().any(Region::is_clean)
    }

    pub(crate) fn shared() -> (NonZeroUsize, Vec<usize>) {
        layout!(
            allocator::Shared<()>,
            heap::Shared<size::Small>,
            heap::Shared<size::Large>,
            huge::Shared,
        )
    }

    pub(crate) fn owned() -> (NonZeroUsize, Vec<usize>) {
        layout!(
            thread::Array<UnsafeCell<allocator::Owned>>,
            thread::Array<UnsafeCell<heap::Owned<size::Small>>>,
            thread::Array<UnsafeCell<heap::Owned<size::Large>>>,
            thread::Array<huge::Owned>,
        )
    }

    fn regions(&self) -> impl Iterator<Item = &dyn Region> {
        [
            &self.shared as &dyn Region,
            &self.owned,
            &self.local_small,
            &self.local_large,
            &self.remote_small,
            &self.remote_large,
            &self.data_small,
            &self.data_large,
            &self.data_huge,
        ]
        .into_iter()
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
