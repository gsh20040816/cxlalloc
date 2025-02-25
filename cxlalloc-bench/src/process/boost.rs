use core::ffi;
use core::ptr::NonNull;
use std::ffi::CString;
use std::io;

use cxx::SharedPtr;

#[cxx::bridge]
mod sys {

    unsafe extern "C++" {
        include!("cxlalloc-bench/src/cpp/boost.hpp");

        type ManagedExternalBuffer;

        unsafe fn managed_open(
            buffer: *mut c_char,
            size: usize,
        ) -> SharedPtr<ManagedExternalBuffer>;

        unsafe fn managed_create(
            buffer: *mut c_char,
            size: usize,
        ) -> SharedPtr<ManagedExternalBuffer>;

        unsafe fn managed_allocate(buffer: *mut ManagedExternalBuffer, size: usize) -> *mut c_char;
        unsafe fn managed_deallocate(buffer: *mut ManagedExternalBuffer, pointer: *mut c_char);

        unsafe fn managed_address_to_handle(
            buffer: *mut ManagedExternalBuffer,
            pointer: *mut c_char,
        ) -> u64;

        unsafe fn managed_handle_to_address(
            buffer: *mut ManagedExternalBuffer,
            handle: u64,
        ) -> *mut c_char;
    }
}

pub struct Backend {
    shm: shm::Raw,
    inner: SharedPtr<sys::ManagedExternalBuffer>,
}

pub struct Boost(SharedPtr<sys::ManagedExternalBuffer>);

unsafe impl Send for Backend {}
unsafe impl Sync for Backend {}

impl allocator_bench::Backend for Backend {
    type Allocator = Boost;
    fn create(node: usize, name: &str, size: usize) -> io::Result<Self> {
        unsafe {
            let shm = shm::Raw::new(Some(node), CString::new(name).unwrap(), size)?;
            let inner = sys::managed_create(shm.address_mut().cast(), size);
            Ok(Self { shm, inner })
        }
    }

    fn open(node: usize, name: &str, size: usize) -> io::Result<Self> {
        unsafe {
            let shm = shm::Raw::new(Some(node), CString::new(name).unwrap(), size)?;
            let inner = sys::managed_open(shm.address_mut().cast(), size);
            Ok(Self { shm, inner })
        }
    }

    fn unlink(mut self) -> io::Result<()> {
        self.shm.unlink()
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        Boost(self.inner.clone())
    }
}

impl allocator_bench::Allocator for Boost {
    type Ptr = NonNull<ffi::c_void>;

    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::managed_allocate(self.inner(), size).cast()) }
    }

    unsafe fn deallocate(&mut self, pointer: NonNull<ffi::c_void>) {
        sys::managed_deallocate(self.inner(), pointer.as_ptr().cast())
    }

    unsafe fn pointer_to_offset(&mut self, pointer: &NonNull<ffi::c_void>) -> u64 {
        sys::managed_address_to_handle(self.inner(), pointer.as_ptr().cast())
    }

    fn offset_to_pointer(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::managed_handle_to_address(self.inner(), offset).cast()) }
    }
}

impl Boost {
    fn inner(&self) -> *mut sys::ManagedExternalBuffer {
        self.0.as_ref().unwrap() as *const _ as *mut _
    }
}
