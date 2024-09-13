use core::alloc::Layout;
use core::ffi;
use core::ptr::NonNull;

use crate::link;
use crate::raw;
use crate::region;
use crate::root;
use crate::size;
use crate::slab;
use crate::thread;
use crate::Heap;
use crate::Root;
use crate::SIZE_SLAB;

pub struct Allocator<'raw> {
    id: thread::Id,
    owned: region::meta::Owned<'raw>,
    heap: Heap<'raw>,
}

impl<'raw> Allocator<'raw> {
    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner, id: thread::Id) -> Self {
        let heap = Heap::from_raw(raw);
        let owned = region::meta::Owned::from_raw(raw, id);

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
        link: L,
    ) -> &'root mut T {
        let layout = Layout::new::<T>();
        let class = match size::Class::new(layout.pad_to_align().size()) {
            size::Class::Small(small) => small,
            size::Class::Large(_) => unimplemented!(),
        };

        let index = self.allocate_small(class);
        let slab = &self.owned.slabs[index];
        let block = unsafe { &*slab.free.get() }.peek();
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
        log::info!("malloc large {}", class);

        let index = 'inner: {
            // First try from unsized
            if class.count() == 1 {
                if let Some(index) = self.owned.meta.r#unsized.peek() {
                    self.owned.meta.r#unsized.pop(&self.owned.slabs);
                    break 'inner index;
                }
            }

            let stage = &self.heap.shared[self.id];
            let version = stage
                .store_versioned::<region::meta::shared::Extent>(None)
                .version();

            // Then try from global shared
            if class.count() == 1 {
                if let Ok(index) = self
                    .heap
                    .shared
                    .pop(self.id, &self.owned.slabs, Some(version))
                {
                    break 'inner index;
                }
            }

            // Then try from bump pointer
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
            .store(slab::shared::Owner::new(size::Class::Large(class), self.id));

        index
    }

    #[cold]
    fn allocate_small(&mut self, class: size::Small) -> slab::Index {
        let thread = &mut *self.owned.meta;
        loop {
            if thread.unsized_to_sized(&self.owned.slabs, &self.heap.shared.slabs, self.id, class) {
                break;
            }

            let stage = &self.heap.shared[self.id];
            let version = stage
                .store_versioned::<region::meta::shared::Extent>(None)
                .version();

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

                log::info!(
                    "{:?} allocated from global {:?} ({})",
                    &self.id,
                    index,
                    class
                );
                continue;
            }

            // TODO: log capsule boundary

            const COUNT: u16 = 16;
            let range = self
                .heap
                .shared
                .allocate(self.id, COUNT, Some(version))
                .unwrap();

            unsafe {
                self.owned.slabs.link(range.clone(), None);
                thread.r#unsized.set(Some(range.start));
            }
        }

        thread.r#sized[class].peek().unwrap()
    }
}

impl<'raw> Allocator<'raw> {
    pub unsafe fn class_untyped(&self, pointer: NonNull<ffi::c_void>) -> usize {
        let offset = self.heap.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);
        self.heap.shared.slabs[index].owner.load().class().size()
    }

    pub unsafe fn realloc_untyped(
        &mut self,
        old_pointer: NonNull<ffi::c_void>,
        new_size: usize,
    ) -> NonNull<ffi::c_void> {
        let old_size = self.class_untyped(old_pointer);

        if old_size >= new_size {
            return old_pointer;
        }

        let new_pointer = self.allocate_untyped(new_size);
        core::ptr::copy_nonoverlapping::<u8>(
            old_pointer.as_ptr().cast(),
            new_pointer.as_ptr().cast(),
            old_size,
        );

        self.free_untyped(old_pointer);
        new_pointer
    }

    #[inline]
    pub unsafe fn allocate_untyped(&mut self, size: usize) -> NonNull<ffi::c_void> {
        let class = match size::Class::new(size) {
            size::Class::Large(class) => return self.malloc_slow_large(class),
            size::Class::Small(class) => class,
        };

        let index = match self.owned.meta.r#sized[class].peek() {
            None => {
                let index = self.allocate_small(class);
                log::info!("malloc small slab {} = {:?}", class, index);
                index
            }
            Some(index) => index,
        };

        let slab = &self.owned.slabs[index];
        let free = unsafe { &mut *slab.free.get() };
        let block = free.peek();

        free.unset(block);
        if free.is_empty() {
            self.disown(class);
        }

        let offset = unsafe { index.offset_block(class, block) };
        self.heap.offset_to_pointer::<ffi::c_void>(offset)
    }

    #[cold]
    fn disown(&mut self, class: size::Small) {
        // log::info!("disowning {:?} from {}", index, class);
        // assert!(self.heap.owned.meta[&mut self.id].r#sized[class]
        //     .trace(&self.heap.owned.slabs)
        //     .all(|other| other != index));
        self.owned.meta.r#sized[class]
            .pop(&self.owned.slabs)
            .unwrap();
    }

    #[cold]
    unsafe fn malloc_slow_large(&mut self, class: size::Large) -> NonNull<ffi::c_void> {
        let index = self.allocate_large(class);
        log::info!("malloc large {} = {:?}", class, index);
        let offset = slab::Offset::from(index);
        return self.heap.offset_to_pointer(offset);
    }

    #[inline]
    pub unsafe fn free_untyped(&mut self, pointer: NonNull<ffi::c_void>) {
        let offset = self.heap.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);

        let shared = &self.heap.shared.slabs[index];
        let owner = shared.owner.load();

        let class = match owner.class() {
            size::Class::Small(small) => small,
            size::Class::Large(large) => {
                log::info!("{:?} large allocation {} at {:?}", self.id, large, index);
                return self.free_large(large, index);
            }
        };

        if owner.id() != self.id {
            return self.free_remote(offset, index, class);
        }

        log::info!("{:?} freeing local {:?}", self.id, index);

        let slab = &self.owned.slabs[index];
        let block = offset.index_block(class);
        let count = unsafe { &*slab.free.get() }.len();

        unsafe { &mut *slab.free.get() }.set(block);

        match count {
            0 => self.reown(class, index),
            count if count + 1 == class.count() => {
                // log::info!(
                //     "id = {:?}, free = {} ({}), removing {:?}",
                //     &self.id,
                //     unsafe { &*slab.free.get() }.len(),
                //     class,
                //     index
                // );
                self.owned
                    .meta
                    .sized_to_unsized(&self.owned.slabs, class, index);
            }
            _ => (),
        }
    }

    #[cold]
    fn reown(&mut self, class: size::Small, index: slab::Index) {
        // assert!(self.heap.owned.meta[&mut self.id].r#sized[class]
        //     .trace(&self.heap.owned.slabs)
        //     .all(|other| other != index));
        // log::info!("pushing {:?} onto {}", index, class);
        self.owned.meta.r#sized[class].push(&self.owned.slabs, index);
    }

    #[cold]
    unsafe fn free_large(&mut self, class: size::Large, index: slab::Index) {
        log::info!("Freed large allocation {:?}", index);
        if class.count() == 1 {
            self.owned.meta.r#unsized.push(&self.owned.slabs, index);
            return;
        }

        let stage = &self.heap.shared[self.id];
        let staged = stage.store_versioned(Some(index)).transpose();

        // TODO: log capsule boundary

        self.heap
            .shared
            .push(self.id, &self.owned.slabs, class.count() as u16, staged);
    }

    #[cold]
    unsafe fn free_remote(&mut self, offset: slab::Offset, index: slab::Index, class: size::Small) {
        let slab = &self.heap.shared.slabs[index];

        log::info!("free remote {:?} {}", index, class);

        let block = offset.index_block(class);

        // FIXME: use compare_exchange to detect if we are the last writer
        // FIXME: also need ^ to avoid clobbering concurrent writes
        if slab.free.set_atomic(block) == 64 && slab.free.is_full(class.count()) {
            self.transfer(index);
        }
    }

    #[cold]
    fn transfer(&mut self, index: slab::Index) {
        let slab = &self.heap.shared.slabs[index];
        let version = slab.meta.load().version();

        slab.meta.store(slab::shared::Meta::new(version.next()));
        slab.free.clear();

        let stage = &self.heap.shared[self.id];
        let staged = stage.store_versioned(Some(index)).transpose();

        self.heap.shared.push(self.id, &self.owned.slabs, 1, staged);
    }
}
