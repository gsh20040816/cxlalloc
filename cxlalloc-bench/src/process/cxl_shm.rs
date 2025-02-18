use core::ffi;
use core::mem::MaybeUninit;
use std::ffi::CString;

use allocator_bench::Pointer as _;
use sys::cxl_shm_cxl_shm2;
use sys::cxl_shm_thread_init;
use sys::CXLRef_s_get_addr;

#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_cxl_shm.rs"));
}

pub struct Backend {
    name: CString,
    size: usize,
    address: *mut ffi::c_void,
}

unsafe impl Send for Backend {}
unsafe impl Sync for Backend {}

pub struct CxlShm(sys::cxl_shm);

impl allocator_bench::Backend for Backend {
    type Allocator = CxlShm;

    fn open(node: usize, name: &str, size: usize) -> Self {
        let name = CString::new(name).unwrap();
        let address = super::open(node, &name, size).unwrap();
        Self {
            name: name.to_owned(),
            size,
            address,
        }
    }

    fn unlink(self) {
        super::unlink(&self.name).unwrap();
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        unsafe {
            let mut cxl_shm: MaybeUninit<sys::cxl_shm> = MaybeUninit::uninit();
            cxl_shm_cxl_shm2(cxl_shm.as_mut_ptr(), self.size as u64, self.address);
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

    unsafe fn pointer_to_offset(&mut self, mut pointer: Self::Ptr) -> u64 {
        pointer.get_addr() as u64 - sys::cxl_shm_get_start(&mut self.0) as u64
    }

    fn offset_to_pointer(&mut self, _: u64) -> Option<Self::Ptr> {
        unimplemented!()
    }

    fn set_root(&mut self, pointer: Self::Ptr) {
        unsafe {
            sys::cxl_shm_set_root(&mut self.0, pointer);
        }
    }

    fn get_root(&mut self) -> Option<Self::Ptr> {
        unsafe {
            match sys::cxl_shm_get_root(&mut self.0) {
                null if null.as_ptr().is_null() => None,
                pointer => Some(pointer),
            }
        }
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
