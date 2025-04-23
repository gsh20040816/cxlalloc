use core::ffi::CStr;
use core::num::NonZeroUsize;

use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;

use crate::Page;
use crate::backend;

#[derive(Debug)]
pub struct Shm;

impl backend::Interface for Shm {
    fn name(&self) -> &'static str {
        "shm"
    }

    fn open(&self, id: &CStr, size: NonZeroUsize) -> crate::Result<backend::File> {
        assert!(
            id.to_bytes()[0] == b'/',
            "Shared memory id {:?} should start with /",
            id.to_string_lossy(),
        );

        let size = size.get().next_multiple_of(Page::SIZE);

        let (create, fd) = match unsafe {
            crate::try_libc!(libc::shm_open(
                id.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o666,
            ))
        } {
            Err(error) if error.is_already_exists() => unsafe {
                let fd = crate::try_libc!(libc::shm_open(id.as_ptr(), libc::O_RDWR, 0o666))
                    .map(|fd| OwnedFd::from_raw_fd(fd))?;
                (false, fd)
            },
            Err(error) => return Err(error),
            Ok(fd) => (true, unsafe { OwnedFd::from_raw_fd(fd) }),
        };

        if create {
            unsafe {
                crate::try_libc!(libc::ftruncate64(fd.as_raw_fd(), size as i64))?;
            }
        }

        Ok(backend::File::builder()
            .fd(fd)
            .size(NonZeroUsize::new(size).unwrap())
            .create(create)
            .offset(0)
            .build())
    }

    fn unlink(&self, id: &CStr) -> crate::Result<()> {
        shm_unlink(id)
    }
}

impl From<Shm> for backend::Backend {
    fn from(shm: Shm) -> Self {
        backend::Backend::Shm(shm)
    }
}

fn shm_unlink(name: &CStr) -> crate::Result<()> {
    unsafe { crate::try_libc!(libc::shm_unlink(name.as_ptr())) }?;
    Ok(())
}
