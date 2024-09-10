use core::alloc::Layout;
use core::ffi;
use core::num::NonZeroU32;
use core::ptr::NonNull;

use crate::bitset::AtomicBitSet;
use crate::bitset::Bit;
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
    heap: Heap<'raw>,
    owned: AtomicBitSet<8192>,
}

impl<'raw> Allocator<'raw> {
    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner, mut id: thread::Id) -> Self {
        let owned = AtomicBitSet::default();
        let heap = Heap::from_raw(raw);
        let thread = &heap.owned.meta[&mut id];

        //Recover state of owned set
        size::Small::all()
            .flat_map(|class| thread.r#sized[class].trace(&heap.owned.slabs))
            .chain(thread.r#unsized.trace(&heap.owned.slabs))
            .for_each(|index| {
                owned.set(Bit::new(NonZeroU32::from(index).get() as usize));
            });

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
        let slab = &self.heap.owned.slabs[index];
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
        let stage = &self.heap.shared[&self.id];
        let version = stage
            .store_versioned::<region::meta::shared::Extent>(None)
            .version();

        let range = self
            .heap
            .shared
            .allocate(
                &mut self.id,
                u16::try_from(class.size() / SIZE_SLAB).unwrap(),
                Some(version),
            )
            .unwrap();

        self.heap.owned.slabs[range.start]
            .meta
            .store(slab::owned::Meta::new(None, size::Class::Large(class)));

        let version = self.heap.shared.slabs[range.start].meta.load().version();
        self.heap.shared.slabs[range.start]
            .meta
            .store(slab::shared::Meta::new(
                version.next(),
                size::Class::Large(class),
            ));

        range.start
    }

    #[cold]
    fn allocate_small(&mut self, class: size::Small) -> slab::Index {
        let thread = &mut self.heap.owned.meta[&mut self.id];
        loop {
            if thread.unsized_to_sized(&self.heap.owned.slabs, &self.heap.shared.slabs, class) {
                break;
            }

            if !self.heap.shared.is_empty() {
                let stage = &self.heap.shared[&self.id];
                let version = stage
                    .store_versioned::<region::meta::shared::Extent>(None)
                    .version();

                if let Ok(index) =
                    self.heap
                        .shared
                        .pop(&mut self.id, &self.heap.owned.slabs, Some(version))
                {
                    self.heap.owned.slabs[index]
                        .meta
                        .store(slab::owned::Meta::new(
                            None,
                            size::Class::Small(size::Small::default()),
                        ));
                    thread.r#unsized.set(Some(index));
                    self.owned
                        .set(Bit::new(NonZeroU32::from(index).get() as usize));
                    continue;
                }
            }

            // Transfer from length expansion to unsized stack
            let stage = &self.heap.shared[&self.id];
            let version = stage
                .store_versioned::<region::meta::shared::Extent>(None)
                .version();

            // TODO: log capsule boundary

            const COUNT: u16 = 4;
            let range = self
                .heap
                .shared
                .allocate(&mut self.id, COUNT, Some(version))
                .unwrap();

            unsafe {
                self.heap.owned.slabs.link(range.clone(), None);
                thread.r#unsized.set(Some(range.start));
                // FIXME: move ownership and range logic here into slab module
                for i in NonZeroU32::from(range.start).get()..NonZeroU32::from(range.end).get() {
                    self.owned.set(Bit::new(i as usize));
                }
            }
        }

        thread.r#sized[class].peek().unwrap()
    }
}

impl<'raw> Allocator<'raw> {
    pub unsafe fn class_untyped(&self, pointer: NonNull<ffi::c_void>) -> usize {
        let offset = self.heap.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);

        if self
            .owned
            .get(Bit::new(NonZeroU32::from(index).get() as usize))
        {
            self.heap.owned.slabs[index].meta.load().class().size()
        } else {
            self.heap.shared.slabs[index].meta.load().class().size()
        }
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

        let index = match self.heap.owned.meta[&mut self.id].r#sized[class].peek() {
            None => self.allocate_small(class),
            Some(index) => index,
        };

        let slab = &self.heap.owned.slabs[index];
        let block = unsafe { &*slab.free.get() }.peek();

        unsafe { &mut *slab.free.get() }.unset(block);
        if unsafe { &*slab.free.get() }.is_empty() {
            self.disown(class);
        }

        let offset = unsafe { index.offset_block(class, block) };
        self.heap.offset_to_pointer::<ffi::c_void>(offset)
    }

    #[cold]
    fn disown(&mut self, class: size::Small) {
        let index = self.heap.owned.meta[&mut self.id].r#sized[class]
            .pop(&self.heap.owned.slabs)
            .unwrap();
        self.owned
            .unset(Bit::new(NonZeroU32::from(index).get() as usize));
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

        if self
            .owned
            .get(Bit::new(NonZeroU32::from(index).get() as usize))
        {
            let slab = &self.heap.owned.slabs[index];
            let meta = slab.meta.load();
            let class = match meta.class() {
                size::Class::Small(small) => small,
                size::Class::Large(large) => {
                    let stage = &self.heap.shared[&self.id];
                    let staged = stage.store_versioned(Some(index)).transpose();

                    // TODO: log capsule boundary

                    self.heap.shared.push(
                        &mut self.id,
                        &self.heap.owned.slabs,
                        large.count() as u16,
                        staged,
                    );

                    log::info!("Freed local large allocation {:?}", index);
                    return;
                }
            };
            let block = offset.index_block(class);
            let count = unsafe { &*slab.free.get() }.len();

            unsafe { &mut *slab.free.get() }.set(block);

            if count == 0 {
                self.heap.owned.meta[&mut self.id].r#sized[class].push(
                    &self.heap.owned.slabs,
                    index,
                    Some(class),
                );
                self.owned
                    .set(Bit::new(NonZeroU32::from(index).get() as usize));
            } else if count + 1 == class.count() {
                self.heap.owned.meta[&mut self.id].sized_to_unsized(
                    &self.heap.owned.slabs,
                    class,
                    index,
                );
            }
        } else {
            let slab = &self.heap.shared.slabs[index];
            let meta = slab.meta.load();
            let class = match meta.class() {
                size::Class::Small(small) => small,
                size::Class::Large(large) => {
                    let stage = &self.heap.shared[&self.id];
                    let staged = stage.store_versioned(Some(index)).transpose();

                    // TODO: log capsule boundary

                    self.owned
                        .unset(Bit::new(NonZeroU32::from(index).get() as usize));
                    self.heap.shared.push(
                        &mut self.id,
                        &self.heap.owned.slabs,
                        large.count() as u16,
                        staged,
                    );

                    log::info!("Freed remote large allocation {:?} ({})", index, large);
                    return;
                }
            };

            let block = offset.index_block(class);

            // FIXME: use compare_exchange to detect if we are the last writer
            // FIXME: also need ^ to avoid clobbering concurrent writes
            if slab.free.set(block) < 64 || !slab.free.is_full(class.count()) {
                return;
            }

            let version = slab.meta.load().version();
            slab.meta.store(slab::shared::Meta::new(
                version.next(),
                size::Class::Small(size::Small::default()),
            ));
            slab.free.clear();

            let stage = &self.heap.shared[&self.id];
            let staged = stage.store_versioned(Some(index)).transpose();

            self.heap
                .shared
                .push(&mut self.id, &self.heap.owned.slabs, 1, staged);
        }
    }
}
