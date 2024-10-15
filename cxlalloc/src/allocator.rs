use core::ffi;
use core::ptr::NonNull;

use crate::atomic::Version;
use crate::link;
use crate::raw;
use crate::region;
use crate::region::owned::State;
use crate::root;
use crate::size;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::Heap;
use crate::Root;
use crate::BATCH_BUMP_POP;
use crate::BATCH_GLOBAL_PUSH;
use crate::COUNT_CACHE_SLAB;

pub struct Allocator<'raw> {
    id: thread::Id,
    owned: region::Owned<'raw>,
    heap: Heap<'raw>,
}

impl<'raw> Allocator<'raw> {
    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner, id: thread::Id) -> Self {
        let heap = Heap::from_raw(raw);
        let owned = region::Owned::from_raw(raw, id);

        Self { id, owned, heap }
    }

    pub fn heap(&self) -> &Heap<'raw> {
        &self.heap
    }

    pub unsafe fn root<T>(&self, index: root::Index) -> Root<'raw, T> {
        Root::new(self, index)
    }

    pub fn allocate_at<'root, T: Default, L: link::Erase<'raw, 'root, T>>(
        &mut self,
        _: L,
    ) -> &'root mut T {
        todo!()
    }

    #[cold]
    fn allocate_small(&mut self, class: size::Class) -> Option<slab::Index> {
        stat::inc(&stat::ALLOCATE_SMALL);

        if class.is_zero() {
            stat::inc(&stat::ALLOCATE_SMALL_ZERO);
            return None;
        }

        let thread = &mut *self.owned.meta;

        // Fast path: local unsized
        if thread.unsized_to_sized(&self.owned.slabs, &self.heap.shared.slabs, self.id, class) {
            stat::inc(&stat::ALLOCATE_SMALL_UNSIZED);
            return thread.r#sized[class].peek();
        }

        loop {
            if let Some(index) = self.heap.shared.pop(self.id, thread, &self.owned.slabs) {
                slab::transfer(
                    &self.heap.shared.slabs,
                    &self.owned.slabs,
                    index,
                    None,
                    Some(self.id),
                );

                thread.r#unsized.push(&self.owned.slabs, index);
                crate::fence();

                thread.state.store(None);
                crate::flush(&thread.state, false);
                crate::fence();

                stat::inc(&stat::ALLOCATE_SMALL_GLOBAL);
                break;
            }

            match self.heap.shared.bump(self.id, thread) {
                Some(range) => {
                    stat::inc(&stat::ALLOCATE_SMALL_BUMP);
                    unsafe {
                        self.owned.slabs.link(range.clone(), None);
                        thread
                            .r#unsized
                            .set(Some(range.start), BATCH_BUMP_POP as usize);

                        crate::fence();
                        thread.state.store(None);
                        crate::flush(&thread.state, false);
                        crate::fence();

                        slab::transfer_all(
                            &self.heap.shared.slabs,
                            &self.owned.slabs,
                            range.start,
                            BATCH_BUMP_POP as usize,
                            None,
                            Some(self.id),
                        );
                    }
                    break;
                }
                None => {
                    todo!()
                }
            }
        }

        // FIXME: optimize cold paths to move to sized instead of unsized
        thread.unsized_to_sized(&self.owned.slabs, &self.heap.shared.slabs, self.id, class);
        thread.r#sized[class].peek()
    }
}

impl<'raw> Allocator<'raw> {
    pub unsafe fn root_untyped(&self, root: root::Index) -> Option<NonNull<ffi::c_void>> {
        let offset = self.heap.shared[root].load()?;
        Some(self.heap.offset_to_pointer(offset))
    }

    pub unsafe fn set_root_untyped(
        &self,
        root: root::Index,
        pointer: Option<NonNull<ffi::c_void>>,
    ) {
        let offset = pointer.map(|pointer| self.heap.pointer_to_offset(pointer));
        self.heap.shared[root].store(offset);
    }

    pub unsafe fn realloc_untyped(
        &mut self,
        old_pointer: NonNull<ffi::c_void>,
        new_size: usize,
    ) -> *mut ffi::c_void {
        let old_size = self.heap.class(old_pointer);

        if old_size >= new_size {
            return old_pointer.as_ptr();
        }

        let new_pointer = self.allocate_untyped(new_size);
        core::ptr::copy_nonoverlapping::<u8>(
            old_pointer.as_ptr().cast(),
            new_pointer.cast(),
            old_size,
        );

        self.free_untyped(old_pointer);
        new_pointer
    }

    #[inline]
    pub unsafe fn allocate_untyped(&mut self, size: usize) -> *mut ffi::c_void {
        stat::inc(&stat::ALLOCATE);

        let class = size::Class::new(size);

        let class = match class {
            None => {
                stat::inc(&stat::ALLOCATE_LARGE);
                return self
                    .heap
                    .shared
                    .allocate_log(self.heap.state, self.heap.data.huge(), size)
                    .as_ptr()
                    .cast();
            }
            Some(class) => class,
        };

        stat::record_small(class);

        let index = match self.owned.meta.r#sized[class].peek() {
            Some(index) => {
                stat::inc(&stat::ALLOCATE_FAST);
                index
            }
            None => match self.allocate_small(class) {
                Some(index) => index,
                None => return core::ptr::null_mut(),
            },
        };

        let free = unsafe { &mut *self.owned.slabs[index].free.get() };
        let block = free.peek();

        self.owned
            .meta
            .state
            .store(Some(State::SizedToApplication { index, block }));
        crate::flush(&self.owned.meta.state, false);
        crate::fence();

        free.unset(block);
        crate::flush(free, false);

        if free.is_empty() {
            self.detach(class);
        }

        crate::fence();
        self.owned.meta.state.store(None);
        let offset = unsafe { index.offset_block(class, block) };
        self.heap.offset_to_pointer::<ffi::c_void>(offset).as_ptr()
    }

    #[cold]
    fn detach(&mut self, class: size::Class) {
        let index = self.owned.meta.r#sized[class]
            .pop(&self.owned.slabs)
            .unwrap();

        let shared = &self.heap.shared.slabs[index];
        if !shared.free.is_empty() {
            stat::inc(&stat::ALLOCATE_FAST_DISOWN);
            let owner = shared.owner.load();
            shared
                .owner
                .store(slab::shared::Owner::new(owner.class(), None));
            crate::flush(&shared.owner, false);
            self.transfer(index, Some(self.id), None);
        } else {
            stat::inc(&stat::ALLOCATE_FAST_DETACH);
        }

        if cfg!(feature = "validate") {
            assert!(self.owned.meta.r#sized[class]
                .trace(&self.owned.slabs)
                .all(|other| other != index));
        }
    }

    #[inline]
    pub unsafe fn free_untyped(&mut self, pointer: NonNull<ffi::c_void>) {
        stat::inc(&stat::FREE);

        if pointer.as_ptr() >= self.heap.data.huge().as_ptr().cast::<ffi::c_void>()
            && pointer.as_ptr()
                < self
                    .heap
                    .data
                    .huge()
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(1 << 40)
        {
            stat::inc(&stat::FREE_LARGE);
            return self
                .heap
                .shared
                .free_log(self.heap.state, self.heap.data.huge(), pointer);
        }

        let offset = self.heap.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);

        let shared = &self.heap.shared.slabs[index];
        let owner = shared.owner.load();
        let class = owner.class();

        if owner.id() != Some(self.id) {
            return self.free_remote(offset, index, class);
        }

        stat::inc(&stat::FREE_FAST);
        let slab = &self.owned.slabs[index];
        let block = offset.index_block(class);
        let free = &mut *slab.free.get();

        self.owned
            .meta
            .state
            .store(Some(State::ApplicationToSized { index, block }));

        crate::flush(&self.owned.meta, false);
        crate::fence();

        let count = free.len();
        free.set(block);
        crate::flush(&free, false);

        match count {
            count if count + 1 == class.count() => {
                stat::inc(&stat::FREE_FAST_UNSIZED);
                self.owned
                    .meta
                    .sized_to_unsized(&self.owned.slabs, class, index);

                crate::fence();
                self.owned.meta.state.store(None);

                return self.unsized_to_global();
            }
            0 => self.attach(class, index),
            _ => (),
        }

        crate::fence();
        self.owned.meta.state.store(None);
    }

    #[cold]
    fn attach(&mut self, class: size::Class, index: slab::Index) {
        if cfg!(feature = "validate") {
            assert!(self.owned.meta.r#sized[class]
                .trace(&self.owned.slabs)
                .all(|other| other != index));
        }

        self.owned.meta.r#sized[class].push(&self.owned.slabs, index);
        stat::inc(&stat::FREE_FAST_ATTACH);
    }

    #[cold]
    unsafe fn free_remote(&mut self, offset: slab::Offset, index: slab::Index, class: size::Class) {
        stat::inc(&stat::FREE_REMOTE);

        let slab = &self.heap.shared.slabs[index];
        let block = offset.index_block(class);
        let version = slab.meta.load().version();

        slab.free.set(block);

        if slab.free.is_full(class.count()) {
            self.claim(index, version);
        }
    }

    #[cold]
    fn claim(&mut self, index: slab::Index, version: Version) {
        stat::inc(&stat::FREE_REMOTE_GLOBAL);

        let slab = &self.heap.shared.slabs[index];

        // Note: must use version from *before* we set our bit,
        // or else the full slab becomes globally visible and
        // some other thread can update the version.
        let old = slab::shared::Meta::new(version, slab.meta.load().claim());
        let new = slab::shared::Meta::new(version.next(), Some(self.id));

        match slab.meta.compare_exchange(old, new) {
            Ok(_) => stat::inc(&stat::FREE_REMOTE_GLOBAL_WIN),
            Err(_) => {
                stat::inc(&stat::FREE_REMOTE_GLOBAL_LOSE);
                return;
            }
        }

        slab.free.clear();

        if cfg!(feature = "validate") {
            assert!(
                self.owned
                    .meta
                    .r#unsized
                    .trace(&self.owned.slabs)
                    .all(|other| other != index),
                "Claim does not introduce alias",
            );
        }

        let victim = slab.owner.load().id();

        self.transfer(index, victim, Some(self.id));

        if victim.is_some() {
            stat::inc(&stat::FREE_REMOTE_GLOBAL_WIN_STEAL);
            slab.owner.store(slab::shared::Owner::new(
                size::Class::default(),
                Some(self.id),
            ));
        }

        self.owned.meta.r#unsized.push(&self.owned.slabs, index);
        self.unsized_to_global();
    }

    fn unsized_to_global(&mut self) {
        let count = self.owned.meta.r#unsized.len();
        if count < COUNT_CACHE_SLAB {
            return;
        }

        let mut iter = self
            .owned
            .meta
            .r#unsized
            .trace(&self.owned.slabs)
            .inspect(|index| self.transfer(*index, Some(self.id), None))
            .take(BATCH_GLOBAL_PUSH);

        let head = iter.next().unwrap();
        let tail = iter.last().unwrap();
        let next = self.owned.slabs[tail].meta.load().next();

        self.owned
            .meta
            .state
            .store(Some(State::LocalToGlobalSave { index: head }));
        crate::flush(&self.owned.meta.state, false);
        crate::fence();

        self.owned
            .meta
            .r#unsized
            .set(next, count - BATCH_GLOBAL_PUSH);

        crate::flush(&self.owned.meta.r#unsized, false);

        self.heap
            .shared
            .push(self.id, self.owned.meta, &self.owned.slabs, head, tail);

        crate::fence();
        self.owned.meta.state.store(None);
    }

    #[inline]
    fn transfer(&self, index: slab::Index, old: Option<thread::Id>, new: Option<thread::Id>) {
        slab::transfer(&self.heap.shared.slabs, &self.owned.slabs, index, old, new);
    }
}

#[cfg(feature = "extend")]
impl<'raw> Allocator<'raw> {
    pub fn extend(&mut self) {
        todo!()
    }

    pub fn epoch(&self) -> crate::extend::Epoch {
        self.heap.shared.epoch()
    }
}
