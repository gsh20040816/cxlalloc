use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::num::Wrapping;
use core::ptr::NonNull;

use crate::block;
use crate::raw;
use crate::size;

#[derive(Debug)]
#[repr(C)]
pub(crate) struct Index(NonZeroU32);

pub(crate) struct Slice<'raw, M> {
    base: NonNull<Slab<M>>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<M> Slice<'_, M> {
    // Implementation detail: store minus one
    pub(crate) unsafe fn from_raw(region: &raw::Region, offset: usize) -> Self {
        let base = region
            .base()
            .byte_add(offset)
            .as_ptr()
            .cast::<Slab<M>>()
            .wrapping_sub(1);

        Self {
            base: NonNull::new(base).unwrap(),
            _raw: PhantomData,
        }
    }
}

impl<'raw> core::ops::Index<&Index> for Slice<'raw, Owned> {
    type Output = Owned;
    fn index(&self, index: &Index) -> &Self::Output {
        unsafe { self.base.add(1 + index.0.get() as usize).cast().as_ref() }
    }
}

impl<'raw> core::ops::Index<&mut Index> for Slice<'raw, Owned> {
    type Output = block::Set<7>;
    fn index(&self, index: &mut Index) -> &Self::Output {
        unsafe {
            self.base
                .add(1 + index.0.get() as usize)
                .byte_add(8)
                .cast()
                .as_ref()
        }
    }
}

impl<'raw> core::ops::IndexMut<&mut Index> for Slice<'raw, Owned> {
    fn index_mut(&mut self, index: &mut Index) -> &mut Self::Output {
        unsafe {
            self.base
                .add(1 + index.0.get() as usize)
                .byte_add(8)
                .cast()
                .as_mut()
        }
    }
}

#[repr(C, align(64))]
pub(crate) struct Slab<M> {
    meta: M,
    free: block::Set<7>,
}

#[repr(C)]
pub(crate) struct Owned {
    next: Option<Index>,
    class: size::Small,
}

#[repr(C)]
pub(crate) struct Shared {
    version: Wrapping<u16>,
    class: size::Small,
}

#[repr(C)]
pub(crate) struct LocalStack {
    head: Option<Index>,
}

#[repr(C)]
pub(crate) struct GlobalStack;
