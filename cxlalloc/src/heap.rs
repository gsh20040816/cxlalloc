use core::ffi;

use crate::atomic::Version;
use crate::cas::help;
use crate::size;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::view::Heap;
use crate::BATCH_BUMP_POP;
use crate::BATCH_GLOBAL_PUSH;
use crate::COUNT_CACHE_SLAB;

impl<'raw, B: size::Bracket> Heap<'raw, view::Focus, B> {
    #[inline]
    pub(crate) fn pop(&mut self, id: thread::Id, class: B, index: slab::Index) -> *mut ffi::c_void {
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

        todo!()

        // let offset = unsafe { index.offset(class, block) };
        // self.data.offset_to_pointer::<ffi::c_void>(offset).as_ptr()
    }

    #[inline]
    pub(crate) fn peek(
        &mut self,
        id: thread::Id,
        help: &help::Array,
        class: B,
    ) -> Option<slab::Index> {
        if let Some(index) = self.owned.r#sized[class].peek() {
            stat::inc(&stat::ALLOCATE_FAST);
            return Some(index);
        };

        self.allocate(id, help, class)
    }

    #[cold]
    fn allocate(&mut self, id: thread::Id, help: &help::Array, class: B) -> Option<slab::Index> {
        stat::inc(&stat::ALLOCATE_SMALL);

        if class.is_min() {
            stat::inc(&stat::ALLOCATE_SMALL_ZERO);
            return None;
        }

        // Fast path: local unsized
        if self.owned.unsized_to_sized(&self.slabs, id, class) {
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

        self.owned.unsized_to_sized(&self.slabs, id, class);
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
    fn attach(&mut self, class: B, index: slab::Index) {
        if cfg!(feature = "validate") {
            assert!(self.owned.r#sized[class]
                .trace(&self.slabs)
                .all(|other| other != index));
        }

        self.owned.r#sized[class].push(&self.slabs, index);
        stat::inc(&stat::FREE_FAST_ATTACH);
    }

    // FIXME: offset
    #[cold]
    unsafe fn free_remote(&mut self, offset: (), index: slab::Index, class: B) {
        stat::inc(&stat::FREE_REMOTE);
        todo!()

        // let slab = &self.slabs[index];
        // let block = offset.index_block(class);
        // let version = slab.meta.load().version();
        //
        // self.owned
        //     .meta
        //     .log_sync(StateUnpacked::Remote(Remote::new(index, block, version)));
        //
        // slab.free.set(block);
        //
        // if slab.free.is_full(class.count()) {
        //     self.claim(index, version);
        // }
    }

    #[cold]
    fn claim(&mut self, id: thread::Id, help: &help::Array, index: slab::Index, version: Version) {
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
    fn transfer(&self, index: slab::Index, old: Option<thread::Id>, new: Option<thread::Id>) {
        slab::transfer(&self.slabs, index, old, new);
    }
}
