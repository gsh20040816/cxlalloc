use core::alloc::Layout;
use core::alloc::LayoutError;
use core::marker::PhantomData;

use crate::raw;
use crate::slab;

pub(crate) struct Slab<'raw, B> {
    descriptors: slab::Slice<'raw, slab::Descriptor>,
    _raw: PhantomData<&'raw raw::Region>,
    _bracket: PhantomData<B>,
}

impl<'raw, B> Slab<'raw, B> {
    pub(crate) fn new(descriptors: slab::Slice<'raw, slab::Descriptor>) -> Self {
        Self {
            descriptors,
            _raw: PhantomData,
            _bracket: PhantomData,
        }
    }

    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        slab::Slice::<slab::Descriptor>::layout(count)
    }
}
