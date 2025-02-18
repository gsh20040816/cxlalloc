use core::ffi;
use core::ptr::NonNull;
use std::ffi::CString;

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

#[derive(Clone)]
pub struct Boost {
    name: CString,
    inner: SharedPtr<sys::ManagedExternalBuffer>,
}

unsafe impl Send for Boost {}
unsafe impl Sync for Boost {}

impl Boost {
    fn inner(&self) -> *mut sys::ManagedExternalBuffer {
        self.inner.as_ref().unwrap() as *const _ as *mut _
    }
}

impl allocator_bench::Backend for Boost {
    type Allocator = Self;
    fn create(node: usize, name: &str, size: usize) -> Self {
        unsafe {
            let name = CString::new(name).unwrap();
            let address = super::open(node, &name, size).unwrap();
            #[allow(clippy::arc_with_non_send_sync)]
            let inner = sys::managed_create(address.cast(), size);
            Self { name, inner }
        }
    }

    fn open(node: usize, name: &str, size: usize) -> Self {
        unsafe {
            let name = CString::new(name).unwrap();
            let address = super::open(node, &name, size).unwrap();
            #[allow(clippy::arc_with_non_send_sync)]
            let inner = sys::managed_open(address.cast(), size);
            Self { name, inner }
        }
    }

    fn unlink(self) {
        super::unlink(&self.name).unwrap();
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        self.clone()
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

    unsafe fn pointer_to_offset(&mut self, pointer: NonNull<ffi::c_void>) -> u64 {
        sys::managed_address_to_handle(self.inner(), pointer.as_ptr().cast())
    }

    fn offset_to_pointer(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::managed_handle_to_address(self.inner(), offset).cast()) }
    }

    fn set_root(&mut self, _pointer: Self::Ptr) {
        todo!()
    }

    fn get_root(&mut self) -> Option<Self::Ptr> {
        todo!()
    }
}
