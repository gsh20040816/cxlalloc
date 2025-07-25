use core::ffi;
use core::mem::MaybeUninit;
use core::num::NonZeroU64;
use core::ptr::NonNull;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;
use serde_inline_default::serde_inline_default;

#[expect(unused)]
#[expect(non_camel_case_types)]
#[expect(non_upper_case_globals)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_mimalloc.rs"));
}

pub struct Backend {
    raw: Option<shm::Raw>,
    arena: Option<sys::mi_arena_id_t>,
}

unsafe impl Sync for Backend {}

pub struct Mimalloc(Option<*mut sys::mi_heap_t>);

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
#[serde_inline_default]
pub struct Config {
    #[serde_inline_default(true)]
    shm: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shm: __serde_inline_default_Config_0(),
        }
    }
}

impl shm_bench::allocator::Backend for Backend {
    type Allocator = Mimalloc;
    type Config = Config;

    fn new(
        create: bool,
        config: &shm_bench::allocator::Config<Self::Config>,
        name: &str,
    ) -> anyhow::Result<Self> {
        if !config.inner.shm {
            return Ok(Self {
                raw: None,
                arena: None,
            });
        }

        let raw = shm::Raw::builder()
            .maybe_numa(config.numa.clone())
            .name(name.to_owned())
            .size(config.size)
            .create(create)
            .maybe_populate(config.populate)
            .build()?;

        let arena = unsafe {
            let mut arena = MaybeUninit::<sys::mi_arena_id_t>::zeroed();
            sys::mi_manage_os_memory_ex(
                raw.address().as_ptr().cast(),
                raw.size().get(),
                true,
                false,
                true,
                // https://github.com/microsoft/mimalloc/blob/af21001f7a65eafb8fb16460b018ebf9d75e2ad8/src/arena.c#L853
                -1,
                true,
                arena.as_mut_ptr(),
            );
            arena.assume_init()
        };

        unsafe {
            let heap = sys::mi_heap_new_ex(0xff, false, arena);
            sys::mi_heap_set_default(heap);
        }

        Ok(Self {
            raw: Some(raw),
            arena: Some(arena),
        })
    }

    fn allocator(&self, _: usize) -> Self::Allocator {
        let heap = match self.arena {
            None => None,
            Some(arena) => unsafe {
                let heap = sys::mi_heap_new_ex(0xff, false, arena);
                sys::mi_heap_set_default(heap);
                Some(heap)
            },
        };

        Mimalloc(heap)
    }

    fn unlink(mut self) -> anyhow::Result<()> {
        if let Some(raw) = &mut self.raw {
            raw.unlink()?;
        }

        // FIXME: the destructor for `shm::Raw` unmaps the memory region,
        // but mimalloc does some cleanup of abandoned segments in a process
        // finalizer that accesses this memory, causing a SEGFAULT.
        //
        // We *do* want to unlink so that the shm file is cleaned up
        // by the OS between benchmark runs.
        std::mem::forget(self.raw);
        Ok(())
    }

    fn categorize(&self, mapping: &shm_bench::Mapping) -> Option<shm_bench::allocator::Memory> {
        let shm = self.raw.as_ref()?;
        (mapping.start == shm.address().as_ptr().addr())
            .then_some(shm_bench::allocator::Memory::Hwcc)
    }
}

impl shm_bench::Allocator for Mimalloc {
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
    fn offset_to_handle(&mut self, offset: NonZeroU64) -> NonNull<ffi::c_void> {
        unsafe { NonNull::new_unchecked(offset.get() as *mut ffi::c_void) }
    }

    #[inline]
    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        unsafe { NonZeroU64::new_unchecked(pointer.as_ptr() as u64) }
    }
}

impl Drop for Mimalloc {
    fn drop(&mut self) {
        if let Some(heap) = self.0 {
            unsafe {
                sys::mi_heap_delete(heap);
            }
        }
    }
}
