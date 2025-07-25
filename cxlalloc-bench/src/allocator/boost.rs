use core::ffi;
use core::num::NonZeroU64;
use core::ptr::NonNull;

use cxx::SharedPtr;
use shm_bench::allocator::Config;

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

impl shm_bench::allocator::Backend for Backend {
    type Allocator = Boost;
    type Config = ();

    fn new(create: bool, config: &Config<Self::Config>, name: &str) -> anyhow::Result<Self> {
        unsafe {
            let shm = shm::Raw::builder()
                .maybe_numa(config.numa.clone())
                .name(name.to_owned())
                .size(config.size)
                .maybe_populate(config.populate)
                .build()?;

            let open = match create {
                true => sys::managed_create,
                false => sys::managed_open,
            };

            let inner = open(shm.address().as_ptr().cast(), config.size);

            Ok(Self { shm, inner })
        }
    }

    fn categorize(&self, mapping: &shm_bench::Mapping) -> Option<shm_bench::allocator::Memory> {
        (mapping.start == self.shm.address().as_ptr().addr())
            .then_some(shm_bench::allocator::Memory::Hwcc)
    }

    fn unlink(mut self) -> anyhow::Result<()> {
        self.shm.unlink()?;
        Ok(())
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        Boost(self.inner.clone())
    }
}

impl shm_bench::Allocator for Boost {
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
