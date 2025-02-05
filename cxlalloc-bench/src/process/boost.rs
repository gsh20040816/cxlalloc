use std::ffi::CString;

#[expect(dead_code)]
#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind-boost.rs"));
}

pub struct Boost(sys::wrap_rbtree);

impl process_bench::Allocator for Boost {
    fn open(name: &str, size: usize) -> Self {
        unsafe {
            let name = CString::new(name).unwrap();
            Self(sys::wrap_open(name.as_ptr(), size))
        }
    }

    fn allocate(&mut self, size: usize) -> *mut core::ffi::c_void {
        unsafe { sys::wrap_allocate(&mut self.0, size) }
    }

    unsafe fn deallocate(&mut self, pointer: *mut core::ffi::c_void) {
        sys::wrap_deallocate(&mut self.0, pointer)
    }

    unsafe fn address_to_offset(&mut self, address: *mut core::ffi::c_void) -> u64 {
        sys::wrap_address_to_handle(&mut self.0, address)
    }

    fn offset_to_address(&mut self, offset: u64) -> *mut core::ffi::c_void {
        unsafe { sys::wrap_handle_to_address(&mut self.0, offset) }
    }
}
