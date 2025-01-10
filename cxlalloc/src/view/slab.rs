use core::alloc::Layout;
use core::alloc::LayoutError;
use core::marker::PhantomData;
use core::ops::Deref;

use crate::raw;
use crate::slab;

pub(crate) struct Slab<'raw, B> {
    descriptors: slab::Slice<'raw, B>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<'raw, B> Slab<'raw, B> {
    pub(crate) fn new(descriptors: slab::Slice<'raw, B>) -> Self {
        Self {
            descriptors,
            _raw: PhantomData,
        }
    }

    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        slab::Slice::<B>::layout(count)
    }
}

impl<'raw, B> Deref for Slab<'raw, B> {
    type Target = slab::Slice<'raw, B>;
    fn deref(&self) -> &Self::Target {
        &self.descriptors
    }
}
