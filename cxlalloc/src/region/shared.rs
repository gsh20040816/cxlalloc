use core::alloc::Layout;

use crate::raw;
use crate::root;
use crate::slab;
use crate::SIZE_PAGE;

pub(crate) struct Shared<'raw> {
    pub(crate) capacity: usize,
    pub(crate) meta: &'raw Meta,
    pub(crate) slabs: slab::Slice<'raw, slab::Shared>,
}

impl<'raw> Shared<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::new::<Meta>()
            .extend(Layout::array::<slab::Shared>(slab_count).unwrap())
            .unwrap()
            .0
            .align_to(SIZE_PAGE)
            .unwrap()
            .pad_to_align()
    }

    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner) -> Self {
        // FIXME: deduplicate with `layout`
        let offset = Layout::new::<Meta>()
            .extend(Layout::array::<slab::Shared>(1).unwrap())
            .unwrap()
            .1;

        Self {
            capacity: raw.capacity,
            meta: raw.shared.base().cast::<Meta>().as_ref(),
            slabs: slab::Slice::from_raw(&raw.shared, offset),
        }
    }
}

#[repr(C)]
pub(crate) struct Meta {
    pub(crate) roots: root::Array,
    pub(crate) free: slab::GlobalStack,
    pub(crate) extent: Extent,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Extent {
    epoch: u8,
    length: usize,
}
