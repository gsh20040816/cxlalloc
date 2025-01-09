use core::alloc::Layout;
use core::alloc::LayoutError;
use core::marker::PhantomData;
use core::ptr::NonNull;

use crate::raw;
use crate::size;
use crate::slab;

#[ribbit::pack(size = 32, nonzero, new(vis = ""))]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) struct Index(NonZeroU32);

pub(crate) struct Slab<'raw, B> {
    descriptors: Slice<'raw, slab::Descriptor>,
    _raw: PhantomData<&'raw raw::Region>,
    _bracket: PhantomData<B>,
}

impl<B> Slab<'_, B>
where
    B: size::Bracket,
{
    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        Slice::<slab::Descriptor>::layout(count)
    }
}

struct Slice<'raw, T> {
    base: NonNull<T>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<T> Slice<'_, T> {
    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        Layout::array::<T>(count)
    }

    pub(crate) unsafe fn from_raw(region: &raw::Region, offset: usize) -> Self {
        let base = region
            .base()
            .byte_add(offset)
            .as_ptr()
            .cast::<T>()
            // Base pointer is one element before first element
            .wrapping_sub(1);

        Self {
            base: NonNull::new(base).unwrap(),
            _raw: PhantomData,
        }
    }
}

impl<T> core::ops::Index<Index> for Slice<'_, T> {
    type Output = T;
    fn index(&self, index: Index) -> &Self::Output {
        unsafe { self.base.add(index._0().get() as usize).as_ref() }
    }
}
