use core::hash::Hash as _;
use core::hash::Hasher;
use core::mem::MaybeUninit;
use std::hash::DefaultHasher;

use sys::cxl_shm_cxl_shm1;
use sys::cxl_shm_thread_init;

#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind-cxlmalloc.rs"));
}

pub struct Backend {
    id: i32,
    size: usize,
}

pub struct Cxlmalloc(sys::cxl_shm);

impl allocator_bench::Backend for Backend {
    type Allocator = Cxlmalloc;

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

    fn allocator(&self, _: usize) -> Self::Allocator {
        unsafe {
            let mut cxl_shm: MaybeUninit<sys::cxl_shm> = MaybeUninit::uninit();
            cxl_shm_cxl_shm1(cxl_shm.as_mut_ptr(), self.size as u64, self.id);
            cxl_shm_thread_init(cxl_shm.as_mut_ptr());
            Cxlmalloc(cxl_shm.assume_init())
        }
    }
}

impl allocator_bench::Allocator for Cxlmalloc {
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
        todo!()
    }
}
