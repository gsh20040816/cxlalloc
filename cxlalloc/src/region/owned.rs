use core::alloc::Layout;

use crate::raw;
use crate::size;
use crate::slab;
use crate::thread;
use crate::SIZE_PAGE;

pub(crate) struct Owned<'raw> {
    pub(crate) thread: &'raw mut Thread,
    pub(crate) slabs: slab::Array<slab::Owned>,
}

impl<'raw> Owned<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::new::<thread::Array<Thread>>()
            .extend(Layout::array::<slab::Owned>(slab_count).unwrap())
            .unwrap()
            .0
            .align_to(SIZE_PAGE)
            .unwrap()
            .pad_to_align()
    }

    /// # Safety
    ///
    /// Caller must ensure that no other thread is concurrently using `id`.
    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner, id: &mut thread::Id) -> Self {
        // FIXME: deduplicate with `layout`
        let offset = Layout::new::<thread::Array<Thread>>()
            .extend(Layout::array::<slab::Owned>(1).unwrap())
            .unwrap()
            .1;

        Self {
            thread: unsafe {
                thread::Array::get(raw.owned.base().cast::<thread::Array<Thread>>(), id).as_mut()
            },
            slabs: unsafe { slab::Array::from_raw(raw.owned.base().byte_add(offset).cast()) },
        }
    }
}

#[repr(C, align(64))]
pub(crate) struct Thread {
    pub(crate) state: Option<()>,
    pub(crate) r#unsized: slab::LocalStack,
    pub(crate) r#sized: size::Array<slab::LocalStack>,
}
