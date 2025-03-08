use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::io;

pub trait Backend: Send + Sync + Sized {
    type Allocator: Allocator;
    fn create(numa: usize, populate: bool, name: &str, size: usize) -> io::Result<Self> {
        Self::open(numa, populate, name, size)
    }

    fn open(numa: usize, populate: bool, name: &str, size: usize) -> io::Result<Self>;

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

    unsafe fn deallocate(&mut self, handle: Self::Handle);
    unsafe fn handle_to_offset(&mut self, handle: &Self::Handle) -> NonZeroU64;
    fn offset_to_handle(&mut self, offset: u64) -> Option<Self::Handle>;
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
