use core::alloc::Layout;
use core::ffi;

use crate::link;
use crate::raw;
use crate::region;
use crate::root;
use crate::size;
use crate::slab;
use crate::thread;
use crate::Heap;
use crate::Root;

pub struct Allocator<'raw> {
    id: thread::Id,
    heap: Heap<'raw>,
}

impl<'raw> Allocator<'raw> {
    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner, id: thread::Id) -> Self {
        Self {
            id,
            heap: Heap::from_raw(raw),
        }
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

    pub unsafe fn allocate_untyped(&mut self, size: usize) -> *mut ffi::c_void {
        let class = match size::Class::new(size) {
            size::Class::Small(small) => small,
            size::Class::Large(_) => unimplemented!(),
        };

        let index = self.allocate(class);
        let slab = &self.heap.owned.slabs[index];
        let block = slab.free.peek().unwrap();
        let offset = unsafe { region::data::Offset::from_slab_block(index, block, class) };
        slab.free.clear(block);
        self.heap.offset_to_pointer::<ffi::c_void>(offset).as_ptr()

        // TODO: link
    }

    fn allocate(&mut self, class: size::Small) -> slab::Index {
        let thread = &mut self.heap.owned.meta[&mut self.id];
        loop {
            if let Some(index) = thread.r#sized[class].peek() {
                break index;
            }

            if thread.size(&mut self.heap.owned.slabs, class) {
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
            }
        }
    }
}
