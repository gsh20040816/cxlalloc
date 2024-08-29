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
