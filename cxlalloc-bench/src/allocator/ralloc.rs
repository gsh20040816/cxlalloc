use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use std::ffi::CString;
use std::ffi::OsStr;
use std::io;

use allocator_bench::allocator::Config;

#[expect(unused)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_ralloc.rs"));
}

pub struct Backend(String);

pub struct Ralloc;

impl allocator_bench::allocator::Backend for Backend {
    type Allocator = Ralloc;

    fn create(config: &Config, name: &str) -> io::Result<Self> {
        unsafe {
            let name = CString::new(name).unwrap();
            // FIXME: hacky workaround for now, since ralloc
            // maps several different files
            assert!(!config.populate);
            std::env::set_var("CXL_NUMA_NODE", config.numa.to_string());
            sys::RP_init(name.as_ptr(), config.size as u64);
        }

        Ok(Self(name.to_owned()))
    }

    fn open(config: &Config, name: &str) -> io::Result<Self> {
        unsafe {
            let name = CString::new(name).unwrap();
            sys::RP_init(name.as_ptr(), config.size as u64);
        }

        Ok(Self(name.to_owned()))
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        Ralloc
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

impl allocator_bench::Allocator for Ralloc {
    type Handle = NonNull<ffi::c_void>;

    #[inline]
    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::RP_malloc(size)) }
    }

    #[inline]
    unsafe fn deallocate(&mut self, handle: NonNull<ffi::c_void>) {
        sys::RP_free(handle.as_ptr())
    }

    #[inline]
    unsafe fn handle_to_offset(&mut self, handle: &NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(sys::RP_pointer_to_offset(handle.as_ptr()) as u64).unwrap()
    }

    #[inline]
    fn offset_to_handle(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        NonNull::new(unsafe { sys::RP_offset_to_pointer(offset as usize) })
    }
}
