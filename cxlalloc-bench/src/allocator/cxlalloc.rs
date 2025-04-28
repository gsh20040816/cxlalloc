use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use std::ffi::OsStr;
use std::io;

use shm_bench::allocator;
use bon::Builder;
use clap::Parser;
use serde::Deserialize;
use serde::Serialize;

pub struct Backend(String);

pub struct Cxlalloc;

#[derive(Builder, Clone, Debug, Deserialize, Serialize, Parser)]
pub struct Config {
    cache_local: usize,
    batch_global: usize,
    batch_bump: usize,
}

impl shm_bench::allocator::Backend for Backend {
    type Allocator = Cxlalloc;
    type Config = Config;

    fn new(
        create: bool,
        config: &allocator::Config<Self::Config>,
        name: &str,
    ) -> anyhow::Result<Self> {
        if create {
            unlink(name)?;
        }

        cxlalloc_global::initialize_process(
            cxlalloc_global::Raw::builder()
                .backend(
                    cxlalloc_global::backend::Backend::builder()
                        .backend(cxlalloc_global::backend::Shm)
                        .maybe_numa(config.numa.clone())
                        .maybe_populate(config.populate)
                        .build(),
                )
                .size_small(config.size / 2)
                .size_large(config.size / 2)
                .cache_local(config.inner.cache_local)
                .batch_global(config.inner.batch_global)
                .batch_bump(config.inner.batch_bump),
            name,
        );

        Ok(Self(name.to_owned()))
    }

    fn allocator(&self, thread_id: usize) -> Cxlalloc {
        cxlalloc_global::initialize_thread(thread_id.try_into().unwrap());
        Cxlalloc
    }

    fn unlink(self) -> anyhow::Result<()> {
        unlink(&self.0)?;
        Ok(())
    }

    #[cfg(feature = "stat-event")]
    fn report(&self) -> serde_json::Value {
        serde_json::to_value(cxlalloc_global::report_process()).unwrap()
    }
}

impl shm_bench::Allocator for Cxlalloc {
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
    fn offset_to_handle(&mut self, offset: NonZeroU64) -> NonNull<ffi::c_void> {
        cxlalloc_global::offset_to_pointer(offset.get() as usize - 1)
    }

    #[inline]
    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(cxlalloc_global::pointer_to_offset(pointer) as u64 + 1).unwrap()
    }

    #[cfg(feature = "stat-event")]
    fn report(&self) -> serde_json::Value {
        serde_json::to_value(cxlalloc_global::report_thread()).unwrap()
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
