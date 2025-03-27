use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use std::ffi::CString;
use std::io;

use allocator_bench::allocator::Config;
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

unsafe impl Sync for Backend {}

impl allocator_bench::allocator::Backend for Backend {
    type Allocator = Boost;
    fn create(config: &Config, name: &str) -> io::Result<Self> {
        unsafe {
            let shm = shm::Raw::new(
                Some(config.numa),
                CString::new(name).unwrap(),
                config.size,
                config.populate,
            )?;
            let inner = sys::managed_create(shm.address_mut().cast(), config.size);
            Ok(Self { shm, inner })
        }
    }

    fn open(config: &Config, name: &str) -> io::Result<Self> {
        unsafe {
            let shm = shm::Raw::new(
                Some(config.numa),
                CString::new(name).unwrap(),
                config.size,
                config.populate,
            )?;
            let inner = sys::managed_open(shm.address_mut().cast(), config.size);
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
    type Handle = NonNull<ffi::c_void>;

    #[inline]
    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::managed_allocate(self.inner(), size).cast()) }
    }

    #[inline]
    unsafe fn deallocate(&mut self, handle: NonNull<ffi::c_void>) {
        sys::managed_deallocate(self.inner(), handle.as_ptr().cast())
    }

    #[inline]
    unsafe fn handle_to_offset(&mut self, handle: &NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(sys::managed_address_to_handle(
            self.inner(),
            handle.as_ptr().cast(),
        ))
        .unwrap()
    }

    #[inline]
    fn offset_to_handle(&mut self, offset: NonZeroU64) -> NonNull<ffi::c_void> {
        unsafe {
            NonNull::new(sys::managed_handle_to_address(self.inner(), offset.get()).cast()).unwrap()
        }
    }

    #[inline]
    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        unsafe {
            NonZeroU64::new(sys::managed_address_to_handle(
                self.inner(),
                pointer.as_ptr().cast(),
            ))
            .unwrap()
        }
    }
}

impl Boost {
    #[inline]
    fn inner(&self) -> *mut sys::ManagedExternalBuffer {
        self.0.as_ref().unwrap() as *const _ as *mut _
    }
}
