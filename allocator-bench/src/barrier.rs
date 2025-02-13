use core::ffi::CStr;
use core::hint;
use core::ptr;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::fd::FromRawFd;
use std::os::fd::OwnedFd;

pub struct Barrier(&'static AtomicU64);

impl Barrier {
    const PAGE: usize = 4096;
    const PATH: &CStr = c"/barrier";

    pub fn new() -> io::Result<Self> {
        Self::open(Self::PATH)
    }

    fn open(path: &CStr) -> io::Result<Self> {
        unsafe {
            let shm = match libc::shm_open(path.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600) {
                -1 => return Err(io::Error::last_os_error()),
                fd => OwnedFd::from_raw_fd(fd),
            };

            if libc::ftruncate64(shm.as_raw_fd(), Self::PAGE as i64) == -1 {
                return Err(io::Error::last_os_error());
            }

            let address = match libc::mmap64(
                ptr::null_mut(),
                Self::PAGE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED_VALIDATE,
                shm.as_raw_fd(),
                0,
            ) {
                libc::MAP_FAILED => return Err(io::Error::last_os_error()),
                address => address,
            };

            Ok(Self(AtomicU64::from_ptr(address.cast())))
        }
    }

    pub fn init(&self, total: u64) {
        self.0.store(total, Ordering::Relaxed);
    }

    pub fn wait(&self) {
        if self.0.fetch_sub(1, Ordering::Relaxed) == 1 {
            return;
        }

        while self.0.load(Ordering::Relaxed) > 0 {
            hint::spin_loop()
        }
    }
}

impl Drop for Barrier {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.0.as_ptr().cast(), Self::PAGE);
        }
    }
}
