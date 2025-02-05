use std::ffi::CString;

use cxlalloc_static::cxlalloc_free;
use cxlalloc_static::cxlalloc_init;
use cxlalloc_static::cxlalloc_init_backend;
use cxlalloc_static::cxlalloc_malloc;
use cxlalloc_static::cxlalloc_offset_to_pointer;
use cxlalloc_static::cxlalloc_pointer_to_offset;

pub struct Cxlalloc;

impl process_bench::Allocator for Cxlalloc {
    fn open(name: &str, size: usize, id: u64) -> Self {
        unsafe {
            let name = CString::new(name).unwrap();
            cxlalloc_init_backend(c"shm".as_ptr());
            cxlalloc_init(name.as_ptr(), size, id as u8, 255, 0, 0);
            Self
        }
    }

    fn allocate(&mut self, size: usize) -> *mut core::ffi::c_void {
        unsafe { cxlalloc_malloc(size) }
    }

    unsafe fn deallocate(&mut self, pointer: *mut core::ffi::c_void) {
        cxlalloc_free(pointer)
    }

    unsafe fn address_to_offset(&mut self, address: *mut core::ffi::c_void) -> u64 {
        let mut offset = 0;
        cxlalloc_pointer_to_offset(address, &mut offset);
        offset
    }

    fn offset_to_address(&mut self, offset: u64) -> *mut core::ffi::c_void {
        cxlalloc_offset_to_pointer(offset)
    }
}
