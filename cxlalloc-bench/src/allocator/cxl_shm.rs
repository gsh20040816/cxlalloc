use core::ffi;
use core::mem::MaybeUninit;
use core::num::NonZeroU64;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::ptr::NonNull;

use shm_bench::allocator::Config;
use sys::cxl_shm_cxl_shm2;
use sys::cxl_shm_thread_init;
use sys::CXLRef_s_get_addr;

#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_cxl_shm.rs"));
}

pub struct Backend(shm::Raw);

unsafe impl Sync for Backend {}

pub struct CxlShm(sys::cxl_shm);

impl shm_bench::allocator::Backend for Backend {
    type Allocator = CxlShm;
    type Config = ();

    fn new(create: bool, config: &Config<Self::Config>, name: &str) -> anyhow::Result<Self> {
        shm::Raw::builder()
            .maybe_numa(config.numa.clone())
            .name(name.to_owned())
            .size(config.size)
            .create(create)
            .maybe_populate(config.populate)
            .build()
            .map(Self)
            .map_err(anyhow::Error::from)
    }

    fn categorize(&self, mapping: &shm_bench::Mapping) -> Option<shm_bench::allocator::Memory> {
        (mapping.start == self.0.address().as_ptr().addr())
            .then_some(shm_bench::allocator::Memory::Hwcc)
    }

    fn unlink(mut self) -> anyhow::Result<()> {
        self.0.unlink().map_err(anyhow::Error::from)
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        unsafe {
            let mut cxl_shm: MaybeUninit<sys::cxl_shm> = MaybeUninit::uninit();
            cxl_shm_cxl_shm2(
                cxl_shm.as_mut_ptr(),
                self.0.size().get() as u64,
                self.0.address().as_ptr().cast(),
            );
            cxl_shm_thread_init(cxl_shm.as_mut_ptr());
            CxlShm(cxl_shm.assume_init())
        }
    }
}

impl shm_bench::Allocator for CxlShm {
    type Handle = sys::CXLRef;

    #[inline]
    fn allocate(&mut self, size: usize) -> Option<Self::Handle> {
        unsafe { Some(self.0.cxl_malloc(size as u64, 0)) }
    }

    #[inline]
    unsafe fn link(&mut self, pointer: *mut u64, pointee: &Self::Handle) {
        unsafe {
            let offset = self.handle_to_offset(pointee);
            self.0.link_reference(pointer, offset.get());
        }
    }

    #[inline]
    unsafe fn deallocate(&mut self, _: Self::Handle) {}

    #[inline]
    unsafe fn unlink(&mut self, pointer: *mut u64) {
        let offset = AtomicU64::from_ptr(pointer).load(Ordering::Relaxed);
        self.0.unlink_reference(pointer, offset)
    }

    #[inline]
    unsafe fn handle_to_offset(&mut self, handle: &Self::Handle) -> NonZeroU64 {
        let address = sys::CXLRef_s_get_addr(handle as *const Self::Handle as *mut _);
        // The `link_reference` and `get_ref` functions expect the offset of the
        // `CXLObj` header, *not* the data.
        NonZeroU64::new(address as u64 - self.0.get_start() as u64 - 24).unwrap()
    }

    #[inline]
    fn offset_to_handle(&mut self, offset: NonZeroU64) -> Self::Handle {
        unsafe { self.0.get_ref(offset.get()) }
    }

    #[inline]
    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(
            pointer.as_ptr() as u64
                - unsafe { sys::cxl_shm_get_start(&self.0 as *const _ as *mut _) } as u64
                - 24,
        )
        .unwrap()
    }
}

impl shm_bench::allocator::Handle for sys::CXLRef {
    fn as_ptr(&self) -> *mut ffi::c_void {
        unsafe { CXLRef_s_get_addr(self as *const _ as *mut _) }
    }
}

impl Drop for sys::CXLRef {
    fn drop(&mut self) {
        unsafe { self.destruct() }
    }
}
