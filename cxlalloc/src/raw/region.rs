use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;
use std::io;

use crate::raw::backend;
use crate::raw::Backend;

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
        let address = unsafe { mmap(None, size, false, &backend::File::default())? };
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

pub(crate) trait Region {
    fn address(&self) -> NonNull<Page>;
    fn is_clean(&self) -> bool;
    fn id(&self) -> &str;
    fn unmap(&self) -> io::Result<()>;
}

pub(crate) struct Fixed {
    id: String,
    clean: bool,
    address: NonNull<Page>,
    size: NonZeroUsize,
}

pub(crate) struct Sequential {
    prefix: String,
    clean: bool,
    reservation: Reservation,
    size: NonZeroUsize,
}

pub(crate) struct Random {
    prefix: String,
    reservation: Reservation,
}

impl Fixed {
    pub(super) fn new(backend: &Backend, id: String, size: NonZeroUsize) -> io::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let file = backend.allocate(id.clone(), size)?;
        let address = unsafe { mmap(None, size, true, &file)? };
        Ok(Self {
            id,
            address,
            clean: file.clean,
            size,
        })
    }
}

impl Region for Fixed {
    fn address(&self) -> NonNull<Page> {
        self.address
    }

    fn is_clean(&self) -> bool {
        self.clean
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn unmap(&self) -> io::Result<()> {
        unsafe { munmap(self.address, self.size) }
    }
}

impl Sequential {
    pub(super) fn new(
        backend: &Backend,
        prefix: String,
        reservation: Reservation,
        size: NonZeroUsize,
    ) -> io::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let file = backend.allocate(format!("{}-0", prefix), size)?;

        unsafe {
            let address = mmap(Some(reservation.address), size, true, &file)?;
            mbind(address, size)?;
        };

        Ok(Sequential {
            prefix,
            clean: file.clean,
            reservation,
            size,
        })
    }
}

impl Region for Sequential {
    fn address(&self) -> NonNull<Page> {
        self.reservation.address
    }

    fn is_clean(&self) -> bool {
        self.clean
    }

    fn id(&self) -> &str {
        &self.prefix
    }

    /// Remove all virtual address space mappings for this region.
    fn unmap(&self) -> io::Result<()> {
        unsafe { munmap(self.reservation.address, self.reservation.size) }
    }
}

impl Random {
    pub(super) fn new(prefix: String, reservation: Reservation) -> io::Result<Self> {
        Ok(Random {
            prefix,
            reservation,
        })
    }

    pub(crate) fn map(
        &self,
        backend: &Backend,
        offset: usize,
        size: NonZeroUsize,
    ) -> io::Result<()> {
        let file = backend.allocate(format!("{}-{:#x}", self.prefix, offset), size)?;

        unsafe {
            let address = mmap(Some(self.address().byte_add(offset)), size, true, &file)?;
            mbind(address, size)?;
        }

        Ok(())
    }
}

impl Region for Random {
    fn address(&self) -> NonNull<Page> {
        self.reservation.address
    }

    fn is_clean(&self) -> bool {
        false
    }

    fn id(&self) -> &str {
        &self.prefix
    }

    fn unmap(&self) -> io::Result<()> {
        unsafe { munmap(self.reservation.address, self.reservation.size) }
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
    rw: bool,
    file: &backend::File,
) -> io::Result<NonNull<Page>> {
    let actual = match libc::mmap64(
        address
            .map(NonNull::as_ptr)
            .unwrap_or_else(ptr::null_mut)
            .cast(),
        size.get(),
        if rw {
            libc::PROT_READ | libc::PROT_WRITE
        } else {
            libc::PROT_NONE
        },
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
