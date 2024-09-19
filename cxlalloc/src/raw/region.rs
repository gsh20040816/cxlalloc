use core::ffi;
use core::ffi::CStr;
use core::fmt;
use core::fmt::Display;
use core::fmt::Write as _;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;
use std::io;
use std::os::fd::RawFd;

use arrayvec::ArrayString;

use crate::extend::Epoch;
use crate::Atomic;

pub(crate) struct Region {
    /// Unique identifier of this memory region
    id: Id,

    /// Size of this memory region in bytes
    size: usize,

    /// Number of heap extensions this memory region has undergone.
    epoch: Atomic<Epoch>,

    /// Starting address of mapped region
    base: NonNull<u64>,
}

impl Region {
    const RESERVATION: usize = 2usize.pow(40);

    pub(super) fn new(id: Id, size: usize, file: Option<(RawFd, i64)>) -> io::Result<Self> {
        // In order to keep heap regions contiguous when extending, we need
        // to reserve an unbacked region of virtual address space via `mmap` with
        // `PROT_NONE`, and then overwrite it later via `mmap` with `MMAP_FIXED`.
        let reserved = match unsafe {
            libc::mmap64(
                ptr::null_mut(),
                Self::RESERVATION,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        } {
            libc::MAP_FAILED => return Err(io::Error::last_os_error()),
            address => address,
        };

        let (fd, offset, flags) = match file {
            Some((fd, offset)) => (fd, offset, libc::MAP_SHARED_VALIDATE),
            None => (-1, 0, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS),
        };

        let base = match unsafe {
            libc::mmap64(
                reserved,
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
                if unsafe { libc::munmap(reserved, Self::RESERVATION) != 0 } {
                    log::warn!("Failed to munmap reserved virtual address space");
                }
                return Err(error);
            }
            address => NonNull::new(address).unwrap().cast(),
        };

        Ok(Region {
            id,
            size,
            epoch: Atomic::new(Epoch::default()),
            base,
        })
    }

    pub(crate) fn base(&self) -> NonNull<u64> {
        self.base
    }

    pub(crate) fn size(&self) -> usize {
        self.size
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

    pub(super) fn epoch_to_metadata(&self, epoch: Epoch) -> (*mut ffi::c_void, usize, Id) {
        (
            self.epoch_to_address(epoch),
            self.epoch_to_size(epoch),
            self.epoch_to_id(epoch),
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

    pub(super) fn epoch_to_id(&self, epoch: Epoch) -> Id {
        self.id.with_epoch(epoch)
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
                Ok(actual)
            }
        }
    }

    /// Remove all virtual address space mappings for this region.
    pub(super) fn unmap(&self) -> io::Result<()> {
        match unsafe { libc::munmap(self.base.as_ptr().cast(), Self::RESERVATION) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

/// Fixed-size persistent region identifier to avoid
/// dynamic allocation within the allocator.
///
/// Invariant: holds a valid null-terminated C string.
#[derive(Debug)]
pub(crate) struct Id(ArrayString<{ Self::MAX_LENGTH + 1 }>);

impl Id {
    const MAX_LENGTH: usize = 15;

    pub(crate) fn new(prefix: &str) -> Self {
        assert!(
            prefix.len() <= Self::MAX_LENGTH,
            "Region prefix {} exceeds maximum length {}",
            prefix,
            Self::MAX_LENGTH,
        );

        assert!(
            !prefix.contains('\0'),
            "Region prefix {} contains null byte",
            prefix,
        );

        let mut id = ArrayString::from(prefix).unwrap();
        id.push('\0');
        Self(id)
    }

    #[cfg_attr(not(feature = "backend-shm"), allow(unused))]
    pub(super) fn as_c_str(&self) -> &CStr {
        CStr::from_bytes_with_nul(self.0.as_bytes()).unwrap()
    }

    pub(crate) fn with_suffix(&self, suffix: &str) -> Self {
        assert!(
            // -1 for null byte
            // +1 for dash
            self.0.len() - 1 + 1 + suffix.len() <= Self::MAX_LENGTH,
            "Region identifier {}-{} exceeds maximum length {}",
            self.0,
            suffix,
            Self::MAX_LENGTH,
        );

        let mut new = self.0;
        debug_assert_eq!(new.pop(), Some('\0'));
        new.push('-');
        new.push_str(suffix);
        new.push('\0');
        Id(new)
    }

    pub(super) fn with_epoch(&self, epoch: Epoch) -> Self {
        let epoch = u8::from(epoch);

        // log10(0..10) = 0
        // log10(10..100) = 1
        let digits = epoch.checked_ilog10().unwrap_or(0) as usize + 1;
        assert!(
            // -1 for null byte
            // +1 for dash
            self.0.len() - 1 + 1 + digits <= Self::MAX_LENGTH,
            "Region identifier {}-{} exceeds maximum length {}",
            self.0,
            epoch,
            Self::MAX_LENGTH,
        );

        let mut new = self.0;
        debug_assert_eq!(new.pop(), Some('\0'));
        write!(&mut new, "-{}\0", epoch).unwrap();
        Id(new)
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0[..self.0.len() - 1])
    }
}
