use core::ffi;
use core::fmt::Display;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use std::io;
use std::io::Write as _;

pub(crate) use shm::Page;
pub(crate) type Reservation = shm::Reservation<{ 1 << 40 }>;

use crate::raw::backend::Backend;

#[derive(Clone, Debug)]
pub(crate) struct Id {
    buffer: [u8; Self::SIZE],
    len: usize,
}

impl Id {
    pub(crate) const SIZE: usize = 64;

    pub(crate) fn new(inner: &str) -> Self {
        let mut buffer = [0u8; Self::SIZE];
        buffer[0..][..inner.len()].copy_from_slice(inner.as_bytes());
        Self {
            buffer,
            len: inner.len(),
        }
    }

    pub(crate) fn with_suffix<T: Display>(&self, suffix: T) -> Self {
        let mut buffer = self.clone().buffer;
        let mut cursor = std::io::Cursor::new(&mut buffer[self.len..]);
        write!(cursor, "-{}", suffix).unwrap();
        let last = buffer.iter().rposition(|byte| *byte != 0).unwrap();
        Self {
            buffer,
            len: last + 1,
        }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buffer[..self.len]).unwrap()
    }
}

pub(crate) trait Region {
    fn address(&self) -> NonNull<Page>;
    fn is_clean(&self) -> bool;
    fn id(&self) -> &str;
    fn unmap(&self) -> crate::Result<()>;
}

pub(crate) struct Fixed {
    id: Id,
    create: bool,
    address: NonNull<Page>,
    size: NonZeroUsize,
}

pub(crate) enum Sequential {
    Normal {
        id: Id,
        create: bool,
        reservation: Reservation,
        size: NonZeroUsize,
    },

    #[cfg_attr(not(feature = "cxl-mcas"), expect(unused))]
    Mcas {
        id: Id,
        address: NonNull<Page>,
        size: NonZeroUsize,
    },
}

pub(crate) struct Random {
    id: Id,
    reservation: Reservation,
}

impl Fixed {
    pub(super) fn new(backend: &Backend, id: Id, size: NonZeroUsize) -> crate::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let file = backend.open(id.as_str(), size)?;
        let create = file.is_create();
        let address = unsafe {
            file.map()
                .maybe_numa(backend.numa().cloned())
                .maybe_populate(backend.populate())
                .call()?
        };

        log::debug!(
            "New fixed region with id={}, size={:#x?}, address={:#x?}",
            id.as_str(),
            size,
            address,
        );

        Ok(Self {
            id,
            address,
            create,
            size,
        })
    }

    #[cfg(feature = "cxl-mcas")]
    pub(super) fn new_mcas(id: Id, size: NonZeroUsize) -> crate::Result<Self> {
        let mcas = crate::mcas::init_process();

        log::debug!(
            "New fixed MCAS region with id={}, size={:#x?}, address={:#x?}",
            id.as_str(),
            size,
            mcas.address(),
        );

        Ok(Fixed {
            id,
            create: true,
            address: mcas.address(),
            size,
        })
    }
}

impl Region for Fixed {
    fn address(&self) -> NonNull<Page> {
        self.address
    }

    fn is_clean(&self) -> bool {
        self.create
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn unmap(&self) -> crate::Result<()> {
        unsafe { munmap(self.address, self.size) }
    }
}

impl Sequential {
    pub(super) fn new(
        backend: &Backend,
        id: Id,
        reservation: Reservation,
        size: NonZeroUsize,
        lazy: bool,
    ) -> crate::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let create = match lazy {
            true => false,
            false => {
                let file = backend.open(id.with_suffix(0).as_str(), size)?;
                let create = file.is_create();
                unsafe {
                    file.map()
                        .address(reservation.start())
                        .maybe_numa(backend.numa().cloned())
                        .maybe_populate(backend.populate())
                        .call()?
                };
                create
            }
        };

        log::debug!(
            "New sequential region with id={}, size={:#x?}, reservation={:#x?}, lazy={}",
            id.as_str(),
            size,
            reservation,
            lazy,
        );

        Ok(Sequential::Normal {
            id,
            create,
            reservation,
            size,
        })
    }

    #[cfg(feature = "cxl-mcas")]
    pub(super) fn new_mcas(id: Id, size: NonZeroUsize) -> crate::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();

        // FIXME: hard-coded for small heap
        let offset = crate::Raw::shared()
            .0
            .get()
            .next_multiple_of(crate::SIZE_PAGE);

        assert!(
            offset < crate::mcas::Buffer::SIZE_TARGET,
            "No room for sequential region of size {:x?} at offset {:x?} in target buffer of size {:x?}",
            size,
            offset,
            crate::mcas::Buffer::SIZE_TARGET,
        );

        let address = unsafe { crate::mcas::init_process().address().byte_add(offset) };

        log::debug!(
            "New sequential MCAS region with id={}, size={:#x?}, address={:#x?}",
            id.as_str(),
            size,
            address,
        );

        Ok(Sequential::Mcas { id, address, size })
    }

    fn size(&self) -> &NonZeroUsize {
        match self {
            Sequential::Normal { size, .. } => size,
            Sequential::Mcas { size, .. } => size,
        }
    }

    fn id(&self) -> &Id {
        match self {
            Sequential::Normal { id, .. } => id,
            Sequential::Mcas { id, .. } => id,
        }
    }

    pub(crate) fn map(&self, backend: &Backend, offset: usize) -> crate::Result<()> {
        let index = offset / self.size().get();

        match self {
            Sequential::Normal {
                id,
                create: _,
                reservation,
                size,
            } => unsafe {
                backend
                    .open(id.with_suffix(index).as_str(), *size)?
                    .map()
                    .address(reservation.start().byte_add(self.size().get() * index))
                    .maybe_numa(backend.numa().cloned())
                    .maybe_populate(backend.populate())
                    .call()?;
            },

            Sequential::Mcas { .. } => unreachable!(),
        }

        Ok(())
    }
}

impl Region for Sequential {
    fn address(&self) -> NonNull<Page> {
        match self {
            Sequential::Normal { reservation, .. } => reservation.start(),
            Sequential::Mcas { address, .. } => *address,
        }
    }

    fn is_clean(&self) -> bool {
        match self {
            Sequential::Normal { create, .. } => *create,
            Sequential::Mcas { .. } => false,
        }
    }

    fn id(&self) -> &str {
        self.id().as_str()
    }

    /// Remove all virtual address space mappings for this region.
    fn unmap(&self) -> crate::Result<()> {
        match self {
            Sequential::Normal { reservation, .. } => reservation.unmap()?,
            Sequential::Mcas { .. } => (),
        }

        Ok(())
    }
}

impl Random {
    pub(super) fn new(id: Id, reservation: Reservation) -> io::Result<Self> {
        log::debug!(
            "New random region with id={}, reservation={:#x?}",
            id.as_str(),
            reservation,
        );

        Ok(Random { id, reservation })
    }

    pub(crate) fn contains(&self, pointer: NonNull<ffi::c_void>) -> bool {
        (self.reservation.start().cast()..self.reservation.end().cast()).contains(&pointer)
    }

    pub(crate) fn map(
        &self,
        backend: &Backend,
        offset: usize,
        size: NonZeroUsize,
    ) -> crate::Result<()> {
        unsafe {
            backend
                .open(
                    self.id.with_suffix(format_args!("{:#x}", offset)).as_str(),
                    size,
                )?
                .map()
                .address(self.address().byte_add(offset))
                .maybe_numa(backend.numa().cloned())
                .maybe_populate(backend.populate())
                .call()?;
        }

        Ok(())
    }

    pub(crate) fn unmap(&self, backend: &Backend, offset: usize, size: NonZeroUsize) {
        let id = self.id.with_suffix(format_args!("{:#x}", offset));
        let _ = unsafe { munmap(self.address().byte_add(offset), size) };
        let _ = backend.unlink(id.as_str());
    }
}

impl Region for Random {
    fn address(&self) -> NonNull<Page> {
        self.reservation.start()
    }

    fn is_clean(&self) -> bool {
        false
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn unmap(&self) -> crate::Result<()> {
        self.reservation.unmap()?;
        Ok(())
    }
}

unsafe fn munmap(address: NonNull<Page>, size: NonZeroUsize) -> crate::Result<()> {
    match unsafe { libc::munmap(address.as_ptr().cast(), size.get()) } {
        0 => Ok(()),
        _ => Err(crate::Error::Munmap(io::Error::last_os_error())),
    }
}
