use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use std::ffi::CString;
use std::ffi::OsStr;
use std::io;
use std::path::Path;

use shm_bench::allocator::Config;

#[expect(unused)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_ralloc.rs"));
}

pub struct Backend(String);

pub struct Ralloc;

impl shm_bench::allocator::Backend for Backend {
    type Allocator = Ralloc;
    type Config = ();

    fn new(create: bool, config: &Config<Self::Config>, name: &str) -> anyhow::Result<Self> {
        unsafe {
            // FIXME: hacky workaround for now, since ralloc
            // maps several different files
            assert!(config.populate.is_none());
            match &config.numa {
                None => (),
                Some(shm::Numa::Bind { node }) => {
                    std::env::set_var("CXL_NUMA_NODE", node.to_string())
                }
                Some(shm::Numa::Interleave { nodes: _ }) => todo!(),
            }

            if create {
                unlink(name)?;
            }

            let name = CString::new(name).unwrap();
            sys::RP_init(name.as_ptr(), config.size as u64);
        }

        Ok(Self(name.to_owned()))
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        Ralloc
    }

    fn unlink(self) -> anyhow::Result<()> {
        unlink(&self.0)?;
        Ok(())
    }

    fn categorize(&self, mapping: &shm_bench::Mapping) -> Option<shm_bench::allocator::Memory> {
        let name = mapping
            .path
            .as_ref()
            .map(Path::new)
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)?;

        if !name.starts_with(&self.0) {
            return None;
        }

        Some(if name.ends_with("desc") || name.ends_with("basemd") {
            shm_bench::allocator::Memory::Hwcc
        } else {
            shm_bench::allocator::Memory::Swcc
        })
    }
}

impl shm_bench::Allocator for Ralloc {
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
    fn offset_to_handle(&mut self, offset: NonZeroU64) -> NonNull<ffi::c_void> {
        NonNull::new(unsafe { sys::RP_offset_to_pointer(offset.get() as usize) }).unwrap()
    }

    #[inline]
    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(unsafe { sys::RP_pointer_to_offset(pointer.as_ptr()) } as u64).unwrap()
    }
}

fn unlink(prefix: &str) -> io::Result<()> {
    let prefix = prefix.trim_start_matches("/");

    for entry in std::fs::read_dir("/dev/shm")? {
        let entry = entry.unwrap();
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if name.starts_with(prefix) {
            std::fs::remove_file(path)?;
        }
    }

    Ok(())
}
