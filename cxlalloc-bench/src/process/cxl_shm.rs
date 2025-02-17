use core::hash::Hash as _;
use core::hash::Hasher;
use core::mem::MaybeUninit;
use std::hash::DefaultHasher;

use allocator_bench::Pointer as _;
use sys::cxl_shm_cxl_shm1;
use sys::cxl_shm_thread_init;
use sys::CXLRef_s_get_addr;

#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_cxl_shm.rs"));
}

pub struct Backend {
    id: i32,
    size: usize,
}

pub struct CxlShm(sys::cxl_shm);

impl allocator_bench::Backend for Backend {
    type Allocator = CxlShm;

    fn open(name: &str, size: usize) -> Self {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let key = hasher.finish() % 64;
        unsafe {
            Self {
                id: libc::shmget(key as i32, size, libc::IPC_CREAT | 0o666),
                size,
            }
        }
    }

    fn unlink(self) {
        unsafe {
            libc::shmctl(
                self.id,
                libc::IPC_RMID,
                &mut std::mem::zeroed::<libc::shmid_ds>(),
            );
        }
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        unsafe {
            let mut cxl_shm: MaybeUninit<sys::cxl_shm> = MaybeUninit::uninit();
            cxl_shm_cxl_shm1(cxl_shm.as_mut_ptr(), self.size as u64, self.id);
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
