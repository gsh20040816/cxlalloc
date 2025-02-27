use core::ffi;
use core::marker::PhantomData;
use core::mem;
use core::ptr;
use std::ffi::CString;
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;

const PAGE: usize = 4096;

pub struct Shm<T> {
    inner: Raw,
    r#type: PhantomData<T>,
}

pub struct Raw {
    name: CString,
    size: usize,
    address: *mut ffi::c_void,
}

impl<T> Shm<T> {
    const SIZE: usize = mem::size_of::<T>().next_multiple_of(PAGE);

    pub fn new(numa: Option<usize>, name: CString, populate: bool) -> io::Result<Self> {
        let inner = Raw::new(numa, name, Self::SIZE, populate)?;
        Ok(Self {
            inner,
            r#type: PhantomData,
        })
    }

    pub fn address(&self) -> *const T {
        self.inner.address.cast()
    }

    pub fn address_mut(&self) -> *mut T {
        self.inner.address.cast()
    }

    pub fn size(&self) -> usize {
        self.inner.size
    }

    pub fn unmap(self) -> io::Result<()> {
        self.inner.unmap()
    }

    pub fn unlink(&mut self) -> io::Result<()> {
        self.inner.unlink()
    }
}

impl Raw {
    pub fn new(
        numa: Option<usize>,
        name: CString,
        size: usize,
        populate: bool,
    ) -> io::Result<Self> {
        let size = size.next_multiple_of(PAGE);

        let (create, fd) = match unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o666,
            )
        } {
            -1 => {
                match io::Error::last_os_error() {
                    error if error.kind() == io::ErrorKind::AlreadyExists => (),
                    error => return Err(error),
                }

                match unsafe { libc::shm_open(name.as_ptr(), libc::O_RDWR, 0o666) } {
                    -1 => return Err(io::Error::last_os_error()),
                    fd => (false, unsafe { OwnedFd::from_raw_fd(fd) }),
                }
            }
            fd => (true, unsafe { OwnedFd::from_raw_fd(fd) }),
        };

        if create && unsafe { libc::ftruncate(fd.as_raw_fd(), size as i64) } == -1 {
            return Err(io::Error::last_os_error());
        }

        let address = match unsafe {
            libc::mmap64(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED_VALIDATE,
                fd.as_raw_fd(),
                0,
            )
        } {
            libc::MAP_FAILED => return Err(io::Error::last_os_error()),
            address => address,
        };

        if let (true, Some(numa)) = (create, numa) {
            Self::mbind(numa, address, size)?;
        }

        if populate {
            Self::madvise(address, size)?;
        }

        Ok(Self {
            name,
            size,
            address,
        })
    }

    pub fn address(&self) -> *const ffi::c_void {
        self.address
    }

    pub fn address_mut(&self) -> *mut ffi::c_void {
        self.address
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn unmap(mut self) -> io::Result<()> {
        let address = mem::replace(&mut self.address, ptr::null_mut());
        match unsafe { libc::munmap(address, self.size) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    pub fn unlink(&mut self) -> io::Result<()> {
        if unsafe { libc::shm_unlink(self.name.as_ptr()) } == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    // https://github.com/numactl/numactl/blob/6c14bd59d438ebb5ef828e393e8563ba18f59cb2/syscall.c#L230-L235
    fn mbind(numa: usize, address: *mut ffi::c_void, size: usize) -> io::Result<()> {
        let mask = 1u64 << numa;
        match unsafe {
            libc::syscall(
                libc::SYS_mbind,
                address,
                size,
                libc::MPOL_BIND | libc::MPOL_F_STATIC_NODES,
                &mask,
                64,
                // MPOL_MF_STRICT
                // https://github.com/torvalds/linux/blob/0c559323bbaabee7346c12e74b497e283aaafef5/include/uapi/linux/mempolicy.h#L48
                1,
            )
        } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn madvise(address: *mut ffi::c_void, size: usize) -> io::Result<()> {
        match unsafe { libc::madvise(address, size, libc::MADV_POPULATE_WRITE) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        if self.address.is_null() {
            return;
        }

        unsafe {
            if libc::munmap(self.address, self.size) == -1 {
                panic!(
                    "Failed to munmap {:#x?} ({:#x}): {:?}",
                    self.address,
                    self.size,
                    io::Error::last_os_error()
                );
            }
        }
    }
}
