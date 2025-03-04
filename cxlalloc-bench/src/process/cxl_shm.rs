use core::mem::MaybeUninit;
use core::num::NonZeroU64;
use std::ffi::CString;
use std::io;

use sys::cxl_shm_cxl_shm2;
use sys::cxl_shm_thread_init;
use sys::CXLRef_s_get_addr;

#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_cxl_shm.rs"));
}

pub struct Backend(shm::Raw);

unsafe impl Send for Backend {}
unsafe impl Sync for Backend {}

pub struct CxlShm(sys::cxl_shm);

impl allocator_bench::Backend for Backend {
    type Allocator = CxlShm;

    fn open(numa: usize, populate: bool, name: &str, size: usize) -> io::Result<Self> {
        shm::Raw::new(Some(numa), CString::new(name).unwrap(), size, populate).map(Self)
    }

    fn unlink(mut self) -> io::Result<()> {
        self.0.unlink()
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        unsafe {
            let mut cxl_shm: MaybeUninit<sys::cxl_shm> = MaybeUninit::uninit();
            cxl_shm_cxl_shm2(
                cxl_shm.as_mut_ptr(),
                self.0.size() as u64,
                self.0.address_mut(),
            );
            cxl_shm_thread_init(cxl_shm.as_mut_ptr());
            CxlShm(cxl_shm.assume_init())
        }
    }
}

impl allocator_bench::Allocator for CxlShm {
    type Ptr = sys::CXLRef;

    fn allocate(&mut self, size: usize) -> Option<Self::Ptr> {
        unsafe { Some(self.0.cxl_malloc(size as u64, 0)) }
    }

    unsafe fn link(&mut self, pointer: *mut u64, pointee: &Self::Ptr) {
        unsafe {
            let offset = self.pointer_to_offset(pointee);
            self.0.link_reference(pointer, offset.get());
        }
    }

    unsafe fn deallocate(&mut self, _: Self::Ptr) {}

    unsafe fn pointer_to_offset(&mut self, pointer: &Self::Ptr) -> NonZeroU64 {
        let address = sys::CXLRef_s_get_addr(pointer as *const Self::Ptr as *mut _);
        // The `link_reference` and `get_ref` functions expect the offset of the
        // `CXLObj` header, *not* the data.
        NonZeroU64::new(address as u64 - self.0.get_start() as u64 - 24).unwrap()
    }

    fn offset_to_pointer(&mut self, offset: u64) -> Option<Self::Ptr> {
        unsafe { Some(self.0.get_ref(offset)) }
    }
}

impl allocator_bench::Pointer for sys::CXLRef {
    fn as_ptr(&self) -> *mut core::ffi::c_void {
        unsafe { CXLRef_s_get_addr(self as *const _ as *mut _) }
    }
}

impl Drop for sys::CXLRef {
    fn drop(&mut self) {
        unsafe { self.destruct() }
    }
}
