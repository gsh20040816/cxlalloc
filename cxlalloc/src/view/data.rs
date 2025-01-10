use core::alloc::Layout;
use core::alloc::LayoutError;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::num::NonZeroU64;
use core::ptr::NonNull;

use crate::bitset::Bit;
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
    pub(crate) fn new(base: NonNull<u64>) -> Self {
        Self {
            base,
            _raw: PhantomData,
            _bracket: PhantomData,
        }
    }

    pub(crate) fn layout(slab_count: usize) -> Result<Layout, LayoutError> {
        Layout::array::<u8>(size::Small::SIZE_SLAB * slab_count)
    }
}

impl<B> Data<'_, B>
where
    B: size::Bracket,
{
    pub(crate) fn offset_to_pointer<T>(&self, offset: Offset<B>) -> NonNull<T> {
        unsafe { self.base.byte_add(NonZeroU64::from(offset).get() as usize) }.cast()
    }

    pub(crate) fn checked_offset_to_offset(&self, offset: usize) -> Option<Offset<B>> {
        let offset = offset + B::SIZE_SLAB;
        // FIXME: check epoch
        if offset > crate::raw::region::RESERVATION.get() * 2 {
            None
        } else {
            NonZeroU64::new(offset as u64).map(|offset| Offset::new_internal(offset))
        }
    }

    pub(crate) fn checked_pointer_to_offset<T>(&self, pointer: NonNull<T>) -> Option<Offset<B>> {
        // FIXME: check epoch
        if pointer.as_ptr().cast::<u64>() < self.base.as_ptr().wrapping_byte_add(B::SIZE_SLAB) {
            None
        } else {
            Some(self.pointer_to_offset(pointer))
        }
    }

    pub(crate) fn pointer_to_offset<T>(&self, pointer: NonNull<T>) -> Offset<B> {
        Offset::new_internal(
            NonZeroU64::new(pointer.as_ptr() as u64 - self.base.as_ptr() as u64).unwrap(),
        )
    }
}

#[ribbit::pack(size = 64, nonzero, new(rename = "new_internal", vis = ""))]
#[repr(transparent)]
#[derive(PartialEq, Eq)]
pub struct Offset<B> {
    value: NonZeroU64,
    #[ribbit(size = 0)]
    _bracket: PhantomData<B>,
}

impl<B: size::Bracket> Offset<B> {
    pub(crate) fn from_block(slab: slab::Index<B>, class: B, block: Bit) -> Self {
        debug_assert!(u64::from(block) <= class.count(), "{} {:?}", class, block);
        NonZeroU64::new(
            NonZeroU32::from(slab).get() as u64 * (B::SIZE_SLAB as u64)
                + u64::from(block) * class.size(),
        )
        .map(Self::new_internal)
        .unwrap()
    }
}

impl<B: size::Bracket> From<slab::Index<B>> for Offset<B> {
    fn from(index: slab::Index<B>) -> Self {
        NonZeroU64::new(NonZeroU32::from(index).get() as u64 * B::SIZE_SLAB as u64)
            .map(Self::new_internal)
            .unwrap()
    }
}

impl<B> From<Offset<B>> for NonZeroU64 {
    fn from(offset: Offset<B>) -> Self {
        offset.value
    }
}

impl<B: size::Bracket> From<Offset<B>> for u64 {
    fn from(offset: Offset<B>) -> Self {
        offset.value.get() + B::SIZE_SLAB as u64
    }
}

impl<B> Clone for Offset<B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B> Copy for Offset<B> {}
