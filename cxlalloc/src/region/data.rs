use core::alloc::Layout;
use core::marker::PhantomData;
use core::num::NonZeroUsize;
use core::ptr::NonNull;

use crate::block;
use crate::raw;
use crate::size;
use crate::slab;
use crate::SIZE_SLAB;

pub(crate) struct Data<'raw> {
    base: NonNull<u64>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<'raw> Data<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::array::<[u8; SIZE_SLAB]>(slab_count).unwrap()
    }

    pub(crate) unsafe fn from_raw(region: &'raw raw::heap::Inner) -> Self {
        Self {
            base: NonNull::new(region.data.base().as_ptr().wrapping_byte_sub(SIZE_SLAB)).unwrap(),
            _raw: PhantomData,
        }
    }

    pub(crate) fn offset_to_pointer<T>(&self, offset: Offset) -> NonNull<T> {
        NonNull::new(
            self.base
                .as_ptr()
                .wrapping_byte_add(SIZE_SLAB)
                .wrapping_byte_add(offset.0.get())
                .cast(),
        )
        .unwrap()
    }

    pub(crate) fn pointer_to_offset<T>(&self, pointer: NonNull<T>) -> Offset {
        NonZeroUsize::new(pointer.as_ptr() as usize - self.base.as_ptr() as usize)
            .map(Offset)
            .unwrap()
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Offset(NonZeroUsize);

impl Offset {
    // TOOD: safer interface?
    pub(crate) unsafe fn from_slab_block(
        slab: slab::Index,
        block: block::Index,
        class: size::Small,
    ) -> Self {
        NonZeroUsize::new(slab.to_offset().get() + block.to_offset(class))
            .map(Offset)
            .unwrap()
    }
}
