use core::ffi;
use core::ptr;
use core::ptr::NonNull;
use std::ffi::CString;
use std::io;
use std::os::fd::RawFd;

use crate::extend::Epoch;
use crate::Atomic;

pub(crate) const RESERVATION: usize = 2usize.pow(40);

pub(crate) struct Region {
    /// Unique identifier of this memory region
    id: String,

    /// Size of this memory region in bytes
    size: usize,

    /// Size of the reserved virtual address space in bytes
    reserved: usize,

    /// Number of heap extensions this memory region has undergone.
    epoch: Atomic<Epoch>,

    /// Starting address of mapped region
    base: NonNull<u64>,

    /// Whether this is a new region
    clean: bool,
}

impl Region {
    pub(super) fn new(
        id: String,
        size: usize,
        reserved: usize,
        file: Option<(RawFd, i64, bool)>,
    ) -> io::Result<Self> {
        // In order to keep heap regions contiguous when extending, we need
        // to reserve an unbacked region of virtual address space via `mmap` with
        // `PROT_NONE`, and then overwrite it later via `mmap` with `MMAP_FIXED`.
        let reservation = match unsafe {
            libc::mmap64(
                ptr::null_mut(),
                reserved,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        } {
            libc::MAP_FAILED => return Err(io::Error::last_os_error()),
            address => address,
        };

        let (fd, offset, flags, clean) = match file {
            Some((fd, offset, clean)) => (fd, offset, libc::MAP_SHARED_VALIDATE, clean),
            None => (-1, 0, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, true),
        };

        let base = match unsafe {
            libc::mmap64(
                reservation,
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                flags | libc::MAP_FIXED,
                fd,
                offset,
            )
        } {
            libc::MAP_FAILED => {
                // Save `mmap64` error before calling `munmap`.
                let error = io::Error::last_os_error();
                if unsafe { libc::munmap(reservation, reserved) != 0 } {
                    log::warn!("Failed to munmap reserved virtual address space");
                }
                return Err(error);
            }
            address => NonNull::new(address).unwrap(),
        };

        unsafe {
            mbind(base.as_ptr(), size).expect("Failed to mbind");
        }

        Ok(Region {
            id,
            size,
            reserved,
            epoch: Atomic::new(Epoch::default()),
            base: base.cast(),
            clean,
        })
    }

    pub(crate) fn base(&self) -> NonNull<u64> {
        self.base
    }

    pub(crate) fn size(&self) -> usize {
        self.size
    }

    pub(crate) fn is_clean(&self) -> bool {
        self.clean
    }

    #[cfg_attr(not(feature = "backend-shm"), allow(unused))]
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
        self.base()
            .as_ptr()
            .wrapping_byte_add(epoch.offset(self.size as u32) as usize)
            .cast()
    }

    fn epoch_to_size(&self, epoch: Epoch) -> usize {
        epoch.partial(self.size as u32) as usize
    }

    pub(super) fn epoch_to_path(id: &str, epoch: Epoch) -> CString {
        CString::new(format!("{id}-{}", u8::from(epoch))).unwrap()
    }

    pub(super) fn extend(
        &self,
        address: *mut libc::c_void,
        size: usize,
        file: Option<(RawFd, i64)>,
    ) -> io::Result<*mut ffi::c_void> {
        let (fd, offset, flags) = match file {
            Some((fd, offset)) => (fd, offset, libc::MAP_SHARED_VALIDATE),
            None => (-1, 0, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS),
        };

        match unsafe {
            libc::mmap64(
                address,
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                flags | libc::MAP_FIXED,
                fd,
                offset,
            )
        } {
            libc::MAP_FAILED => Err(io::Error::last_os_error()),
            actual => {
                assert_eq!(actual, address);

                unsafe {
                    mbind(actual, size).expect("Failed to mbind");
                }

                Ok(actual)
            }
        }
    }

    /// Remove all virtual address space mappings for this region.
    pub(super) fn unmap(&self) -> io::Result<()> {
        match unsafe { libc::munmap(self.base.as_ptr().cast(), self.reserved) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

// https://github.com/numactl/numactl/blob/6c14bd59d438ebb5ef828e393e8563ba18f59cb2/syscall.c#L230-L235
pub(crate) unsafe fn mbind(address: *mut ffi::c_void, size: usize) -> io::Result<()> {
    let Some(numa) = std::env::var("CXL_NUMA_NODE")
        .ok()
        .and_then(|numa| numa.parse::<usize>().ok())
    else {
        return Ok(());
    };

    let mask = 1u64 << numa;
    if libc::syscall(
        libc::SYS_mbind,
        address,
        size as u64,
        libc::MPOL_BIND | libc::MPOL_F_STATIC_NODES,
        &mask,
        64,
        // MPOL_MF_STRICT
        // https://github.com/torvalds/linux/blob/0c559323bbaabee7346c12e74b497e283aaafef5/include/uapi/linux/mempolicy.h#L48
        1,
    ) >= 0
    {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
