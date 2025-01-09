use core::alloc::Layout;
use core::marker::PhantomData;
use core::num::NonZeroUsize;
use core::ptr::NonNull;

use crate::raw;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;

pub(crate) struct Data<'raw, B> {
    pub(crate) base: NonNull<u64>,
    _raw: PhantomData<&'raw raw::Region>,
    _bracket: PhantomData<B>,
}

impl<'raw> Data<'raw, size::Small>
where
    size::Small: size::Bracket,
{
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::array::<u8>(size::Small::SIZE_SLAB * slab_count).unwrap()
    }

    pub(crate) unsafe fn from_raw(region: &'raw raw::heap::Heap) -> Self {
        Self {
            base: NonNull::new(
                region
                    .data
                    .base()
                    .as_ptr()
                    .wrapping_byte_sub(size::Small::SIZE_SLAB),
            )
            .unwrap(),
            _raw: PhantomData,
            _bracket: PhantomData,
        }
    }
}

impl<B> Data<'_, B>
where
    B: size::Bracket,
{
    pub(crate) fn offset_to_pointer<T>(&self, offset: slab::Offset) -> NonNull<T> {
        unsafe { self.base.byte_add(NonZeroUsize::from(offset).get()) }.cast()
    }

    pub(crate) fn checked_offset_to_offset(&self, offset: usize) -> Option<slab::Offset> {
        let offset = offset + B::SIZE_SLAB;
        // FIXME: check epoch
        if offset > crate::raw::region::RESERVATION * 2 {
            None
        } else {
            unsafe { NonZeroUsize::new(offset).map(|offset| slab::Offset::new(offset)) }
        }
    }

    pub(crate) fn checked_pointer_to_offset<T>(&self, pointer: NonNull<T>) -> Option<slab::Offset> {
        // FIXME: check epoch
        if pointer.as_ptr().cast::<u64>() < self.base.as_ptr().wrapping_byte_add(B::SIZE_SLAB) {
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

#[ribbit::pack(size = 64, nonzero, new(rename = "new_internal", vis = ""))]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Offset(NonZeroU64);
