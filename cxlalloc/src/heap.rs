use core::ptr::NonNull;

use crate::raw;
use crate::region;

pub struct Heap<'raw> {
    shared: region::meta::Shared<'raw>,
    owned: region::meta::Owned<'raw>,
    data: region::Data<'raw>,
}

impl<'raw> Heap<'raw> {
    pub(crate) unsafe fn from_raw(heap: &'raw raw::heap::Inner) -> Self {
        Heap {
            shared: region::meta::Shared::from_raw(heap),
            owned: region::meta::Owned::from_raw(heap),
            data: region::Data::from_raw(heap),
        }
    }

    pub fn offset_to_pointer<T>(&self, offset: region::data::Offset) -> NonNull<T> {
        self.data.offset_to_pointer(offset)
    }
}
