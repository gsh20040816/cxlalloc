use core::num::NonZeroUsize;
use std::io;
use std::os::fd::AsFd as _;

use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;

use crate::extend::Epoch;
use crate::raw;
use crate::raw::backend;
use crate::raw::Region;
use crate::raw::Reservation;
use crate::SIZE_PAGE;

#[derive(Debug)]
pub struct Shm;

impl backend::Impl for Shm {
    fn name(&self) -> &'static str {
        "shm"
    }

    fn allocate(
        &self,
        id: String,
        reservation: Option<Reservation>,
        size: usize,
    ) -> io::Result<Region> {
        let size = size.next_multiple_of(SIZE_PAGE);

        unsafe {
            let path = Region::epoch_to_path(&id, Epoch::default());

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

            Region::new(
                id,
                backend::File::new(fd.as_fd(), 0, clean),
                reservation,
                size,
            )
        }
    }

    fn map(&self, region: &Region, offset: usize, size: NonZeroUsize) -> io::Result<()> {
        unsafe {
            todo!()
            // let epoch = region.advance_epoch();
            // let (address, size, path) = region.epoch_to_metadata(epoch);
            //
            // let fd = match libc::shm_open(
            //     path.as_c_str().as_ptr(),
            //     libc::O_RDWR | libc::O_CREAT,
            //     libc::S_IRUSR | libc::S_IWUSR | libc::S_IRGRP | libc::S_IWGRP,
            // ) {
            //     -1 => return Err(std::io::Error::last_os_error()),
            //     fd => OwnedFd::from_raw_fd(fd),
            // };
            //
            // if libc::ftruncate64(fd.as_raw_fd(), size.try_into().unwrap()) == -1 {
            //     return Err(io::Error::last_os_error());
            // }
            //
            // region.map(backend::File::new(fd.as_fd(), 0, true), offset, size)
        }
    }

    fn unmap(&self, region: &Region) -> io::Result<()> {
        region.unmap()
    }

    fn free(&self, region: &Region) -> io::Result<()> {
        unsafe {
            let mut start = Epoch::default();
            let end = region.epoch();

            while start <= end {
                let (_, _, path) = region.epoch_to_metadata(start);
                if libc::shm_unlink(path.as_c_str().as_ptr()) != 0 {
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
