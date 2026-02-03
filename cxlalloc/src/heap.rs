use core::ffi;
use core::marker::PhantomData;
use core::mem;
use core::num::NonZeroUsize;
use core::ops::Range;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use std::collections::HashSet;

use crate::allocator;
use crate::bitset::Bit;
use crate::bitset::Interface as _;
use crate::cache;
use crate::cas;
use crate::crash;
use crate::data;
use crate::raw::region;
use crate::raw::Backend;
use crate::recover;
use crate::recover::HeapState;
use crate::recover::State;
use crate::size;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::Data;
use crate::Slab;
use crate::BATCH_BUMP_POP;
use crate::BATCH_GLOBAL_PUSH;
use crate::COUNT_CACHE_SLAB;

use self::region::Region as _;

pub struct Heap<'raw, L: view::Lens, B: size::Bracket> {
    /// Multiple-reader, multiple-writer metadata
    pub(crate) shared: &'raw Shared<B>,

    /// Single-reader, single-writer metadata
    pub(crate) owned: L::Scope<'raw, Owned<B>>,

    pub(crate) slabs: Slab<'raw, B>,
    pub(crate) data: Data<'raw, B>,

    stat: stat::thread::Recorder<B>,
}

pub(crate) struct Layout<B> {
    pub(crate) locals: NonZeroUsize,
    pub(crate) remotes: NonZeroUsize,
    pub(crate) data: NonZeroUsize,
    _bracket: PhantomData<B>,
}

impl<B> Default for Layout<B>
where
    B: size::Bracket,
{
    fn default() -> Self {
        const SIZE: usize = 1 << 30;
        Heap::<view::Unfocus, B>::layout(
            NonZeroUsize::new(SIZE.next_multiple_of(B::SIZE_SLAB) / B::SIZE_SLAB).unwrap(),
        )
        .unwrap()
    }
}

impl<L, B> Heap<'_, L, B>
where
    L: view::Lens,
    B: size::Bracket,
{
    pub(crate) fn layout(count: NonZeroUsize) -> Result<Layout<B>, core::alloc::LayoutError> {
        let count = count.get();
        Ok(Layout {
            locals: NonZeroUsize::new(slab::Slice::<B, slab::Local<B>>::layout(count)?.size())
                .unwrap(),
            remotes: NonZeroUsize::new(
                slab::Slice::<B, cas::Detectable<slab::Remote>>::layout(count)?.size(),
            )
            .unwrap(),
            data: NonZeroUsize::new(Data::<B>::layout(count)?.size()).unwrap(),
            _bracket: PhantomData,
        })
    }
}

impl<'raw, L, B> Heap<'raw, L, B>
where
    L: view::Lens,
    B: size::Bracket,
    State: From<HeapState<B>>,
{
    pub(crate) fn new(
        shared: &'raw Shared<B>,
        owned: L::Scope<'raw, Owned<B>>,
        slabs: Slab<'raw, B>,
        data: Data<'raw, B>,
    ) -> Self {
        Self {
            shared,
            owned,
            slabs,
            data,
            stat: stat::thread::Recorder::default(),
        }
    }

    pub(crate) unsafe fn focus(self, id: thread::Id) -> Heap<'raw, view::Focus, B> {
        Heap {
            shared: self.shared,
            owned: L::focus(self.owned, id),
            slabs: self.slabs,
            data: self.data,
            stat: self.stat,
        }
    }

    pub(crate) fn report(&self, id: thread::Id) -> impl Iterator<Item = stat::Report> + '_ {
        self.stat.report(id)
    }

    pub(crate) fn checked_pointer_to_offset(
        &self,
        pointer: NonNull<ffi::c_void>,
    ) -> Option<data::Offset<B>> {
        let offset = self.data.pointer_to_offset(pointer)?;
        match (u64::from(offset) as usize) < crate::raw::region::Reservation::SIZE.get() {
            true => Some(offset),
            false => None,
        }
    }

    pub(crate) fn try_map(
        &self,
        backend: &Backend,
        local: &region::Sequential,
        remote: &region::Sequential,
        data: &region::Sequential,
        context: &allocator::Context,
        address: NonNull<ffi::c_void>,
    ) -> crate::Result<()> {
        let Some(len) = self.shared.len(context).map(u32::from) else {
            return Err(crate::Error::OutOfBounds);
        };

        let size_local = const { mem::size_of::<slab::Local<B>>() };
        let size_remote = const { mem::size_of::<cas::Detectable<slab::Remote>>() };
        let size_slab = const { B::SIZE_SLAB };

        // Check if within either region
        let local_lo = local.address().as_ptr().cast::<ffi::c_void>();
        let local_hi = local_lo.wrapping_byte_add(len as usize * size_local);

        let remote_lo = remote.address().as_ptr().cast::<ffi::c_void>();
        let remote_hi =
            remote_lo.wrapping_byte_add(len as usize * size_of::<cas::Detectable<slab::Remote>>());

        let data_lo = data.address().as_ptr().cast::<ffi::c_void>();
        let data_hi = data_lo.wrapping_byte_add(len as usize * B::SIZE_SLAB);

        let address = address.as_ptr();

        let (local_offset, remote_offset, data_offset) = if (local_lo..local_hi).contains(&address)
        {
            let local_offset = address as usize - local_lo as usize;
            let remote_offset = local_offset / size_local * size_remote;
            let data_offset = local_offset / size_local * size_slab;
            (local_offset, remote_offset, data_offset)
        } else if (remote_lo..remote_hi).contains(&address) {
            let remote_offset = address as usize - remote_lo as usize;
            let local_offset = remote_offset / size_remote * size_local;
            let data_offset = remote_offset / size_remote * size_slab;
            (local_offset, remote_offset, data_offset)
        } else if (data_lo..data_hi).contains(&address) {
            let data_offset = address as usize - data_lo as usize;
            let local_offset = data_offset / size_slab * size_local;
            let remote_offset = data_offset / size_slab * size_remote;
            (local_offset, remote_offset, data_offset)
        } else {
            return Err(crate::Error::OutOfBounds);
        };

        local.map(backend, local_offset)?;
        remote.map(backend, remote_offset)?;
        data.map(backend, data_offset)?;
        Ok(())
    }
}

impl<B> Heap<'_, view::Focus, B>
where
    B: size::Bracket,
    recover::State: From<HeapState<B>>,
{
    pub(crate) fn class(&self, offset: data::Offset<B>) -> B {
        let index = offset.into_index();
        self.slabs.local(index).class.load(Ordering::Relaxed)
    }

    #[inline]
    pub(crate) fn pop(
        &mut self,
        context: &mut allocator::Context,
        class: B,
        index: slab::Index<B>,
        block: Bit,
    ) -> *mut ffi::c_void {
        self.stat.record(
            context.id,
            stat::thread::Event::Allocate { size: class.size() },
        );

        let free = unsafe { &mut *self.slabs.local(index).free.get() };
        free.unset(block);

        if free.is_empty() {
            let index = self.owned.r#sized[class].pop(&self.slabs).unwrap();
            self.detach(context, class, index);
        }

        let offset = data::Offset::from_block(class, index, block);
        self.data.offset_to_pointer::<ffi::c_void>(offset).as_ptr()
    }

    #[inline]
    pub(crate) fn peek(
        &mut self,
        context: &mut allocator::Context,
        class: B,
    ) -> Option<(slab::Index<B>, Bit)> {
        let index = self.owned.r#sized[class]
            .peek()
            .or_else(|| self.allocate(context, class))?;

        let free = unsafe { &*self.slabs.local(index).free.get() };
        let block = unsafe { free.peek_unchecked() };
        Some((index, block))
    }

    #[cold]
    fn allocate(&mut self, context: &mut allocator::Context, class: B) -> Option<slab::Index<B>> {
        if class.is_zero() {
            return None;
        }

        // Fast path: local unsized
        if self.owned.unsized_to_sized(context, &self.slabs, class) {
            self.stat
                .record(context.id, stat::thread::Event::UnsizedToSized { class });

            validate!(self.owned.is_valid(context, &self.slabs));
            return self.owned.r#sized[class].peek();
        }

        if let Some(index) = self.shared.pop(context, &self.slabs) {
            self.stat
                .record(context.id, stat::thread::Event::GlobalToUnsized);

            log::info!("Transfer: {} from global to unsized", index);
            self.slabs.local(index).own(context.id);

            self.owned.r#unsized.push(&self.slabs, index);
        } else {
            let range = self.shared.bump(context);

            log::info!(
                "Transfer: {}..{} from bump to unsized",
                range.start,
                range.end,
            );
            self.stat.record(context.id, stat::thread::Event::Bump);

            let batch = BATCH_BUMP_POP.load(Ordering::Relaxed);

            unsafe {
                self.slabs.link(context.id, range.clone(), None);
                self.owned.r#unsized.set(Some(range.start), batch);
            }
        }

        self.owned.unsized_to_sized(context, &self.slabs, class);
        self.stat
            .record(context.id, stat::thread::Event::UnsizedToSized { class });

        validate!(self.owned.is_valid(context, &self.slabs));
        self.owned.r#sized[class].peek()
    }

    #[cold]
    pub(crate) fn detach(
        &mut self,
        context: &mut allocator::Context,
        class: B,
        index: slab::Index<B>,
    ) {
        self.stat
            .record(context.id, stat::thread::Event::Detach { class });

        let remote = &self.slabs.remote(index);
        let meta = remote.load(context, Ordering::Relaxed);

        if (meta.free as u64) < class.count() {
            log::info!("Transfer: disown {} ({:?})", index, class,);

            self.stat
                .record(context.id, stat::thread::Event::Disown { class });

            let local = self.slabs.local(index);
            local.disown(context.id);
            cache::flush_cxl(local);
            cache::fence_cxl();
        } else {
            log::info!("Transfer: detach {} ({:?})", index, class,);
        }
    }

    #[cold]
    fn attach(&mut self, context: &mut allocator::Context, class: B, index: slab::Index<B>) {
        self.stat
            .record(context.id, stat::thread::Event::Attach { class });

        log::info!("Transfer: attach {} ({:?})", index, class);

        self.owned.r#sized[class].push(&self.slabs, index);
    }

    #[inline]
    pub(crate) fn free(&mut self, context: &mut allocator::Context, offset: data::Offset<B>) {
        let index = slab::Index::from(offset);

        let local = self.slabs.local(index);
        let owner = local.owner();

        if owner != Some(context.id) {
            return self.free_remote(context, index);
        }

        let local = self.slabs.local(index);
        let class = local.class.load(Ordering::Relaxed);
        let block = offset.into_block(class);
        let free = unsafe { &mut *local.free.get() };

        self.stat
            .record(context.id, stat::thread::Event::Free { size: class.size() });
        context.log(HeapState::ApplicationToSized { index, block });

        free.set(block);
        let count = free.len();

        // Attach if not empty
        if count == 1 {
            self.attach(context, class, index);
        }

        // Return to unsized if full
        if count < class.count() {
            return;
        }

        self.owned.sized_to_unsized(&self.slabs, class, index);
        validate!(self.owned.is_valid(context, &self.slabs));

        self.stat
            .record(context.id, stat::thread::Event::SizedToUnsized { class });

        self.unsized_to_global(context);
    }

    #[cold]
    pub(crate) fn free_remote(&mut self, context: &mut allocator::Context, index: slab::Index<B>) {
        let class = self.slabs.local(index).class.load(Ordering::Relaxed);

        self.stat
            .record(context.id, stat::thread::Event::Free { size: class.size() });

        let remote = self.slabs.remote(index);
        let meta = remote
            .update(
                context,
                Ordering::Relaxed,
                Ordering::Relaxed,
                |meta, version| {
                    validate!(meta.free > 0);

                    let last = meta.free as u64 == 1;
                    let next = slab::Remote {
                        free: meta.free - 1,
                    };

                    Some((
                        next,
                        recover::HeapState::Remote {
                            index,
                            version,
                            last,
                        },
                    ))
                },
            )
            .unwrap();

        if meta.free == 1 {
            self.claim(context, class, index);
        }
    }

    #[cold]
    fn claim(&mut self, context: &mut allocator::Context, class: B, index: slab::Index<B>) {
        self.stat
            .record(context.id, stat::thread::Event::Claim { class });

        log::info!(
            "Transfer: {} from detached ({:?}) to unsized",
            index,
            self.slabs.local(index).owner(),
        );

        // NOTE: it's possible for previous owner to be non-None if they
        // detached slab without disowning, and all allocations are
        // subsequently freed remotely.
        self.slabs.local(index).steal(context.id);
        self.owned.r#unsized.push(&self.slabs, index);

        validate!(self.owned.is_valid(context, &self.slabs));

        self.unsized_to_global(context);
    }

    pub(crate) fn unsized_to_global(&mut self, context: &mut allocator::Context) {
        let count = self.owned.r#unsized.len();
        if count <= COUNT_CACHE_SLAB.load(Ordering::Relaxed) {
            return;
        }

        self.stat
            .record(context.id, stat::thread::Event::UnsizedToGlobal);

        let batch = BATCH_GLOBAL_PUSH.load(Ordering::Relaxed);
        let mut iter = self
            .owned
            .r#unsized
            .trace(&self.slabs)
            .inspect(|index| {
                // Not strictly necessary, but helps debugging
                if cfg!(any(feature = "validate", debug_assertions)) {
                    self.slabs.local(*index).disown(context.id);
                }
            })
            .take(batch);

        let head = iter.next().unwrap();
        let tail = iter.last().unwrap_or(head);
        let next = self.slabs.local(tail).next.load(Ordering::Relaxed);

        log::info!("Transfer: {}..={} from unsized to global", head, tail);

        context.log(HeapState::UnsizedToGlobalSave { index: head });

        self.owned.r#unsized.set(next, count - batch);

        // Maintain software cache coherence
        self.slabs
            .trace(Some(head))
            .take(batch)
            .for_each(|index| cache::flush_cxl(self.slabs.local(index)));
        cache::fence_cxl();

        self.shared.push(context, &self.slabs, head, tail);
    }
}

#[repr(C)]
pub(crate) struct Shared<B> {
    free: slab::stack::Global<B>,
    bump: cas::Detectable<Option<slab::Index<B>>>,
}

impl<B> Shared<B>
where
    B: size::Bracket,
    State: From<HeapState<B>>,
{
    fn len(&self, context: &allocator::Context) -> Option<slab::Index<B>> {
        self.bump.load(context, Ordering::Relaxed)
    }

    fn bump(&self, context: &mut allocator::Context) -> Range<slab::Index<B>> {
        let batch = BATCH_BUMP_POP.load(Ordering::Relaxed) as u32;
        let start = self
            .bump
            .update(
                context,
                Ordering::Relaxed,
                Ordering::Relaxed,
                |old, version| {
                    let new = unsafe { old.unwrap_or(slab::Index::MIN).add(batch) };
                    Some((
                        Some(new),
                        HeapState::BumpToUnsized {
                            start: old,
                            version,
                        },
                    ))
                },
            )
            .unwrap();

        let start = start.unwrap_or(slab::Index::MIN);
        let end = unsafe { start.add(batch) };
        start..end
    }

    pub(crate) fn detect_bump(
        &self,
        context: &mut allocator::Context,
        version: cas::Version,
    ) -> bool {
        self.bump.detect(context, version)
    }

    fn push(
        &self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
        head: slab::Index<B>,
        tail: slab::Index<B>,
    ) {
        self.free.push(context, slabs, head, tail);
    }

    fn pop(&self, context: &mut allocator::Context, slabs: &Slab<B>) -> Option<slab::Index<B>> {
        if self.free.is_empty(context) {
            return None;
        }

        self.free.pop(context, slabs)
    }

    pub(crate) fn detect_global(
        &self,
        context: &mut allocator::Context,
        version: cas::Version,
    ) -> bool {
        self.free.detect(context, version)
    }
}

pub(crate) struct Owned<B: size::Bracket> {
    pub(crate) r#unsized: slab::stack::Local<B>,
    pub(crate) r#sized: size::Array<B, slab::stack::Local<B>>,
}

impl<B> Owned<B>
where
    B: size::Bracket,
    State: From<HeapState<B>>,
{
    /// Invariants:
    /// - All descriptors in `unsized` stack have this thread as owner
    /// - All descriptors in `sized` stacks have:
    ///     - correct size class
    ///     - this thread as owner
    ///     - non-empty free bitset
    /// - `unsized` stack and `sized stacks contain disjoint sets of descriptors
    pub(crate) fn is_valid(&self, context: &allocator::Context, slabs: &Slab<B>) -> bool {
        let mut set = HashSet::new();

        if !self.r#unsized.is_valid(slabs) {
            return false;
        }

        if !self.r#unsized.trace(slabs).all(|index| {
            if !set.insert(index) {
                return false;
            }

            let local = slabs.local(index);
            local.owner() == Some(context.id)
        }) {
            return false;
        }

        self.r#sized.iter().all(|(class, stack)| {
            if !stack.is_valid(slabs) {
                return false;
            }

            stack.trace(slabs).all(|index| {
                if !set.insert(index) {
                    return false;
                }

                let local = slabs.local(index);

                unsafe { *local.free.get() }.len() > 0
                    && local.class.load(Ordering::Relaxed) == class
                    && local.owner() == Some(context.id)
            })
        })
    }

    pub(crate) fn unsized_to_sized(
        &mut self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
        class: B,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        log::info!("Transfer: {} from unsized to sized ({:?})", index, class);

        crash::define!(unsized_to_sized_pre_log);

        let local = slabs.local(index);
        let next = local.next.load(Ordering::Relaxed);
        context.log(HeapState::UnsizedToSized { index: next, class });

        local.class.store(class, Ordering::Relaxed);
        unsafe { &mut *local.free.get() }.fill(class.count());

        cache::flush(local, cache::Invalidate::No);

        self.r#sized[class].push(slabs, index);

        let remote = slabs.remote(index);
        remote.store(
            context,
            slab::Remote {
                free: class.count() as u16,
            },
            Ordering::Relaxed,
        );

        let count = self.r#unsized.len();
        self.r#unsized.set(next, count - 1);
        true
    }

    #[cold]
    pub(crate) fn sized_to_unsized(&mut self, slabs: &Slab<B>, class: B, index: slab::Index<B>) {
        log::info!("Transfer: {} from sized ({:?}) to unsized", index, class);

        let next = slabs.local(index).next.load(Ordering::Relaxed);

        let mut walk = self.r#sized[class].peek().unwrap();

        if walk == index {
            let count = self.r#sized[class].len();
            self.r#sized[class].set(next, count - 1);
        } else {
            let prev = loop {
                let local = slabs.local(walk);
                match local.next.load(Ordering::Relaxed) {
                    None => panic!("removing non-existent slab {index} {class:?}"),
                    Some(next) if next == index => break local,
                    Some(next) => walk = next,
                }
            };

            prev.next.store(next, Ordering::Relaxed);
            cache::flush(prev, cache::Invalidate::No);
        };

        self.r#unsized.push(slabs, index);
    }
}
