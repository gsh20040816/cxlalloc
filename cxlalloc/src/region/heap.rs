use core::ptr::NonNull;

use crate::raw;
use crate::region;
use crate::size;
use crate::thread;

/// A `Heap` is a thread-agnostic view of the heap; it can
/// satisfy global metadata queries or updates, but not
/// allocation.
pub struct Heap<'raw> {
    pub(crate) shared: region::meta::Shared<'raw>,
    data: region::Data<'raw>,
}

impl<'raw> Heap<'raw> {
    pub(crate) fn from_raw(heap: &'raw raw::heap::Inner) -> Self {
        Heap {
            shared: region::meta::Shared::from_raw(heap),
            data: region::Data::from_raw(heap),
        }
    }

    pub fn offset_to_pointer<T>(&self, offset: region::data::Offset) -> NonNull<T> {
        self.data.offset_to_pointer(offset)
    }
}
