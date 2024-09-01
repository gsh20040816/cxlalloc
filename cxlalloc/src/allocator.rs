use core::alloc::Layout;

use crate::link;
use crate::raw;
use crate::size;
use crate::slab;
use crate::thread;
use crate::Heap;

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

    pub fn allocate_at<'root, T: Default, L: link::Erase<'raw, 'root>>(
        &mut self,
        link: L,
    ) -> &'root mut T {
        let layout = Layout::new::<T>();
        let class = size::Class::new(layout.pad_to_align().size());

        let class = match class {
            size::Class::Small(small) => small,
            size::Class::Large(_) => unimplemented!(),
        };

        let site = link.erase(&self.heap);
        let thread = &mut self.heap.owned.threads[&mut self.id];

        let (slab, block) = loop {
            if let Some(slab) = thread.r#sized[class].peek_mut() {
                let free = &mut self.heap.owned.slabs[&mut *slab];
                let block = free.peek().unwrap();
                break (slab, block);
            }

            if slab::LocalStack::r#move(
                &self.heap.owned.slabs,
                &mut thread.r#unsized,
                &mut thread.r#sized[class],
            ) {
                continue;
            }

            // Transfer from global stack to sized stack

            // Transfer from length expansion to unsized stack
        };

        todo!()
    }
}
