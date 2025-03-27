use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::io;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

#[derive(Builder, Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    /// NUMA node for remote memory
    pub numa: usize,

    /// Initial heap size
    pub size: usize,

    /// Eagerly populate page tables
    pub populate: bool,

    pub consistency: Consistency,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consistency {
    None,

    /// Only sfence
    Sfence,

    /// Only clflush
    Clflush,

    /// (clflush or clwb) and sfence
    Clflushopt,
}

pub trait Backend: Sync + Sized {
    type Allocator: Allocator;
    fn create(config: &Config, name: &str) -> io::Result<Self> {
        Self::open(config, name)
    }

    fn open(config: &Config, name: &str) -> io::Result<Self>;

    fn unlink(self) -> io::Result<()>;
    fn allocator(&self, thread_id: usize) -> Self::Allocator;
}

pub trait Allocator: Sized {
    type Handle: Handle;
    fn allocate(&mut self, size: usize) -> Option<Self::Handle>;

    unsafe fn link(&mut self, pointer: *mut u64, pointee: &Self::Handle) {
        unsafe {
            let offset = self.handle_to_offset(pointee);
            AtomicU64::from_ptr(pointer).store(offset.get(), Ordering::Release);
        }
    }

    unsafe fn unlink(&mut self, pointer: *mut u64) {
        let offset = unsafe { AtomicU64::from_ptr(pointer) }.load(Ordering::Relaxed);
        let Some(handle) = self.offset_to_handle(offset) else {
            return;
        };
        unsafe { self.deallocate(handle) }
    }

    unsafe fn deallocate(&mut self, handle: Self::Handle);
    unsafe fn handle_to_offset(&mut self, handle: &Self::Handle) -> NonZeroU64;
    fn offset_to_handle(&mut self, offset: u64) -> Option<Self::Handle>;

    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64;
}

pub trait Handle {
    fn as_ptr(&self) -> *mut ffi::c_void;
}

impl Handle for *mut ffi::c_void {
    fn as_ptr(&self) -> *mut ffi::c_void {
        *self
    }
}

impl Handle for NonNull<ffi::c_void> {
    fn as_ptr(&self) -> *mut ffi::c_void {
        (*self).as_ptr()
    }
}

/// For testing and debugging purposes.
///
/// Will not work across processes.
pub struct Libc;

impl Backend for Libc {
    type Allocator = Self;

    fn open(_config: &Config, _name: &str) -> io::Result<Self> {
        Ok(Self)
    }

    fn unlink(self) -> io::Result<()> {
        Ok(())
    }

    fn allocator(&self, _thread_id: usize) -> Self::Allocator {
        Self
    }
}

impl Allocator for Libc {
    type Handle = NonNull<ffi::c_void>;

    fn allocate(&mut self, size: usize) -> Option<Self::Handle> {
        NonNull::new(unsafe { libc::malloc(size) })
    }

    unsafe fn deallocate(&mut self, handle: Self::Handle) {
        unsafe { libc::free(handle.as_ptr()) }
    }

    unsafe fn handle_to_offset(&mut self, handle: &Self::Handle) -> NonZeroU64 {
        NonZeroU64::new(handle.as_ptr() as u64).unwrap()
    }

    fn offset_to_handle(&mut self, offset: u64) -> Option<Self::Handle> {
        NonNull::new(offset as *mut ffi::c_void)
    }

    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(pointer.as_ptr() as u64).unwrap()
    }
}
