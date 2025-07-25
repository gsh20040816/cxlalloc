use core::ffi;
use core::mem::MaybeUninit;
use core::num::NonZeroU64;
use core::ops::Deref;
use core::ptr::NonNull;
use std::ffi::OsStr;
use std::sync::Arc;

use shm_bench::allocator::Config;
use sys::LightningAllocator_Free;
use sys::LightningAllocator_Initialize;
use sys::LightningAllocator_Malloc;
use sys::LightningAllocator_OffsetToPointer;
use sys::LightningAllocator_PointerToOffset;

#[expect(unused)]
#[expect(non_camel_case_types)]
#[expect(non_snake_case)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind_lightning.rs"));
}

pub struct Backend {
    shm: shm::Raw,
    inner: Arc<sys::LightningAllocator>,
}

unsafe impl Sync for Backend {}

pub struct Lightning {
    id: usize,
    store: Arc<sys::LightningAllocator>,
}

unsafe impl Send for sys::LightningAllocator {}
unsafe impl Sync for sys::LightningAllocator {}

impl shm_bench::allocator::Backend for Backend {
    type Allocator = Lightning;
    type Config = ();

    fn new(create: bool, config: &Config<Self::Config>, name: &str) -> anyhow::Result<Self> {
        let shm = shm::Raw::builder()
            .maybe_numa(config.numa.clone())
            .name(name.to_owned())
            .size(config.size)
            .create(create)
            .maybe_populate(config.populate)
            .build()?;

        let mut store = MaybeUninit::<sys::LightningAllocator>::uninit();
        let inner = Arc::new(unsafe {
            sys::LightningAllocator_LightningAllocator(
                store.as_mut_ptr(),
                shm.address().as_ptr().cast(),
                config.size as _,
            );
            store.assume_init()
        });

        if create {
            unsafe { LightningAllocator_Initialize(inner.deref() as *const _ as *mut _, 0) }
        }

        Ok(Self { shm, inner })
    }

    fn allocator(&self, id: usize) -> Self::Allocator {
        Lightning {
            id,
            store: Arc::clone(&self.inner),
        }
    }

    fn unlink(mut self) -> anyhow::Result<()> {
        self.shm.unlink()?;

        for entry in std::fs::read_dir("/dev/shm").unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let Some(name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if name.starts_with("log") {
                std::fs::remove_file(path)?;
            }
        }

        Ok(())
    }

    fn categorize(&self, mapping: &shm_bench::Mapping) -> Option<shm_bench::allocator::Memory> {
        (mapping.start == self.shm.address().as_ptr().addr())
            .then_some(shm_bench::allocator::Memory::Hwcc)
    }
}

impl Lightning {
    fn as_ptr(&self) -> *mut sys::LightningAllocator {
        self.store.deref() as *const _ as *mut _
    }
}

impl shm_bench::Allocator for Lightning {
    type Handle = NonNull<ffi::c_void>;

    #[inline]
    fn allocate(&mut self, size: usize) -> Option<Self::Handle> {
        let store = self.as_ptr();
        unsafe {
            let offset = LightningAllocator_Malloc(store, self.id as u64, size);
            let pointer = LightningAllocator_OffsetToPointer(store, offset);
            NonNull::new(pointer)
        }
    }

    #[inline]
    unsafe fn deallocate(&mut self, handle: Self::Handle) {
        let store = self.as_ptr();
        unsafe {
            let offset = LightningAllocator_PointerToOffset(store, handle.as_ptr());
            LightningAllocator_Free(store, self.id as u64, offset);
        }
    }

    #[inline]
    unsafe fn handle_to_offset(&mut self, handle: &Self::Handle) -> NonZeroU64 {
        NonZeroU64::new(LightningAllocator_PointerToOffset(self.as_ptr(), handle.as_ptr()) as u64)
            .unwrap()
    }

    #[inline]
    fn offset_to_handle(&mut self, offset: NonZeroU64) -> Self::Handle {
        NonNull::new(unsafe {
            LightningAllocator_OffsetToPointer(self.as_ptr(), offset.get() as i64)
        })
        .unwrap()
    }

    #[inline]
    fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(unsafe {
            LightningAllocator_PointerToOffset(self.as_ptr(), pointer.as_ptr())
        } as u64)
        .unwrap()
    }
}
