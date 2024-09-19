use core::ptr::NonNull;

use crate::raw;
use crate::region;
use crate::slab;
use crate::Root;

pub struct Heap<'raw> {
    pub(crate) shared: region::Shared<'raw>,
    pub(crate) data: region::Data<'raw>,
}

impl<'raw> Heap<'raw> {
    pub(crate) unsafe fn from_raw(heap: &'raw raw::heap::Inner) -> Self {
        Heap {
            shared: region::Shared::from_raw(heap),
            data: region::Data::from_raw(heap),
        }
    }

    pub fn offset_to_pointer<T>(&self, offset: slab::Offset) -> NonNull<T> {
        self.data.offset_to_pointer(offset)
    }

    pub fn pointer_to_offset<T>(&self, pointer: NonNull<T>) -> slab::Offset {
        self.data.pointer_to_offset(pointer)
    }

    pub fn class<T>(&self, pointer: NonNull<T>) -> usize {
        let offset = self.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);
        self.shared.slabs[index].owner.load().class().size()
    }

    pub unsafe fn root<'root, T>(&self, root: &'root Root<'raw, T>) -> Option<&'root T> {
        let index = root.index();
        let offset = self.shared[index]?;
        Some(self.offset_to_pointer(offset).as_ref())
    }

    pub unsafe fn root_mut<'root, T>(
        &self,
        root: &'root mut Root<'raw, T>,
    ) -> Option<&'root mut T> {
        let index = root.index();
        let offset = self.shared[index]?;
        Some(self.offset_to_pointer(offset).as_mut())
    }
}
