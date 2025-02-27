use core::ffi;
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::ptr::NonNull;
use std::ffi::CString;
use std::ffi::OsStr;
use std::io;
use std::sync::Arc;

use sys::LightningAllocator_Free;
use sys::LightningAllocator_Initialize;
use sys::LightningAllocator_Malloc;
use sys::LightningAllocator_OffsetToPointer;
use sys::LightningAllocator_PointerToOffset;

use crate::MAP_POPULATE;

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

unsafe impl Send for Backend {}
unsafe impl Sync for Backend {}

pub struct Lightning {
    id: usize,
    store: Arc<sys::LightningAllocator>,
}

unsafe impl Send for sys::LightningAllocator {}
unsafe impl Sync for sys::LightningAllocator {}

impl allocator_bench::Backend for Backend {
    type Allocator = Lightning;

    fn open(numa: usize, name: &str, size: usize) -> io::Result<Self> {
        let shm = shm::Raw::new(Some(numa), CString::new(name).unwrap(), size, *MAP_POPULATE)?;
        let mut store = MaybeUninit::<sys::LightningAllocator>::uninit();
        let inner = Arc::new(unsafe {
            sys::LightningAllocator_LightningAllocator(
                store.as_mut_ptr(),
                shm.address_mut().cast(),
                size as _,
            );
            store.assume_init()
        });
        Ok(Self { shm, inner })
    }

    fn allocator(&self, id: usize) -> Self::Allocator {
        if id == 0 {
            unsafe { LightningAllocator_Initialize(self.inner.deref() as *const _ as *mut _, 0) }
        }
        Lightning {
            id,
            store: Arc::clone(&self.inner),
        }
    }

    fn unlink(mut self) -> io::Result<()> {
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
}

impl Lightning {
    fn as_ptr(&self) -> *mut sys::LightningAllocator {
        self.store.deref() as *const _ as *mut _
    }
}

impl allocator_bench::Allocator for Lightning {
    type Ptr = NonNull<ffi::c_void>;

    fn allocate(&mut self, size: usize) -> Option<Self::Ptr> {
        let store = self.as_ptr();
        unsafe {
            let offset = LightningAllocator_Malloc(store, self.id as u64, size);
            let pointer = LightningAllocator_OffsetToPointer(store, offset);
            NonNull::new(pointer)
        }
    }

    unsafe fn deallocate(&mut self, pointer: Self::Ptr) {
        let store = self.as_ptr();
        unsafe {
            let offset = LightningAllocator_PointerToOffset(store, pointer.as_ptr());
            LightningAllocator_Free(store, self.id as u64, offset);
        }
    }

    unsafe fn pointer_to_offset(&mut self, pointer: &Self::Ptr) -> u64 {
        LightningAllocator_PointerToOffset(self.as_ptr(), pointer.as_ptr()) as u64
    }

    fn offset_to_pointer(&mut self, offset: u64) -> Option<Self::Ptr> {
        NonNull::new(unsafe { LightningAllocator_OffsetToPointer(self.as_ptr(), offset as i64) })
    }
}
