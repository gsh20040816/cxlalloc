use core::cell::UnsafeCell;
use core::ffi;
use core::ptr::NonNull;
use std::ffi::CString;
use std::sync::Arc;

#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_boost.rs"));
}

#[derive(Clone)]
pub struct Boost {
    name: CString,
    inner: Arc<UnsafeCell<sys::wrap_rbtree>>,
}

unsafe impl Send for Boost {}
unsafe impl Sync for Boost {}

impl allocator_bench::Backend for Boost {
    type Allocator = Self;
    fn open(name: &str, size: usize) -> Self {
        unsafe {
            let name = CString::new(name).unwrap();
            let inner = Arc::new(UnsafeCell::new(sys::wrap_open(name.as_ptr(), size)));
            Self { name, inner }
        }
    }

    fn unlink(self) {
        unsafe {
            libc::shm_unlink(self.name.as_ptr());
        }
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        self.clone()
    }
}

impl allocator_bench::Allocator for Boost {
    type Ptr = NonNull<ffi::c_void>;

    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::wrap_allocate(self.inner.get(), size)) }
    }

    unsafe fn deallocate(&mut self, pointer: NonNull<ffi::c_void>) {
        sys::wrap_deallocate(self.inner.get(), pointer.as_ptr())
    }

    unsafe fn pointer_to_offset(&mut self, pointer: NonNull<ffi::c_void>) -> u64 {
        sys::wrap_address_to_handle(self.inner.get(), pointer.as_ptr())
    }

    fn offset_to_pointer(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        unsafe { NonNull::new(sys::wrap_handle_to_address(self.inner.get(), offset)) }
    }

    fn set_root(&mut self, pointer: Self::Ptr) {
        unsafe { sys::wrap_set_root(self.inner.get(), pointer.as_ptr()) };
    }

    fn get_root(&mut self) -> Option<Self::Ptr> {
        unsafe { NonNull::new(sys::wrap_get_root(self.inner.get())) }
    }
}
