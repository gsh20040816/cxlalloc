use core::cell::UnsafeCell;
use core::ffi;
use core::ptr::NonNull;
use std::ffi::CString;
use std::sync::Arc;

#[expect(dead_code)]
#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind-boost.rs"));
}

pub struct Boost(Arc<UnsafeCell<sys::wrap_rbtree>>);

unsafe impl Send for Boost {}
unsafe impl Sync for Boost {}

impl allocator_bench::Backend for Boost {
    type Allocator = Self;
    fn open(name: &str, size: usize) -> Self {
        unsafe {
            let name = CString::new(name).unwrap();
            Self(Arc::new(UnsafeCell::new(sys::wrap_open(
                name.as_ptr(),
                size,
            ))))
        }
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        Self(Arc::clone(&self.0))
    }
}

impl allocator_bench::Allocator for Boost {
    type Ptr = NonNull<ffi::c_void>;

    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::wrap_allocate(self.0.get(), size)) }
    }

    unsafe fn deallocate(&mut self, pointer: NonNull<ffi::c_void>) {
        sys::wrap_deallocate(self.0.get(), pointer.as_ptr())
    }

    unsafe fn pointer_to_offset(&mut self, pointer: NonNull<ffi::c_void>) -> u64 {
        sys::wrap_address_to_handle(self.0.get(), pointer.as_ptr())
    }

    fn offset_to_pointer(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::wrap_handle_to_address(self.0.get(), offset)) }
    }
}
