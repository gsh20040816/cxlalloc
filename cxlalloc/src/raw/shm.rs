use std::io;
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;

use crate::extend::Epoch;
use crate::raw;
use crate::raw::backend::Backend;
use crate::raw::Id;
use crate::raw::Region;
use crate::SIZE_PAGE;

#[derive(Clone, Debug)]
pub struct Shm {
    destroy: bool,
}

impl Shm {
    pub fn new(destroy: bool) -> Self {
        Self { destroy }
    }
}

impl Backend for Shm {
    fn allocate(&self, id: Id, size: usize, reserved: usize) -> io::Result<Region> {
        let size = size.next_multiple_of(SIZE_PAGE);

        unsafe {
            let path = id.with_epoch(Epoch::default());
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

            if libc::ftruncate64(fd.as_raw_fd(), size.try_into().unwrap()) == -1 {
                return Err(io::Error::last_os_error());
            }

            let region = Region::new(id, size, reserved, Some((fd.as_raw_fd(), 0, clean)))?;
            Ok(region)
        }
    }

    fn extend(&self, region: &Region) -> io::Result<()> {
        unsafe {
            let epoch = region.advance_epoch();
            let (address, size, id) = region.epoch_to_metadata(epoch);

            let fd = match libc::shm_open(
                id.as_c_str().as_ptr(),
                libc::O_RDWR | libc::O_CREAT,
                libc::S_IRUSR | libc::S_IWUSR | libc::S_IRGRP | libc::S_IWGRP,
            ) {
                -1 => return Err(std::io::Error::last_os_error()),
                fd => OwnedFd::from_raw_fd(fd),
            };

            if libc::ftruncate64(fd.as_raw_fd(), size.try_into().unwrap()) == -1 {
                return Err(io::Error::last_os_error());
            }

            region.extend(address, size, Some((fd.as_raw_fd(), 0)))?;
            Ok(())
        }
    }

    fn free(&self, region: &Region) -> io::Result<()> {
        region.unmap()?;

        if !self.destroy {
            return Ok(());
        }

        unsafe {
            let mut start = Epoch::default();
            let end = region.epoch();

            while start <= end {
                let id = region.epoch_to_id(start);
                if libc::shm_unlink(id.as_c_str().as_ptr()) != 0 {
                    return Err(io::Error::last_os_error());
                }
                start = start.next();
            }

            Ok(())
        }
    }
}

impl From<Shm> for raw::Backend {
    fn from(shm: Shm) -> Self {
        raw::Backend::Shm(shm)
    }
}
