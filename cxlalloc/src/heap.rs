use crate::cas::help;
use crate::size;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::view::Heap;
use crate::BATCH_BUMP_POP;

impl<'raw, B: size::Bracket> Heap<'raw, view::Focus, B> {
    pub(crate) fn allocate(
        &mut self,
        id: thread::Id,
        help: &help::Array,
        class: B,
    ) -> Option<slab::Index> {
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
}
