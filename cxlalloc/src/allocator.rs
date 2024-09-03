use core::alloc::Layout;
use core::ffi;
use core::ptr::NonNull;

use crate::block;
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
    // 4096 * 64 * 4096 = 2**(12 + 6 + 12) = 1 GiB?
    owned: block::Set<4096>,
    heap: Heap<'raw>,
}

impl<'raw> Allocator<'raw> {
    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner, mut id: thread::Id) -> Self {
        let owned = block::Set::default();
        let heap = Heap::from_raw(raw);
        let thread = &heap.owned.meta[&mut id];

        //Recover state of owned set
        size::Small::all()
            .flat_map(|class| thread.r#sized[class].trace(&heap.owned.slabs))
            .chain(thread.r#unsized.trace(&heap.owned.slabs))
            .for_each(|index| {
                owned.set(block::Index::new(index.to_offset().get() / SIZE_SLAB));
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

        let index = self.allocate(class);
        let slab = &self.heap.owned.slabs[index];
        let block = slab.free.peek().unwrap();
        let offset = unsafe { region::data::Offset::from_slab_block(index, block, class) };
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

    pub unsafe fn allocate_untyped(&mut self, size: usize) -> NonNull<ffi::c_void> {
        let class = match size::Class::new(size) {
            size::Class::Small(small) => small,
            size::Class::Large(_) => unimplemented!(),
        };

        let index = self.allocate(class);
        let slab = &self.heap.owned.slabs[index];
        let block = slab.free.peek().unwrap();
        let offset = unsafe { region::data::Offset::from_slab_block(index, block, class) };
        slab.free.clear(block);

        match slab.free.len() {
            0 => todo!("pop from sized"),
            _ => (),
        }

        self.heap.offset_to_pointer::<ffi::c_void>(offset)

        // TODO: link
    }

    fn allocate(&mut self, class: size::Small) -> slab::Index {
        let thread = &mut self.heap.owned.meta[&mut self.id];
        loop {
            if let Some(index) = thread.r#sized[class].peek() {
                break index;
            }

            if thread.unsized_to_sized(&self.heap.owned.slabs, class) {
                continue;
            }

            // TODO: Transfer from global stack to sized stack

            // Transfer from length expansion to unsized stack
            let stage = &self.heap.shared[&self.id];
            let version = stage
                .store_versioned::<region::meta::shared::Extent>(None)
                .version();

            // TODO: log capsule boundary

            const COUNT: u16 = 4;
            let length = self
                .heap
                .shared
                .allocate(&mut self.id, COUNT, Some(version))
                .unwrap()
                .length();

            unsafe {
                self.heap.owned.slabs.link(length - COUNT as u32..length);
                thread.r#unsized.set_raw(length - COUNT as u32);
                for i in length - COUNT as u32..length {
                    self.owned.set(block::Index::new(i as usize + 1));
                }
            }
        }
    }

    pub unsafe fn free_untyped(&mut self, pointer: NonNull<ffi::c_void>) {
        let offset = self.heap.pointer_to_offset(pointer);
        let index = offset.to_slab();

        if self
            .owned
            .get(block::Index::new(index.to_offset().get() / SIZE_SLAB))
        {
            let slab = &self.heap.owned.slabs[index];
            let meta = slab.meta.load();
            let class = meta.class();
            let block = offset.to_block(index, class);
            slab.free.set(block);
            match slab.free.len() {
                1 => todo!("push to sized"),
                len if len == class.count() => {
                    self.heap.owned.meta[&mut self.id].sized_to_unsized(
                        &self.heap.owned.slabs,
                        class,
                        index,
                    );
                }
                _ => (),
            }
        } else {
            todo!("remote free")
        }
    }
}
