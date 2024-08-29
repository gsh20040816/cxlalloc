use core::ops::Index;
use core::ptr::NonNull;

use crate::COUNT_THREAD;

#[repr(C)]
pub(crate) struct Array<T>([T; COUNT_THREAD]);

impl<T> Array<T> {
    pub(crate) unsafe fn get(pointer: NonNull<Array<T>>, id: &mut Id) -> NonNull<T> {
        pointer.byte_add(id.0).cast()
    }
}

impl<T> Index<Id> for Array<T> {
    type Output = T;
    fn index(&self, index: Id) -> &Self::Output {
        &self.0[index.0]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Id(usize);
