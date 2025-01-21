use core::ffi;
use core::fmt::Display;
use core::ops::Range;
use core::ptr::NonNull;

use crate::allocator;
use crate::allocator::Bracket;
use crate::allocator::Index;
use crate::atomic::Version;
use crate::cas;
use crate::cas::help;
use crate::coherence::flush;
use crate::coherence::sfence;
use crate::coherence::Invalidate;
use crate::crash;
use crate::data;
use crate::raw::region;
use crate::raw::Backend;
use crate::recover::ApplicationToSized;
use crate::recover::BumpToLocal;
use crate::recover::LocalToGlobalSave;
use crate::recover::Remote;
use crate::recover::SizedToApplication;
use crate::recover::StateUnpacked;
use crate::recover::UnsizedToSized;
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

pub struct Heap<'raw, L: view::Lens, B> {
    /// Capacity is in units of slabs
    pub(crate) capacity: u32,

    /// Multiple-reader, multiple-writer metadata
    pub(crate) shared: &'raw Shared<B>,

    /// Single-reader, single-writer metadata
    pub(crate) owned: L::Scope<'raw, Owned<B>>,

    pub(crate) slabs: Slab<'raw, B>,
    pub(crate) data: Data<'raw, B>,
}

impl<'raw, L: view::Lens, B> Heap<'raw, L, B> {
    pub(crate) fn new(
        capacity: u32,
        shared: &'raw Shared<B>,
        owned: L::Scope<'raw, Owned<B>>,
        slabs: Slab<'raw, B>,
        data: Data<'raw, B>,
    ) -> Self {
        Self {
            capacity,
            shared,
            owned,
            slabs,
            data,
        }
    }

    pub(crate) unsafe fn focus(self, id: thread::Id) -> Heap<'raw, view::Focus, B> {
        Heap {
            capacity: self.capacity,
            shared: self.shared,
            owned: L::focus(self.owned, id),
            slabs: self.slabs,
            data: self.data,
        }
    }
}

#[repr(C)]
pub(crate) struct Shared<B> {
    free: slab::stack::Global<B>,
    bump: cas::Detectable<Option<slab::Index<B>>>,
}

impl<B> Shared<B>
where
    slab::Index<B>: Into<allocator::Index>,
{
    pub(crate) fn bump(&self, context: &mut allocator::Context) -> Range<slab::Index<B>> {
        let start = self
            .bump
            .update(context, |old, version| {
                let new = unsafe { old.unwrap_or(slab::Index::MIN).add(BATCH_BUMP_POP) };
                Some((
                    Some(new),
                    StateUnpacked::BumpToLocal(BumpToLocal::new(old.map(Into::into), version)),
                ))
            })
            .unwrap();

        let start = start.unwrap_or(slab::Index::MIN);
        let end = unsafe { start.add(BATCH_BUMP_POP) };
        start..end
    }

    pub(crate) fn push(
        &self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
        head: slab::Index<B>,
        tail: slab::Index<B>,
    ) {
        self.free.push(context, slabs, head, tail);
    }

    pub(crate) fn pop(
        &self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
    ) -> Option<slab::Index<B>> {
        if self.free.is_empty(context.help) {
            return None;
        }

        self.free.pop(context, slabs)
    }
}

pub(crate) struct Owned<B> {
    pub(crate) r#unsized: slab::stack::Local<B>,
    pub(crate) r#sized: size::Array<B, slab::stack::Local<B>>,
}

impl<B> Owned<B>
where
    B: size::Bracket + Display + ribbit::Pack<Loose = u8> + Into<Bracket>,
    slab::Index<B>: Into<Index>,
{
    pub(crate) fn unsized_to_sized(
        &mut self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
        class: B,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        crash::define!(unsized_to_sized_pre_log);

        let slab = &slabs[index];
        let next = slab.local.next.load();

        context.log(StateUnpacked::UnsizedToSized(UnsizedToSized::new(
            next.map(Into::into),
            class.into(),
        )));

        self.r#sized[class].push(slabs, index);
        unsafe {
            (*slab.local.free.get()).fill(class.count());
        }

        slab.remote
            .owner
            .store(slab::remote::Owner::new(class, Some(context.id)));
        flush(&slab.remote.owner, Invalidate::No);

        let count = self.r#unsized.len();
        self.r#unsized.set(next, count - 1);
        true
    }

    #[cold]
    pub(crate) fn sized_to_unsized(&mut self, slabs: &Slab<B>, class: B, index: slab::Index<B>) {
        // Special case: not in sized list
        if class.is_max() {
            return self.r#unsized.push(slabs, index);
        }

        let next = slabs[index].local.next.load();

        let mut walk = self.r#sized[class].peek().unwrap();

        if walk == index {
            let count = self.r#sized[class].len();
            self.r#sized[class].set(next, count - 1);
        } else {
            let prev = loop {
                match slabs[walk].local.next.load() {
                    None => panic!("removing non-existent slab {} {}", index, class),
                    Some(next) if next == index => break walk,
                    Some(next) => walk = next,
                }
            };

            slabs[prev].local.next.store(next);
            flush(&slabs[prev], Invalidate::No);
        };

        self.r#unsized.push(slabs, index);
    }
}

impl<L, B> Heap<'_, L, B>
where
    L: view::Lens,
    B: size::Bracket + Display + ribbit::Pack<Loose = u8>,
{
    pub(crate) fn class(&self, offset: data::Offset<B>) -> B {
        let index = offset.into_index();
        self.slabs[index].remote.owner.load().class()
    }

    pub(crate) fn try_map(
        &self,
        backend: &Backend,
        slab: &region::Sequential,
        data: &region::Sequential,
        help: &help::Array,
        address: NonNull<ffi::c_void>,
    ) -> crate::Result<()> {
        let Some(len) = self.shared.bump.load(help).map(u32::from) else {
            return Err(crate::Error::OutOfBounds);
        };

        // Check if within either region
        let slab_lo = slab.address().as_ptr().cast::<ffi::c_void>();
        let slab_hi = slab_lo.wrapping_byte_add(len as usize * size_of::<slab::Descriptor<B>>());

        let data_lo = data.address().as_ptr().cast::<ffi::c_void>();
        let data_hi = data_lo.wrapping_byte_add(len as usize * B::SIZE_SLAB);

        let address = address.as_ptr();
        let (slab_offset, data_offset) = if (slab_lo..slab_hi).contains(&address) {
            let slab_offset = address as usize - slab_lo as usize;
            let data_offset = slab_offset / size_of::<slab::Descriptor<B>>() * B::SIZE_SLAB;
            (slab_offset, data_offset)
        } else if (data_lo..data_hi).contains(&address) {
            let data_offset = address as usize - data_lo as usize;
            let slab_offset = data_offset / B::SIZE_SLAB * size_of::<slab::Descriptor<B>>();
            (slab_offset, data_offset)
        } else {
            return Err(crate::Error::OutOfBounds);
        };

        slab.map(backend, slab_offset)?;
        data.map(backend, data_offset)?;
        Ok(())
    }
}

impl<B> Heap<'_, view::Focus, B>
where
    B: size::Bracket + Default + Display + ribbit::Pack<Loose = u8>,
    B: Into<Bracket>,
    slab::Index<B>: Into<Index>,
{
    #[inline]
    pub(crate) fn pop(
        &mut self,
        context: &mut allocator::Context,
        class: B,
        index: slab::Index<B>,
    ) -> *mut ffi::c_void {
        let free = unsafe { &mut *self.slabs[index].local.free.get() };
        let block = free.peek();

        context.log(StateUnpacked::SizedToApplication(SizedToApplication::new(
            index.into(),
            block,
        )));

        free.unset(block);

        if free.is_empty() {
            self.detach(context, class);
        }

        let offset = data::Offset::from_block(index, class, block);
        self.data.offset_to_pointer::<ffi::c_void>(offset).as_ptr()
    }

    #[inline]
    pub(crate) fn peek(
        &mut self,
        context: &mut allocator::Context,
        class: B,
    ) -> Option<slab::Index<B>> {
        if let Some(index) = self.owned.r#sized[class].peek() {
            stat::inc(&stat::ALLOCATE_FAST);
            return Some(index);
        };

        self.allocate(context, class)
    }

    #[cold]
    fn allocate(&mut self, context: &mut allocator::Context, class: B) -> Option<slab::Index<B>> {
        stat::inc(&stat::ALLOCATE_SMALL);

        if class.is_min() {
            stat::inc(&stat::ALLOCATE_SMALL_ZERO);
            return None;
        }

        // Fast path: local unsized
        if self.owned.unsized_to_sized(context, &self.slabs, class) {
            stat::inc(&stat::ALLOCATE_SMALL_UNSIZED);
            return self.owned.r#sized[class].peek();
        }

        'slow: {
            if let Some(index) = self.shared.pop(context, &self.slabs) {
                stat::inc(&stat::ALLOCATE_SMALL_GLOBAL);
                slab::transfer(&self.slabs, index, None, Some(context.id));

                self.owned.r#unsized.push(&self.slabs, index);
                break 'slow;
            }

            let range = self.shared.bump(context);
            stat::inc(&stat::ALLOCATE_SMALL_BUMP);
            slab::transfer_all(
                &self.slabs,
                range.start,
                BATCH_BUMP_POP as usize,
                None,
                Some(context.id),
            );

            unsafe {
                self.slabs.link(range.clone(), None);
                self.owned
                    .r#unsized
                    .set(Some(range.start), BATCH_BUMP_POP as usize);
            }
        }

        self.owned.unsized_to_sized(context, &self.slabs, class);
        self.owned.r#sized[class].peek()
    }

    #[cold]
    fn detach(&mut self, context: &mut allocator::Context, class: B) {
        let index = self.owned.r#sized[class].pop(&self.slabs).unwrap();

        let slab = &self.slabs[index];
        if !slab.remote.free.is_empty() {
            stat::inc(&stat::ALLOCATE_FAST_DISOWN);
            let owner = slab.remote.owner.load();
            slab.remote
                .owner
                .store(slab::remote::Owner::new(owner.class(), None));
            flush(&slab.remote.owner, Invalidate::No);
            self.transfer(index, Some(context.id), None);
        } else {
            stat::inc(&stat::ALLOCATE_FAST_DETACH);
        }

        if cfg!(feature = "validate") {
            assert!(self.owned.r#sized[class]
                .trace(&self.slabs)
                .all(|other| other != index));
        }
    }

    #[cold]
    fn attach(&mut self, class: B, index: slab::Index<B>) {
        if cfg!(feature = "validate") {
            assert!(self.owned.r#sized[class]
                .trace(&self.slabs)
                .all(|other| other != index));
        }

        self.owned.r#sized[class].push(&self.slabs, index);
        stat::inc(&stat::FREE_FAST_ATTACH);
    }

    pub(crate) fn free(&mut self, context: &mut allocator::Context, offset: data::Offset<B>) {
        let index = slab::Index::from(offset);
        let slab = &self.slabs[index];
        let owner = slab.remote.owner.load();
        let class = owner.class();

        if owner.id() != Some(context.id) {
            return unsafe { self.free_remote(context, offset, index, class) };
        }

        stat::inc(&stat::FREE_FAST);
        let slab = &self.slabs[index];
        let block = offset.into_block(class);
        let free = unsafe { &mut *slab.local.free.get() };

        context.log(StateUnpacked::ApplicationToSized(ApplicationToSized::new(
            index.into(),
            block,
        )));

        let count = free.len();
        free.set(block);

        match count {
            count if count + 1 == class.count() => {
                stat::inc(&stat::FREE_FAST_UNSIZED);
                self.owned.sized_to_unsized(&self.slabs, class, index);
                self.unsized_to_global(context);
            }
            0 => self.attach(class, index),
            _ => (),
        }
    }

    #[cold]
    pub(crate) unsafe fn free_remote(
        &mut self,
        context: &mut allocator::Context,
        offset: data::Offset<B>,
        index: slab::Index<B>,
        class: B,
    ) {
        stat::inc(&stat::FREE_REMOTE);

        let slab = &self.slabs[index].remote;
        let block = offset.into_block(class);
        let version = slab.meta.load().version();

        context.log(StateUnpacked::Remote(Remote::new(
            index.into(),
            block,
            version,
        )));

        slab.free.set(block);

        if slab.free.is_full(class.count()) {
            self.claim(context, index, version);
        }
    }

    #[cold]
    fn claim(&mut self, context: &mut allocator::Context, index: slab::Index<B>, version: Version) {
        stat::inc(&stat::FREE_REMOTE_GLOBAL);

        let slab = &self.slabs[index].remote;

        // Note: must use version from *before* we set our bit,
        // or else the full slab becomes globally visible and
        // some other thread can update the version.
        let old = slab::remote::Meta::new(version, slab.meta.load().claim());
        let new = slab::remote::Meta::new(version.next(), Some(context.id));

        // FIXME: get rid of CAS
        match slab.meta.compare_exchange(old, new) {
            Ok(_) => {
                flush(&slab.meta, Invalidate::No);
                sfence();
                stat::inc(&stat::FREE_REMOTE_GLOBAL_WIN);
            }
            Err(_) => {
                flush(&slab.meta, Invalidate::Yes);
                stat::inc(&stat::FREE_REMOTE_GLOBAL_LOSE);
                return;
            }
        }

        slab.free.clear();
        flush(&slab.free, Invalidate::No);

        if cfg!(feature = "validate") {
            assert!(
                self.owned
                    .r#unsized
                    .trace(&self.slabs)
                    .all(|other| other != index),
                "Claim does not introduce alias",
            );
        }

        let victim = slab.owner.load().id();

        self.transfer(index, victim, Some(context.id));

        if victim.is_some() {
            stat::inc(&stat::FREE_REMOTE_GLOBAL_WIN_STEAL);
            slab.owner
                .store(slab::remote::Owner::new(B::default(), Some(context.id)));
            flush(&slab.owner, Invalidate::No);
        }

        self.owned.r#unsized.push(&self.slabs, index);
        self.unsized_to_global(context);
    }

    fn unsized_to_global(&mut self, context: &mut allocator::Context) {
        let count = self.owned.r#unsized.len();
        if count < COUNT_CACHE_SLAB {
            return;
        }

        let mut iter = self
            .owned
            .r#unsized
            .trace(&self.slabs)
            .inspect(|index| self.transfer(*index, Some(context.id), None))
            .take(BATCH_GLOBAL_PUSH);

        let head = iter.next().unwrap();
        let tail = iter.last().unwrap();
        let next = self.slabs[tail].local.next.load();

        context.log(StateUnpacked::LocalToGlobalSave(LocalToGlobalSave::new(
            head.into(),
        )));

        self.owned.r#unsized.set(next, count - BATCH_GLOBAL_PUSH);
        self.shared.push(context, &self.slabs, head, tail);
    }

    #[inline]
    fn transfer(&self, index: slab::Index<B>, old: Option<thread::Id>, new: Option<thread::Id>) {
        slab::transfer(&self.slabs, index, old, new);
    }
}
