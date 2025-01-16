use core::ffi;
use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;
use std::ffi::CString;
use std::io;
use std::os::fd::RawFd;

use crate::extend::Epoch;
use crate::raw::backend;
use crate::Atomic;

#[repr(C, align(4096))]
pub(crate) struct Page([u8; 4096]);

pub(crate) struct Reservation {
    address: NonNull<Page>,
    size: NonZeroUsize,
}

impl Reservation {
    pub(crate) const TIB: NonZeroUsize = NonZeroUsize::new(1 << 40).unwrap();

    // In order to keep heap regions contiguous when extending, we need
    // to reserve an unbacked region of virtual address space,
    // and then overwrite it later via `mmap` with `MMAP_FIXED`.
    pub(crate) fn new(size: NonZeroUsize) -> io::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let address = unsafe { mmap(None, size, libc::PROT_NONE, backend::File::default())? };
        Ok(Self { address, size })
    }

    pub(crate) fn split(self, at: NonZeroUsize) -> (Self, Self) {
        let lo = Self {
            address: self.address,
            size: at,
        };

        let hi = Self {
            address: unsafe { self.address.byte_add(at.get()) },
            size: NonZeroUsize::new(self.size.get() - at.get()).unwrap(),
        };

        (lo, hi)
    }
}

pub(crate) struct Region {
    /// Unique identifier of this memory region
    id: String,

    /// Offset into physical memory (for ivshmem driver)
    #[cfg_attr(not(feature = "backend-ivshmem"), allow(dead_code))]
    offset: i64,

    /// Size of this memory region in bytes
    size: Option<NonZeroUsize>,

    reservation: Option<Reservation>,

    /// Number of heap extensions this memory region has undergone.
    epoch: Atomic<Epoch>,

    /// Starting address of mapped region
    address: NonNull<Page>,

    /// Whether this is a new region
    clean: bool,
}

impl Region {
    pub(super) fn new(
        id: String,
        file: backend::File,
        reservation: Option<Reservation>,
        size: usize,
    ) -> io::Result<Self> {
        let size = size.next_multiple_of(crate::SIZE_PAGE);
        let size = NonZeroUsize::new(size);
        let address = match size {
            None => reservation
                .as_ref()
                .map(|reservation| reservation.address)
                .unwrap(),
            Some(size) => unsafe {
                let address = mmap(
                    reservation.as_ref().map(|reservation| reservation.address),
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    file,
                )?;
                mbind(address, size)?;
                address
            },
        };

        Ok(Region {
            id,
            offset: file.offset,
            size,
            reservation,
            epoch: Atomic::new(Epoch::default()),
            address: address.cast(),
            clean: file.clean,
        })
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn address(&self) -> NonNull<Page> {
        self.address
    }

    #[cfg_attr(not(feature = "backend-ivshmem"), allow(unused))]
    pub(crate) fn offset(&self) -> i64 {
        self.offset
    }

    pub(crate) fn size(&self) -> usize {
        self.size.map(NonZeroUsize::get).unwrap_or_default()
    }

    pub(crate) fn is_clean(&self) -> bool {
        self.clean
    }

    #[cfg_attr(
        not(any(feature = "backend-ivshmem", feature = "backend-shm")),
        allow(unused)
    )]
    pub(crate) fn epoch(&self) -> Epoch {
        self.epoch.load()
    }

    pub(crate) fn advance_epoch(&self) -> Epoch {
        let epoch = self.epoch.load();
        self.epoch.store(epoch.next());
        epoch.next()
    }

    pub(super) fn epoch_to_metadata(&self, epoch: Epoch) -> (*mut ffi::c_void, usize, CString) {
        (
            self.epoch_to_address(epoch),
            self.epoch_to_size(epoch),
            Self::epoch_to_path(&self.id, epoch),
        )
    }

    fn epoch_to_address(&self, epoch: Epoch) -> *mut ffi::c_void {
        self.address()
            .as_ptr()
            .wrapping_byte_add(epoch.offset(self.size() as u32) as usize)
            .cast()
    }

    fn epoch_to_size(&self, epoch: Epoch) -> usize {
        epoch.partial(self.size() as u32) as usize
    }

    pub(super) fn epoch_to_path(id: &str, epoch: Epoch) -> CString {
        CString::new(format!("{id}-{}", u8::from(epoch))).unwrap()
    }

    pub(super) fn map(
        &self,
        file: backend::File,
        offset: usize,
        size: NonZeroUsize,
    ) -> io::Result<()> {
        unsafe {
            let address = mmap(
                Some(self.address.byte_add(offset)),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                file,
            )?;
            mbind(address, size)?;
        }

        Ok(())
    }

    /// Remove all virtual address space mappings for this region.
    pub(super) fn unmap(&self) -> io::Result<()> {
        match (&self.reservation, self.size) {
            (Some(reservation), _) => unsafe { munmap(reservation.address, reservation.size) },
            (None, Some(size)) => unsafe { munmap(self.address, size) },
            (None, None) => Ok(()),
        }
    }
}

unsafe fn munmap(address: NonNull<Page>, size: NonZeroUsize) -> io::Result<()> {
    match unsafe { libc::munmap(address.as_ptr().cast(), size.get()) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

unsafe fn mmap(
    address: Option<NonNull<Page>>,
    size: NonZeroUsize,
    protect: libc::c_int,
    file: backend::File,
) -> io::Result<NonNull<Page>> {
    let actual = match libc::mmap64(
        address
            .map(NonNull::as_ptr)
            .unwrap_or_else(ptr::null_mut)
            .cast(),
        size.get(),
        protect,
        file.flags() | address.map(|_| libc::MAP_FIXED).unwrap_or(0),
        file.fd(),
        file.offset,
    ) {
        libc::MAP_FAILED => return Err(io::Error::last_os_error()),
        actual => NonNull::new(actual).unwrap().cast::<Page>(),
    };

    if let Some(expected) = address {
        assert_eq!(expected, actual);
    }

    Ok(actual)
}

// https://github.com/numactl/numactl/blob/6c14bd59d438ebb5ef828e393e8563ba18f59cb2/syscall.c#L230-L235
unsafe fn mbind(address: NonNull<Page>, size: NonZeroUsize) -> io::Result<()> {
    let Some(numa) = std::env::var("CXL_NUMA_NODE")
        .ok()
        .and_then(|numa| numa.parse::<usize>().ok())
    else {
        return Ok(());
    };

    let mask = 1u64 << numa;
    match libc::syscall(
        libc::SYS_mbind,
        address,
        size.get(),
        libc::MPOL_BIND | libc::MPOL_F_STATIC_NODES,
        &mask,
        64,
        // MPOL_MF_STRICT
        // https://github.com/torvalds/linux/blob/0c559323bbaabee7346c12e74b497e283aaafef5/include/uapi/linux/mempolicy.h#L48
        1,
    ) {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}
