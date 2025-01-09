use core::alloc::Layout;
use core::marker::PhantomData;
use core::num::NonZeroUsize;
use core::ptr::NonNull;

use crate::raw;
use crate::slab;
use crate::SIZE_SLAB;

pub(crate) struct Data<'raw> {
    pub(crate) base: NonNull<u64>,
    huge: NonNull<u64>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<'raw> Data<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::array::<[u8; SIZE_SLAB]>(slab_count).unwrap()
    }

    pub(crate) unsafe fn from_raw(region: &'raw raw::heap::Inner) -> Self {
        Self {
            base: NonNull::new(region.data.base().as_ptr().wrapping_byte_sub(SIZE_SLAB)).unwrap(),
            huge: NonNull::new(
                region
                    .data
                    .base()
                    .as_ptr()
                    .wrapping_byte_add(crate::raw::region::RESERVATION),
            )
            .unwrap(),
            _raw: PhantomData,
        }
    }

    pub(crate) fn huge(&self) -> NonNull<u64> {
        self.huge
    }

    pub(crate) fn offset_to_pointer<T>(&self, offset: slab::Offset) -> NonNull<T> {
        unsafe { self.base.byte_add(NonZeroUsize::from(offset).get()) }.cast()
    }

    pub(crate) fn checked_offset_to_offset(&self, offset: usize) -> Option<slab::Offset> {
        let offset = offset + SIZE_SLAB;
        // FIXME: check epoch
        if offset > crate::raw::region::RESERVATION * 2 {
            None
        } else {
            unsafe { NonZeroUsize::new(offset).map(|offset| slab::Offset::new(offset)) }
        }
    }

    pub(crate) fn checked_pointer_to_offset<T>(&self, pointer: NonNull<T>) -> Option<slab::Offset> {
        // FIXME: check epoch
        if pointer.as_ptr().cast::<u64>() < self.base.as_ptr().wrapping_byte_add(SIZE_SLAB)
            || pointer.as_ptr().cast::<u64>()
                >= self
                    .huge
                    .as_ptr()
                    .wrapping_byte_add(crate::raw::region::RESERVATION)
        {
            None
        } else {
            Some(self.pointer_to_offset(pointer))
        }
    }

    pub(crate) fn pointer_to_offset<T>(&self, pointer: NonNull<T>) -> slab::Offset {
        unsafe {
            slab::Offset::new(NonZeroUsize::new_unchecked(
                pointer.as_ptr() as usize - self.base.as_ptr() as usize,
            ))
        }
    }
}
