use core::ffi;
use core::ptr::NonNull;
use std::ffi::CString;

use cxlalloc_static::cxlalloc_free;
use cxlalloc_static::cxlalloc_init;
use cxlalloc_static::cxlalloc_init_backend;
use cxlalloc_static::cxlalloc_init_thread;
use cxlalloc_static::cxlalloc_malloc;
use cxlalloc_static::cxlalloc_offset_to_pointer;
use cxlalloc_static::cxlalloc_pointer_to_offset;

pub struct Cxlalloc;

impl process_bench::Backend for Cxlalloc {
    type Allocator = Self;

    fn open(name: &str, size: usize) -> Self {
        unsafe {
            let name = CString::new(name).unwrap();
            cxlalloc_init_backend(c"shm".as_ptr());
            cxlalloc_init(name.as_ptr(), size, 0, 255, 0, 0);
            Self
        }
    }

    fn allocator(&self, thread_id: usize) -> Self::Allocator {
        unsafe {
            cxlalloc_init_thread(thread_id);
        }
        Self
    }
}

impl process_bench::Allocator for Cxlalloc {
    type Ptr = NonNull<ffi::c_void>;

    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        NonNull::new(unsafe { cxlalloc_malloc(size) })
    }

    unsafe fn deallocate(&mut self, pointer: NonNull<ffi::c_void>) {
        cxlalloc_free(pointer.as_ptr())
    }

    unsafe fn pointer_to_offset(&mut self, pointer: NonNull<ffi::c_void>) -> u64 {
        let mut offset = 0;
        cxlalloc_pointer_to_offset(pointer.as_ptr(), &mut offset);
        offset
    }

    fn offset_to_pointer(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        NonNull::new(cxlalloc_offset_to_pointer(offset))
    }
}
