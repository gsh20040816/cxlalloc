use core::alloc::Layout;
use core::ffi;
use core::ptr::NonNull;

use crate::extend::Epoch;
use crate::huge;
use crate::link;
use crate::raw;
use crate::region;
use crate::root;
use crate::size;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::Heap;
use crate::Root;
use crate::SIZE_SLAB;

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
        let layout = Layout::new::<T>();
        let class = match size::Class::new(layout.pad_to_align().size()) {
            size::Class::Small(small) => small,
            size::Class::Large(_) => unimplemented!(),
        };

        let index = self.allocate_small(class).unwrap();
        let slab = &self.owned.slabs[index];
        let block = unsafe { slab.free.with(|free| free.peek()) };
        let offset = unsafe { index.offset_block(class, block) };
        let mut pointer = self.heap.offset_to_pointer::<T>(offset);

        unsafe {
            pointer.write(T::default());
        }

        // match link.erase(&self.heap) {
        //     link::Site::Root(index) => todo!(),
        //     link::Site::Data(offset) => todo!(),
        // }

        unsafe { pointer.as_mut() }
    }

    fn allocate_large(&mut self, class: size::Large) -> slab::Index {
        stat::inc(&stat::ALLOCATE_LARGE);

        let index = 'inner: {
            // First try from unsized
            if class.count() == 1 {
                if let Some(index) = self.owned.meta.r#unsized.peek() {
                    self.owned.meta.r#unsized.pop(&self.owned.slabs);
                    stat::inc(&stat::ALLOCATE_LARGE_UNSIZED);
                    break 'inner index;
                }
            }

            let stage = &self.heap.shared[self.id];
            let version = stage
                .store_versioned::<region::shared::Length>(None)
                .version();

            // Then try from global shared
            if class.count() == 1 {
                if let Ok(index) = self
                    .heap
                    .shared
                    .pop(self.id, &self.owned.slabs, Some(version))
                {
                    stat::inc(&stat::ALLOCATE_LARGE_GLOBAL);
                    break 'inner index;
                }
            }

            // Then try from bump pointer
            stat::inc(&stat::ALLOCATE_LARGE_BUMP);
            self.heap
                .shared
                .allocate(
                    self.id,
                    u16::try_from(class.size() / SIZE_SLAB).unwrap(),
                    Some(version),
                )
                .unwrap()
                .start
        };

        self.heap.shared.slabs[index]
            .owner
            .store(slab::shared::Owner::new(
                size::Class::Large(class),
                Some(self.id),
            ));

        index
    }

    #[cold]
    fn allocate_small(&mut self, class: size::Small) -> Option<slab::Index> {
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

        let stage = &self.heap.shared[self.id];
        let version = stage
            .store_versioned::<region::shared::Length>(None)
            .version();

        loop {
            if let Ok(index) = self
                .heap
                .shared
                .pop(self.id, &self.owned.slabs, Some(version))
            {
                self.owned.slabs[index]
                    .meta
                    .store(slab::owned::Meta::new(None));

                // unsized is empty
                thread.r#unsized.set(Some(index));

                stat::inc(&stat::ALLOCATE_SMALL_GLOBAL);
                break;
            }

            const COUNT: u16 = 16;
            match self.heap.shared.allocate(self.id, COUNT, Some(version)) {
                Ok(range) => {
                    stat::inc(&stat::ALLOCATE_SMALL_BUMP);
                    unsafe {
                        self.owned.slabs.link(range.clone(), None);
                        thread.r#unsized.set(Some(range.start));
                    }
                    break;
                }
                Err(epoch) => {
                    let _ = self.heap.shared.extend(self.id, epoch, Some(version));
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
        stat::record(class);

        let class = match class {
            size::Class::Large(class) => return self.malloc_slow_large(class),
            size::Class::Small(class) => class,
        };

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

        let (block, empty) = self.owned.slabs[index].free.with_mut(|free| {
            let block = free.peek();
            free.unset(block);
            (block, free.is_empty())
        });

        if empty {
            self.detach(class);
        }

        let offset = unsafe { index.offset_block(class, block) };
        self.heap.offset_to_pointer::<ffi::c_void>(offset).as_ptr()
    }

    #[cold]
    fn detach(&mut self, class: size::Small) {
        let index = self.owned.meta.r#sized[class]
            .pop(&self.owned.slabs)
            .unwrap();

        ::log::info!("Detaching {} from {}", index, class);

        let shared = &self.heap.shared.slabs[index];
        if !shared.free.is_empty() {
            stat::inc(&stat::ALLOCATE_FAST_DISOWN);
            let owner = shared.owner.load();
            shared
                .owner
                .store(slab::shared::Owner::new(owner.class(), None));
        } else {
            stat::inc(&stat::ALLOCATE_FAST_DETACH);
        }

        if cfg!(feature = "validate") {
            assert!(self.owned.meta.r#sized[class]
                .trace(&self.owned.slabs)
                .all(|other| other != index));
        }
    }

    #[cold]
    unsafe fn malloc_slow_large(&mut self, class: size::Large) -> *mut ffi::c_void {
        if class.size() > 32768 {
            return self
                .heap
                .shared
                .allocate_log(self.heap.state, self.heap.data.huge(), class.size())
                .as_ptr()
                .cast();
        }

        let index = self.allocate_large(class);
        let offset = slab::Offset::from(index);
        self.heap.offset_to_pointer(offset).as_ptr()
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
            return self
                .heap
                .shared
                .free_log(self.heap.state, self.heap.data.huge(), pointer);
        }

        let offset = self.heap.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);

        let shared = &self.heap.shared.slabs[index];
        let owner = shared.owner.load();

        let class = match owner.class() {
            size::Class::Small(small) => small,
            size::Class::Large(large) => {
                return self.free_large(large, index);
            }
        };

        if owner.id() != Some(self.id) {
            return self.free_remote(offset, index, class);
        }

        stat::inc(&stat::FREE_FAST);
        let slab = &self.owned.slabs[index];
        let block = offset.index_block(class);
        let count = slab.free.with_mut(|free| {
            let count = free.len();
            free.set(block);
            count
        });

        match count {
            0 => self.attach(class, index),
            count if count + 1 == class.count() => {
                stat::inc(&stat::FREE_FAST_UNSIZED);
                self.owned
                    .meta
                    .sized_to_unsized(&self.owned.slabs, class, index);
            }
            _ => (),
        }
    }

    #[cold]
    fn attach(&mut self, class: size::Small, index: slab::Index) {
        if cfg!(feature = "validate") {
            assert!(self.owned.meta.r#sized[class]
                .trace(&self.owned.slabs)
                .all(|other| other != index));
        }

        self.owned.meta.r#sized[class].push(&self.owned.slabs, index);

        ::log::info!("Attaching {} to {}", index, class);

        stat::inc(&stat::FREE_FAST_ATTACH);
    }

    #[cold]
    unsafe fn free_large(&mut self, class: size::Large, index: slab::Index) {
        stat::inc(&stat::FREE_LARGE);

        if class.count() == 1 {
            stat::inc(&stat::FREE_LARGE_UNSIZED);
            self.owned.meta.r#unsized.push(&self.owned.slabs, index);
            return;
        }

        let stage = &self.heap.shared[self.id];
        let staged = stage.store_versioned(Some(index)).transpose();

        // TODO: log capsule boundary

        stat::inc(&stat::FREE_LARGE_GLOBAL);
        self.heap
            .shared
            .push(self.id, &self.owned.slabs, class.count() as u16, staged);
    }

    #[cold]
    unsafe fn free_remote(&mut self, offset: slab::Offset, index: slab::Index, class: size::Small) {
        stat::inc(&stat::FREE_REMOTE);

        let slab = &self.heap.shared.slabs[index];

        let block = offset.index_block(class);

        slab.free.set(block);

        if slab.free.is_full(class.count()) {
            self.claim(index);
        }
    }

    #[cold]
    fn claim(&mut self, index: slab::Index) {
        stat::inc(&stat::FREE_REMOTE_GLOBAL);

        let slab = &self.heap.shared.slabs[index];
        let old = slab.meta.load();
        let new = slab::shared::Meta::new(old.version().next(), Some(self.id));

        match slab.meta.compare_exchange(old, new) {
            Ok(_) => stat::inc(&stat::FREE_REMOTE_GLOBAL_WIN),
            Err(_) => {
                stat::inc(&stat::FREE_REMOTE_GLOBAL_LOSE);
                return;
            }
        }

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

        slab.free.clear();
        self.owned.meta.r#unsized.push(&self.owned.slabs, index);
    }
}

#[cfg(feature = "extend")]
impl<'raw> Allocator<'raw> {
    pub fn extend(&mut self) {
        let stage = &self.heap.shared[self.id];
        let version = stage
            .store_versioned::<region::shared::Length>(None)
            .version();

        let epoch = self.heap.shared.epoch();
        let _ = self.heap.shared.extend(self.id, epoch, Some(version));
    }

    pub fn epoch(&self) -> Epoch {
        self.heap.shared.epoch()
    }
}
