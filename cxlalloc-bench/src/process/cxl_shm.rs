use core::mem::MaybeUninit;
use std::ffi::CString;
use std::io;

use sys::cxl_shm_cxl_shm2;
use sys::cxl_shm_thread_init;
use sys::CXLRef_s_get_addr;

use crate::MAP_POPULATE;

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

    fn open(node: usize, name: &str, size: usize) -> io::Result<Self> {
        shm::Raw::new(Some(node), CString::new(name).unwrap(), size, *MAP_POPULATE).map(Self)
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

    unsafe fn deallocate(&mut self, mut pointer: Self::Ptr) {
        unsafe { pointer.destruct() }
    }

    unsafe fn pointer_to_offset(&mut self, pointer: &Self::Ptr) -> u64 {
        let address = sys::CXLRef_s_get_addr(pointer as *const Self::Ptr as *mut _);
        address as u64 - sys::cxl_shm_get_start(&mut self.0) as u64
    }

    fn offset_to_pointer(&mut self, _: u64) -> Option<Self::Ptr> {
        unimplemented!()
    }
}

impl allocator_bench::Pointer for sys::CXLRef {
    fn as_ptr(&self) -> *mut core::ffi::c_void {
        unsafe { CXLRef_s_get_addr(self as *const _ as *mut _) }
    }

    fn as_u64(&self) -> u64 {
        (*self).as_ptr() as u64
    }

    fn from_u64(_pointer: u64) -> Self {
        unimplemented!()
    }
}
