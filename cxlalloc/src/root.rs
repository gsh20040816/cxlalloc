use std::marker::PhantomData;

use crate::COUNT_THREAD;

#[repr(C)]
#[derive(Debug)]
pub struct Root<'heap, T> {
    index: Index,
    _heap: PhantomData<&'heap ()>,
    _type: PhantomData<T>,
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
