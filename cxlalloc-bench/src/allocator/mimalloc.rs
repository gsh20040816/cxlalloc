use core::ffi;
use core::mem::MaybeUninit;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use std::ffi::CString;
use std::io;

use allocator_bench::allocator::Config;

#[expect(unused)]
#[expect(non_camel_case_types)]
#[expect(non_upper_case_globals)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_mimalloc.rs"));
}

pub struct Backend {
    raw: shm::Raw,
    arena: sys::mi_arena_id_t,
}

unsafe impl Sync for Backend {}

pub struct Mimalloc(*mut sys::mi_heap_t);

impl allocator_bench::allocator::Backend for Backend {
    type Allocator = Mimalloc;

    fn open(config: &Config, name: &str) -> io::Result<Self> {
        let raw = shm::Raw::new(
            Some(config.numa),
            CString::new(name).unwrap(),
            config.size,
            config.populate,
        )?;

        let arena = unsafe {
            let mut arena = MaybeUninit::<sys::mi_arena_id_t>::zeroed();
            sys::mi_manage_os_memory_ex(
                raw.address_mut(),
                raw.size(),
                false,
                false,
                true,
                config.numa as i32,
                true,
                arena.as_mut_ptr(),
            );
            arena.assume_init()
        };

        unsafe {
            let heap = sys::mi_heap_new_ex(0xff, false, arena);
            sys::mi_heap_set_default(heap);
        }

        Ok(Self { raw, arena })
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        let heap = unsafe {
            let heap = sys::mi_heap_new_ex(0xff, false, self.arena);
            sys::mi_heap_set_default(heap);
            heap
        };

        Mimalloc(heap)
    }

    fn unlink(mut self) -> io::Result<()> {
        self.raw.unlink()?;

        // FIXME: the destructor for `shm::Raw` unmaps the memory region,
        // but mimalloc does some cleanup of abandoned segments in a process
        // finalizer that accesses this memory, causing a SEGFAULT.
        //
        // We *do* want to unlink so that the shm file is cleaned up
        // by the OS between benchmark runs.
        std::mem::forget(self.raw);
        Ok(())
    }
}

impl allocator_bench::Allocator for Mimalloc {
    type Handle = NonNull<ffi::c_void>;

    #[inline]
    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        NonNull::new(unsafe { sys::mi_malloc(size) })
    }

    #[inline]
    unsafe fn deallocate(&mut self, handle: NonNull<ffi::c_void>) {
        sys::mi_free(handle.as_ptr())
    }

    // NOTE: will not work across processes unless mapped at a fixed address
    #[inline]
    unsafe fn handle_to_offset(&mut self, handle: &NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new_unchecked(handle.as_ptr() as u64)
    }

    #[inline]
    fn offset_to_handle(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        Some(unsafe { NonNull::new_unchecked(NonZeroU64::new(offset)?.get() as *mut ffi::c_void) })
    }
}

impl Drop for Mimalloc {
    fn drop(&mut self) {
        unsafe {
            sys::mi_heap_delete(self.0);
        }
    }
}
