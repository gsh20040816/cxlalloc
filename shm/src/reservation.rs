use core::ffi;
use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;

use crate::Page;

pub struct Reservation {
    size: NonZeroUsize,
    address: NonNull<Page>,
}

impl Reservation {
    // In order to keep heap regions contiguous when extending, we need
    // to reserve an unbacked region of virtual address space,
    // and then overwrite it later via `mmap` with `MMAP_FIXED`.
    pub fn new(size: NonZeroUsize) -> crate::Result<Self> {
        let address = Self::mmap(size)?;
        Ok(Self { size, address })
    }

    pub fn new_contiguous<const COUNT: usize>(size: NonZeroUsize) -> crate::Result<[Self; COUNT]> {
        let total = NonZeroUsize::new(size.get() * COUNT).unwrap();
        let address = Self::mmap(total)?;
        Ok(std::array::from_fn(|i| Self {
            size,
            address: unsafe { address.byte_add(size.get() * i) },
        }))
    }

    fn mmap(size: NonZeroUsize) -> crate::Result<NonNull<Page>> {
        match unsafe {
            libc::mmap64(
                ptr::null_mut(),
                size.get(),
                libc::PROT_NONE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                -1,
                0,
            )
        } {
            libc::MAP_FAILED => Err(crate::Error::Libc {
                name: "mmap64",
                source: std::io::Error::last_os_error(),
            }),
            actual => Ok(NonNull::new(actual).unwrap().cast::<Page>()),
        }
    }

    pub fn unmap(&self) -> crate::Result<()> {
        unsafe {
            crate::try_libc!(libc::munmap(
                self.address.as_ptr().cast::<ffi::c_void>(),
                self.size.get(),
            ))?;
        }
        Ok(())
    }

    pub fn start(&self) -> NonNull<Page> {
        self.address
    }

    pub fn end(&self) -> NonNull<Page> {
        unsafe { self.address.byte_add(self.size.get()) }
    }
}
