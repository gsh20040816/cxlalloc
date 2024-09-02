use core::marker::PhantomData;

use crate::region;
use crate::Allocator;
use crate::COUNT_ROOT;
use crate::COUNT_THREAD;

#[repr(C)]
#[derive(Debug)]
pub struct Root<'raw, T> {
    index: Index,
    _heap: PhantomData<&'raw ()>,
    _type: PhantomData<T>,
}

impl<'raw, T> Root<'raw, T> {
    pub(crate) fn index(&self) -> Index {
        self.index
    }

    pub(crate) unsafe fn new(_: &Allocator<'raw>, index: Index) -> Self {
        Self {
            index,
            _heap: PhantomData,
            _type: PhantomData,
        }
    }
}

/// Type-erased root representation for heap internals.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Index(usize);

impl Index {
    #[track_caller]
    pub const fn new(index: usize) -> Self {
        match Self::checked_new(index) {
            Some(root) => root,
            None => panic!("Root index out of bounds"),
        }
    }

    pub const fn checked_new(index: usize) -> Option<Self> {
        if index < COUNT_THREAD {
            Some(Index(index))
        } else {
            None
        }
    }
}

impl From<Index> for usize {
    fn from(index: Index) -> Self {
        index.0
    }
}

pub(crate) struct Array([Option<region::data::Offset>; COUNT_ROOT]);

impl core::ops::Index<Index> for Array {
    type Output = Option<region::data::Offset>;
    fn index(&self, index: Index) -> &Self::Output {
        &self.0[index.0]
    }
}
