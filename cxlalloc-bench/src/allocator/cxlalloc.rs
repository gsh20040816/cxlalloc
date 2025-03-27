use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use std::ffi::OsStr;
use std::io;

use allocator_bench::allocator::Config;

pub struct Backend(String);

pub struct Cxlalloc;

impl allocator_bench::allocator::Backend for Backend {
    type Allocator = Cxlalloc;

    fn open(config: &Config, name: &str) -> io::Result<Self> {
        cxlalloc_global::initialize_process(
            cxlalloc_global::Builder::default()
                .backend(cxlalloc_global::backend::Shm {
                    numa: Some(config.numa),
                    populate: config.populate,
                })
                .size_small(config.size / 2)
                .size_large(config.size / 2),
            name,
        );

        Ok(Self(name.to_owned()))
    }

    fn allocator(&self, thread_id: usize) -> Cxlalloc {
        cxlalloc_global::initialize_thread(thread_id.try_into().unwrap());
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
    type Handle = NonNull<ffi::c_void>;

    #[inline]
    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        cxlalloc_global::allocate_untyped(size)
    }

    #[inline]
    unsafe fn deallocate(&mut self, handle: NonNull<ffi::c_void>) {
        cxlalloc_global::deallocate_untyped(handle.as_ptr())
    }

    #[inline]
    unsafe fn handle_to_offset(&mut self, handle: &NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(cxlalloc_global::pointer_to_offset(*handle) as u64 + 1).unwrap()
    }

    #[inline]
    fn offset_to_handle(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        Some(cxlalloc_global::offset_to_pointer(offset as usize - 1))
    }

    #[inline]
    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(cxlalloc_global::pointer_to_offset(pointer) as u64 + 1).unwrap()
    }
}
