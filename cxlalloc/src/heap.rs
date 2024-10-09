use core::ffi;
use core::ptr::NonNull;
use std::sync::Mutex;

use crate::huge;
use crate::raw;
use crate::region;
use crate::slab;
use crate::Root;

pub struct Heap<'raw> {
    pub(crate) state: &'raw Mutex<huge::Dram>,
    pub(crate) shared: region::Shared<'raw>,
    pub(crate) data: region::Data<'raw>,
}

impl<'raw> Heap<'raw> {
    pub(crate) unsafe fn from_raw(heap: &'raw raw::heap::Inner) -> Self {
        Heap {
            state: &heap.state,
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

    pub fn checked_pointer_to_offset<T>(&self, pointer: NonNull<T>) -> Option<slab::Offset> {
        self.data.checked_pointer_to_offset(pointer)
    }

    pub fn checked_offset_to_offset(&self, offset: usize) -> Option<slab::Offset> {
        self.data.checked_offset_to_offset(offset)
    }

    pub fn class<T>(&self, pointer: NonNull<T>) -> usize {
        if pointer.as_ptr().cast::<ffi::c_void>() >= self.data.huge().as_ptr().cast::<ffi::c_void>()
            && pointer.as_ptr().cast::<ffi::c_void>()
                < self
                    .data
                    .huge()
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(1 << 40)
        {
            return unsafe {
                self.shared
                    .size_log(self.data.huge(), pointer.cast::<ffi::c_void>())
            };
        }

        let offset = self.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);
        self.shared.slabs[index].owner.load().class().size()
    }

    pub unsafe fn root<'root, T>(&self, root: &'root Root<'raw, T>) -> Option<&'root T> {
        let index = root.index();
        let offset = self.shared[index].load()?;
        Some(self.offset_to_pointer(offset).as_ref())
    }

    pub unsafe fn root_mut<'root, T>(
        &self,
        root: &'root mut Root<'raw, T>,
    ) -> Option<&'root mut T> {
        let index = root.index();
        let offset = self.shared[index].load()?;
        Some(self.offset_to_pointer(offset).as_mut())
    }
}
