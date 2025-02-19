use core::ffi;
use core::ptr::NonNull;
use std::ffi::CString;
use std::ffi::OsStr;
use std::io;

use cxlalloc_static::cxlalloc_free;
use cxlalloc_static::cxlalloc_init;
use cxlalloc_static::cxlalloc_init_backend;
use cxlalloc_static::cxlalloc_init_thread;
use cxlalloc_static::cxlalloc_malloc;
use cxlalloc_static::cxlalloc_offset_to_pointer;
use cxlalloc_static::cxlalloc_pointer_to_offset;

pub struct Backend(String);

pub struct Cxlalloc;

impl allocator_bench::Backend for Backend {
    type Allocator = Cxlalloc;

    // FIXME: implicitly passed through `CXL_NUMA_NODE` environment variable
    fn open(_: usize, name: &str, size: usize) -> io::Result<Self> {
        unsafe {
            let name = CString::new(name).unwrap();
            cxlalloc_init_backend(c"shm".as_ptr());
            cxlalloc_init(name.as_ptr(), size, 0, 255, 0, 0);
        }
        Ok(Self(name.to_owned()))
    }

    fn allocator(&self, thread_id: usize) -> Cxlalloc {
        unsafe {
            cxlalloc_init_thread(thread_id);
        }
        Cxlalloc
    }

    fn unlink(self) -> io::Result<()> {
        for entry in std::fs::read_dir("/dev/shm")? {
            let entry = entry.unwrap();
            let path = entry.path();
            let Some(name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if name.starts_with(&self.0) {
                std::fs::remove_file(path)?;
            }
        }

        Ok(())
    }
}

impl allocator_bench::Allocator for Cxlalloc {
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
