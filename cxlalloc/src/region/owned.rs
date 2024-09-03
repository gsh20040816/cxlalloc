use core::alloc::Layout;

use crate::raw;
use crate::size;
use crate::slab;
use crate::thread;
use crate::SIZE_PAGE;

pub(crate) struct Owned<'raw> {
    pub(crate) meta: thread::Slice<'raw, Meta>,
    pub(crate) slabs: slab::Slice<'raw, slab::Owned>,
}

impl<'raw> Owned<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::new::<thread::Array<Meta>>()
            .extend(Layout::array::<slab::Owned>(slab_count).unwrap())
            .unwrap()
            .0
            .align_to(SIZE_PAGE)
            .unwrap()
            .pad_to_align()
    }

    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner) -> Self {
        // FIXME: deduplicate with `layout`
        let (_, offset) = Layout::new::<thread::Array<Meta>>()
            .extend(Layout::array::<slab::Owned>(1).unwrap())
            .unwrap();

        Self {
            meta: thread::Slice::from_raw(&raw.owned, 0),
            slabs: slab::Slice::from_raw(&raw.owned, offset),
        }
    }
}

#[repr(C, align(64))]
pub(crate) struct Meta {
    pub(crate) state: Option<()>,
    pub(crate) r#unsized: slab::LocalStack,
    pub(crate) r#sized: size::Array<slab::LocalStack>,
}

impl Meta {
    pub(crate) fn size(
        &mut self,
        slabs: &mut slab::Slice<slab::Owned>,
        class: size::Small,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        let slab = &slabs[index];
        let next = slab.meta.load().next();
        slab.meta.store(slab::Owned::new(None, class));
        slab.free.fill(class.count());

        self.r#sized[class].set(Some(index));
        self.r#unsized.set(next);

        true
    }
}
