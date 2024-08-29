use core::marker::PhantomData;
use core::ptr::NonNull;

use crate::raw;
use crate::COUNT_THREAD;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Id(usize);

#[repr(C)]
pub(crate) struct Array<T>([T; COUNT_THREAD]);

pub(crate) struct Slice<'raw, T> {
    base: NonNull<T>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<T> Slice<'_, T> {
    pub(crate) unsafe fn from_raw(region: &raw::Region, offset: usize) -> Self {
        let base = region.base().byte_add(offset).as_ptr().cast::<T>();

        Self {
            base: NonNull::new(base).unwrap(),
            _raw: PhantomData,
        }
    }
}
