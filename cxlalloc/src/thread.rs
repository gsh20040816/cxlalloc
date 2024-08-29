use core::marker::PhantomData;
use core::ops::Index;
use core::ops::IndexMut;
use core::ptr::NonNull;

use crate::raw;
use crate::COUNT_THREAD;

#[derive(Debug, PartialEq, Eq, Hash)]
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

impl<T> Index<&Id> for Slice<'_, T> {
    type Output = T;
    fn index(&self, index: &Id) -> &Self::Output {
        unsafe { self.base.add(index.0).as_ref() }
    }
}

impl<T> Index<&mut Id> for Slice<'_, T> {
    type Output = T;
    fn index(&self, index: &mut Id) -> &Self::Output {
        unsafe { self.base.add(index.0).as_ref() }
    }
}

impl<T> IndexMut<&mut Id> for Slice<'_, T> {
    fn index_mut(&mut self, index: &mut Id) -> &mut Self::Output {
        unsafe { self.base.add(index.0).as_mut() }
    }
}
