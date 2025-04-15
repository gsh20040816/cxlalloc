use core::num::NonZeroUsize;
use std::fs;
use std::os::fd::OwnedFd;

use crate::raw::backend;
use crate::raw::region;

#[derive(Debug)]
pub struct Ivshmem {
    device: fs::File,
}

impl Ivshmem {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        fs::File::options()
            .read(true)
            .write(true)
            .open("/dev/cxl_ivpci0")
            .map(|device| Self { device })
            .expect("Failed to open `/dev/cxl_ivpci0`: is CXL driver module loaded?")
    }
}

impl backend::Impl for Ivshmem {
    fn name(&self) -> &'static str {
        "ivshmem"
    }

    fn allocate(&self, id: region::Id, size: NonZeroUsize) -> crate::Result<backend::File> {
        let allocation = driver::find_cxl_alloc_nomap(&self.device, &id, size.get())
            .map_err(crate::Error::Ioctl)?;

        Ok(backend::File::new(
            OwnedFd::from(self.device.try_clone().unwrap()),
            allocation.desc.offset as i64,
            allocation.existing == 0,
        ))
    }

    fn unlink(&self, _id: &region::Id) -> crate::Result<()> {
        todo!()
    }
}

impl From<Ivshmem> for backend::Kind {
    fn from(ivshmem: Ivshmem) -> Self {
        backend::Kind::Ivshmem(ivshmem)
    }
}

#[allow(dead_code, non_camel_case_types)]
mod driver {
    use core::ffi;
    use core::ffi::CStr;
    use std::fs::File;
    use std::io;
    use std::os::fd::AsRawFd as _;

    use ribbit::private::u14;

    use crate::raw::region;

    // https://sites.uclouvain.be/SystInfo/usr/include/asm-generic/ioctl.h.html
    #[ribbit::pack(size = 32, debug)]
    #[repr(C)]
    struct Ioctl {
        function: u8,
        driver: u8,
        size: u14,
        #[ribbit(size = 2)]
        dir: Dir,
    }

    #[ribbit::pack(size = 2, debug)]
    enum Dir {
        None,
        W,
        R,
        RW,
    }

    #[repr(C)]
    #[derive(Default)]
    pub(super) struct region_desc {
        pub(super) offset: u64,
        pub(super) length: u64,
        prog_id: [u8; 12],
    }

    #[repr(C)]
    #[derive(Default)]
    pub(super) struct vcxl_find_alloc {
        pub(super) desc: region_desc,
        pub(super) existing: ffi::c_int,
    }

    const IOCTL_MAGIC: u8 = b'f';

    pub(super) fn find_cxl_alloc_nomap(
        file: &File,
        id: &region::Id,
        size: usize,
    ) -> io::Result<vcxl_find_alloc> {
        const IOCTL_FIND_ALLOC: Ioctl = Ioctl::new(
            8,
            IOCTL_MAGIC,
            u14::new(size_of::<vcxl_find_alloc>() as u16),
            Dir::new(DirUnpacked::RW),
        );

        let mut find = vcxl_find_alloc::default();
        find.desc.length = size as u64;

        assert!(
            id.len() < 12,
            "Ivshmem driver only supports IDs up to length 12 (including null byte), got {id:?}"
        );

        // Note: `to_bytes` does not include null terminator. We check above
        // that `id` length + 1 fits, and array is 0-initialized.
        find.desc.prog_id[..id.len()].copy_from_slice(id.as_bytes());

        match unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                ribbit::private::pack(IOCTL_FIND_ALLOC) as u64,
                &mut find,
            )
        } {
            0 => Ok(find),
            _ => Err(io::Error::last_os_error()),
        }
    }

    #[allow(clippy::field_reassign_with_default)]
    pub(super) fn cxl_free(file: &File, id: &CStr, offset: i64, size: usize) -> io::Result<()> {
        const IOCTL_FREE: Ioctl = Ioctl::new(
            7,
            IOCTL_MAGIC,
            u14::new(size_of::<region_desc>() as u16),
            // Possibly an issue with the driver interface? Should at least be `R`.
            Dir::new(DirUnpacked::W),
        );

        let mut free = region_desc::default();
        free.offset = offset as u64;
        free.length = size as u64;
        free.prog_id[..id.to_bytes().len()].copy_from_slice(id.to_bytes());

        match unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                ribbit::private::pack(IOCTL_FREE) as u64,
                &mut free,
            )
        } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}
