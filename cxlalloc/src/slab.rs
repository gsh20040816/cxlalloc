use core::num::NonZeroU32;
use core::num::Wrapping;
use core::ptr::NonNull;

use crate::block;
use crate::size;

#[derive(Debug)]
#[repr(C)]
pub(crate) struct Index(NonZeroU32);

pub(crate) struct Array<M>(NonNull<Slab<M>>);

impl<M> Array<M> {
    // Implementation detail: store minus one
    pub(crate) unsafe fn from_raw(array: NonNull<Slab<M>>) -> Self {
        Self(NonNull::new(array.as_ptr().wrapping_sub(1)).unwrap())
    }
}

#[repr(C, align(64))]
pub(crate) struct Slab<M> {
    pub(crate) meta: M,
    pub(crate) free: block::Set<7>,
}

#[repr(C)]
pub(crate) struct Owned {
    pub(crate) next: Option<Index>,
    pub(crate) class: size::Small,
}

#[repr(C)]
pub(crate) struct Shared {
    pub(crate) version: Wrapping<u16>,
    pub(crate) class: size::Small,
}

#[repr(C)]
pub(crate) struct LocalStack {
    head: Option<Index>,
}

#[repr(C)]
pub(crate) struct GlobalStack;
