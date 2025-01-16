use core::num::NonZeroUsize;
use std::ffi::CString;
use std::io;

use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;

use crate::raw;
use crate::raw::backend;

#[derive(Debug)]
pub struct Shm;

impl backend::Impl for Shm {
    fn name(&self) -> &'static str {
        "shm"
    }

    fn allocate(&self, id: String, size: NonZeroUsize) -> io::Result<backend::File> {
        let path = id_to_path(id);
        unsafe {
            let (fd, clean) = match libc::shm_open(
                path.as_c_str().as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                libc::S_IRUSR | libc::S_IWUSR | libc::S_IRGRP | libc::S_IWGRP,
            ) {
                -1 => {
                    let error = std::io::Error::last_os_error();
                    if !matches!(error.kind(), io::ErrorKind::AlreadyExists) {
                        return Err(error);
                    }

                    // Note: there's still a race condition here, since another
                    // process could have deleted and recreated the shared memory
                    // region between the previous call to `shm_open` and this one.
                    match libc::shm_open(
                        path.as_c_str().as_ptr(),
                        libc::O_RDWR,
                        libc::S_IRUSR | libc::S_IWUSR | libc::S_IRGRP | libc::S_IWGRP,
                    ) {
                        -1 => return Err(std::io::Error::last_os_error()),
                        fd => (OwnedFd::from_raw_fd(fd), false),
                    }
                }
                fd => (OwnedFd::from_raw_fd(fd), true),
            };

            if libc::ftruncate64(fd.as_raw_fd(), size.get().try_into().unwrap()) == -1 {
                return Err(io::Error::last_os_error());
            }

            Ok(backend::File::new(fd, 0, clean))
        }
    }

    fn free(&self, id: String) -> io::Result<()> {
        let path = id_to_path(id);
        match unsafe { libc::shm_unlink(path.as_c_str().as_ptr()) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

fn id_to_path(id: String) -> CString {
    let mut path = vec![b'/'];
    path.append(&mut id.into_bytes());
    path.push(0);
    CString::from_vec_with_nul(path).unwrap()
}

impl From<Shm> for raw::Backend {
    fn from(shm: Shm) -> Self {
        raw::Backend::Shm(shm)
    }
}
