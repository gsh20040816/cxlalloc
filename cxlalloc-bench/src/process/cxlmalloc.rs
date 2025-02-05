use core::hash::Hash as _;
use core::hash::Hasher;
use core::mem::MaybeUninit;
use std::hash::DefaultHasher;

use sys::cxl_shm_cxl_malloc;
use sys::cxl_shm_cxl_shm1;
use sys::cxl_shm_thread_init;

#[expect(dead_code)]
#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind-cxlmalloc.rs"));
}

pub struct Cxlmalloc(sys::cxl_shm);

impl process_bench::Allocator for Cxlmalloc {
    fn open(name: &str, size: usize, _: u64) -> Self {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let key = hasher.finish() % 64;
        unsafe {
            let mut cxl_shm: MaybeUninit<sys::cxl_shm> = MaybeUninit::uninit();
            let shmid = libc::shmget(key as i32, size, libc::IPC_CREAT | 0o666);
            cxl_shm_cxl_shm1(cxl_shm.as_mut_ptr(), size as u64, shmid);
            cxl_shm_thread_init(cxl_shm.as_mut_ptr());
            Self(cxl_shm.assume_init())
        }
    }

    fn allocate(&mut self, size: usize) -> *mut core::ffi::c_void {
        unsafe { cxl_shm_cxl_malloc(&mut self.0, size as u64, 0).get_addr() }
    }

    unsafe fn deallocate(&mut self, _: *mut core::ffi::c_void) {}

    unsafe fn address_to_offset(&mut self, address: *mut core::ffi::c_void) -> u64 {
        address as u64 - sys::cxl_shm_get_start(&mut self.0) as u64
    }

    fn offset_to_address(&mut self, offset: u64) -> *mut core::ffi::c_void {
        unsafe { sys::cxl_shm_get_start(&mut self.0).wrapping_byte_add(offset as usize) }
    }
}
