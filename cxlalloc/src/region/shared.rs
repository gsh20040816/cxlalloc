use core::alloc::Layout;

use crate::raw;
use crate::region::data;
use crate::slab;
use crate::COUNT_ROOT;
use crate::SIZE_PAGE;

pub(crate) struct Shared<'raw> {
    capacity: usize,
    meta: &'raw Meta,
    slabs: slab::Array<slab::Shared>,
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

    pub(crate) fn from_raw(raw: &'raw raw::heap::Inner) -> Self {
        // FIXME: deduplicate with `layout`
        let offset = Layout::new::<Meta>()
            .extend(Layout::array::<slab::Shared>(1).unwrap())
            .unwrap()
            .1;

        Self {
            capacity: raw.capacity,
            meta: unsafe { raw.shared.base().cast::<Meta>().as_ref() },
            slabs: unsafe { slab::Array::from_raw(raw.shared.base().byte_add(offset).cast()) },
        }
    }

    pub(crate) fn meta(&self) -> &Meta {
        self.meta
    }
}

#[repr(C)]
pub(crate) struct Meta {
    roots: [Option<data::Offset>; COUNT_ROOT],
    free: slab::GlobalStack,
    extent: Extent,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Extent {
    epoch: u8,
    length: usize,
}
