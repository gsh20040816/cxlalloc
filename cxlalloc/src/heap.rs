use core::ffi;
use core::fmt::Display;
use core::ops::Add;
use core::ops::Range;

use ribbit::private::u24;

use crate::allocator;
use crate::allocator::Bracket;
use crate::allocator::BracketUnpacked;
use crate::allocator::Index;
use crate::allocator::IndexUnpacked;
use crate::atomic::Version;
use crate::cas;
use crate::cas::help;
use crate::crash;
use crate::data;
use crate::log::StateUnpacked;
use crate::log::UnsizedToSized;
use crate::size;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::Data;
use crate::Epoch;
use crate::Slab;
use crate::BATCH_BUMP_POP;
use crate::BATCH_GLOBAL_PUSH;
use crate::COUNT_CACHE_SLAB;

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
    bump: cas::Detectable<Bump>,
}

impl<B> Shared<B> {
    pub(crate) fn bump(
        &self,
        id: thread::Id,
        capacity: u32,
        help: &help::Array,
    ) -> Option<Range<slab::Index<B>>> {
        let bump = self
            .bump
            .update(&help, id, |old, version| {
                let old_len = old.length();
                let new_len = old_len + BATCH_BUMP_POP;

                if u32::from(new_len) >= old.epoch().total(capacity) {
                    panic!(
                        "Heap extension not yet enabled. Tried to expand from {:#x} to {:#x} but capacity is {:#x}.",
                        u32::from(old_len),
                        u32::from(new_len),
                        capacity
                    );
                } else {
                    Some(
                        old.with_length(new_len),
                        // StateUnpacked::BumpToLocal(BumpToLocal::new(old, version)),
                    )
                }
            })?;

        let start = slab::Index::from_length(bump.length());
        let end = slab::Index::from_length(bump.length() + BATCH_BUMP_POP);
        Some(start..end)
    }

    pub(crate) fn push(
        &self,
        id: thread::Id,
        slabs: &Slab<B>,
        help: &help::Array,
        head: slab::Index<B>,
        tail: slab::Index<B>,
    ) {
        self.free.push(id, slabs, help, head, tail);
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        slabs: &Slab<B>,
        help: &help::Array,
    ) -> Option<slab::Index<B>> {
        if self.free.is_empty(help) {
            return None;
        }

        self.free.pop(id, slabs, help)
    }
}

#[ribbit::pack(size = 32, debug, new(vis = ""))]
#[derive(Copy, Clone)]
pub(crate) struct Bump {
    #[ribbit(size = 24)]
    length: Length,
    #[ribbit(size = 8)]
    epoch: Epoch,
}

#[ribbit::pack(size = 24)]
#[derive(Copy, Clone)]
pub(crate) struct Length(u24);

impl From<Length> for u32 {
    fn from(length: Length) -> Self {
        length._0().value()
    }
}

impl Add<u32> for Length {
    type Output = Self;
    fn add(self, rhs: u32) -> Self::Output {
        Self::new(self._0() + u24::new(rhs))
    }
}

impl Display for Length {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        Display::fmt(&u32::from(*self), f)
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
        id: thread::Id,
        log: &mut allocator::Owned,
        slabs: &Slab<B>,
        class: B,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        crash::define!(unsized_to_sized_pre_log);

        let slab = &slabs[index];
        let next = slab.local.next.load();

        log.log(StateUnpacked::UnsizedToSized(UnsizedToSized::new(
            next.map(Into::into),
            class.into(),
        )));

        self.r#sized[class].push(slabs, index);
        unsafe {
            (*slab.local.free.get()).fill(class.count());
        }

        slab.remote
            .owner
            .store(slab::remote::Owner::new(class, Some(id)));
        crate::flush(&slab.remote.owner, false);

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
            crate::flush(&slabs[prev], false);
        };

        self.r#unsized.push(slabs, index);
    }
}

impl<B> Heap<'_, view::Focus, B>
where
    B: size::Bracket + Default + Display + ribbit::Pack<Loose = u8>,
    B: Into<Bracket>,
    slab::Index<B>: Into<Index>,
{
    pub(crate) fn class(&self, offset: data::Offset<B>) -> B {
        let index = offset.into_index();
        self.slabs[index].remote.owner.load().class()
    }

    #[inline]
    pub(crate) fn pop(
        &mut self,
        id: thread::Id,
        class: B,
        index: slab::Index<B>,
    ) -> *mut ffi::c_void {
        let free = unsafe { &mut *self.slabs[index].local.free.get() };
        let block = free.peek();

        // FIXME: log
        // self.owned
        //     .log_sync(StateUnpacked::SizedToApplication(SizedToApplication::new(
        //         index, block,
        //     )));

        free.unset(block);

        if free.is_empty() {
            self.detach(id, class);
        }

        let offset = data::Offset::from_block(index, class, block);
        self.data.offset_to_pointer::<ffi::c_void>(offset).as_ptr()
    }

    #[inline]
    pub(crate) fn peek(
        &mut self,
        id: thread::Id,
        log: &mut allocator::Owned,
        help: &help::Array,
        class: B,
    ) -> Option<slab::Index<B>> {
        if let Some(index) = self.owned.r#sized[class].peek() {
            stat::inc(&stat::ALLOCATE_FAST);
            return Some(index);
        };

        self.allocate(id, log, help, class)
    }

    #[cold]
    fn allocate(
        &mut self,
        id: thread::Id,
        log: &mut allocator::Owned,
        help: &help::Array,
        class: B,
    ) -> Option<slab::Index<B>> {
        stat::inc(&stat::ALLOCATE_SMALL);

        if class.is_min() {
            stat::inc(&stat::ALLOCATE_SMALL_ZERO);
            return None;
        }

        // Fast path: local unsized
        if self.owned.unsized_to_sized(id, log, &self.slabs, class) {
            stat::inc(&stat::ALLOCATE_SMALL_UNSIZED);
            return self.owned.r#sized[class].peek();
        }

        loop {
            if let Some(index) = self.shared.pop(id, &self.slabs, help) {
                stat::inc(&stat::ALLOCATE_SMALL_GLOBAL);
                slab::transfer(&self.slabs, index, None, Some(id));

                self.owned.r#unsized.push(&self.slabs, index);
                break;
            }

            match self.shared.bump(id, self.capacity, help) {
                Some(range) => {
                    stat::inc(&stat::ALLOCATE_SMALL_BUMP);
                    slab::transfer_all(
                        &self.slabs,
                        range.start,
                        BATCH_BUMP_POP as usize,
                        None,
                        Some(id),
                    );

                    unsafe {
                        self.slabs.link(range.clone(), None);
                        self.owned
                            .r#unsized
                            .set(Some(range.start), BATCH_BUMP_POP as usize);
                    }
                    break;
                }
                None => {
                    todo!()
                }
            }
        }

        self.owned.unsized_to_sized(id, log, &self.slabs, class);
        self.owned.r#sized[class].peek()
    }

    #[cold]
    fn detach(&mut self, id: thread::Id, class: B) {
        let index = self.owned.r#sized[class].pop(&self.slabs).unwrap();

        let slab = &self.slabs[index];
        if !slab.remote.free.is_empty() {
            stat::inc(&stat::ALLOCATE_FAST_DISOWN);
            let owner = slab.remote.owner.load();
            slab.remote
                .owner
                .store(slab::remote::Owner::new(owner.class(), None));
            crate::flush(&slab.remote.owner, false);
            self.transfer(index, Some(id), None);
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

    pub(crate) fn free(&mut self, id: thread::Id, help: &help::Array, offset: data::Offset<B>) {
        let index = slab::Index::from(offset);
        let slab = &self.slabs[index];
        let owner = slab.remote.owner.load();
        let class = owner.class();

        if owner.id() != Some(id) {
            return unsafe { self.free_remote(id, help, offset, index, class) };
        }

        stat::inc(&stat::FREE_FAST);
        let slab = &self.slabs[index];
        let block = offset.into_block(class);
        let free = unsafe { &mut *slab.local.free.get() };

        // self.owned
        //     .meta
        //     .log_sync(StateUnpacked::ApplicationToSized(ApplicationToSized::new(
        //         index, block,
        //     )));

        let count = free.len();
        free.set(block);

        match count {
            count if count + 1 == class.count() => {
                stat::inc(&stat::FREE_FAST_UNSIZED);
                self.owned.sized_to_unsized(&self.slabs, class, index);
                self.unsized_to_global(id, help);
            }
            0 => self.attach(class, index),
            _ => (),
        }
    }

    #[cold]
    pub(crate) unsafe fn free_remote(
        &mut self,
        id: thread::Id,
        help: &help::Array,
        offset: data::Offset<B>,
        index: slab::Index<B>,
        class: B,
    ) {
        stat::inc(&stat::FREE_REMOTE);

        let slab = &self.slabs[index].remote;
        let block = offset.into_block(class);
        let version = slab.meta.load().version();

        // self.owned
        //     .log_sync(StateUnpacked::Remote(Remote::new(index, block, version)));

        slab.free.set(block);

        if slab.free.is_full(class.count()) {
            self.claim(id, help, index, version);
        }
    }

    #[cold]
    fn claim(
        &mut self,
        id: thread::Id,
        help: &help::Array,
        index: slab::Index<B>,
        version: Version,
    ) {
        stat::inc(&stat::FREE_REMOTE_GLOBAL);

        let slab = &self.slabs[index].remote;

        // Note: must use version from *before* we set our bit,
        // or else the full slab becomes globally visible and
        // some other thread can update the version.
        let old = slab::remote::Meta::new(version, slab.meta.load().claim());
        let new = slab::remote::Meta::new(version.next(), Some(id));

        // FIXME: get rid of CAS
        match slab.meta.compare_exchange(old, new) {
            Ok(_) => {
                crate::flush(&slab.meta, false);
                crate::fence();
                stat::inc(&stat::FREE_REMOTE_GLOBAL_WIN);
            }
            Err(_) => {
                crate::flush(&slab.meta, true);
                stat::inc(&stat::FREE_REMOTE_GLOBAL_LOSE);
                return;
            }
        }

        slab.free.clear();
        crate::flush(&slab.free, false);

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

        self.transfer(index, victim, Some(id));

        if victim.is_some() {
            stat::inc(&stat::FREE_REMOTE_GLOBAL_WIN_STEAL);
            slab.owner
                .store(slab::remote::Owner::new(B::default(), Some(id)));
            crate::flush(&slab.owner, false);
        }

        self.owned.r#unsized.push(&self.slabs, index);
        self.unsized_to_global(id, help);
    }

    fn unsized_to_global(&mut self, id: thread::Id, help: &help::Array) {
        let count = self.owned.r#unsized.len();
        if count < COUNT_CACHE_SLAB {
            return;
        }

        let mut iter = self
            .owned
            .r#unsized
            .trace(&self.slabs)
            .inspect(|index| self.transfer(*index, Some(id), None))
            .take(BATCH_GLOBAL_PUSH);

        let head = iter.next().unwrap();
        let tail = iter.last().unwrap();
        let next = self.slabs[tail].local.next.load();

        // FIXME: logging
        // self.owned
        //     .log_sync(StateUnpacked::LocalToGlobalSave(LocalToGlobalSave::new(
        //         head,
        //     )));

        self.owned.r#unsized.set(next, count - BATCH_GLOBAL_PUSH);
        self.shared.push(id, &self.slabs, help, head, tail);
    }

    #[inline]
    fn transfer(&self, index: slab::Index<B>, old: Option<thread::Id>, new: Option<thread::Id>) {
        slab::transfer(&self.slabs, index, old, new);
    }
}
