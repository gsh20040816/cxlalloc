use core::ffi;
use core::fmt::Display;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use std::io;
use std::io::Write as _;

pub(crate) use shm::Page;
#[cfg(feature = "small-reservation")]
const SIZE_RESERVATION: usize = 1 << 36;
#[cfg(not(feature = "small-reservation"))]
const SIZE_RESERVATION: usize = 1 << 40;

use crate::raw::backend::Backend;

pub(crate) struct Reservation {
    address: NonNull<Page>,
    size: NonZeroUsize,
    owned: Option<shm::Reservation<SIZE_RESERVATION>>,
    fixed: bool,
}

impl Reservation {
    pub(crate) const SIZE: NonZeroUsize = NonZeroUsize::new(SIZE_RESERVATION).unwrap();

    pub(crate) fn new() -> crate::Result<Self> {
        let owned = shm::Reservation::<SIZE_RESERVATION>::new()?;
        let address = owned.start();
        Ok(Self {
            address,
            size: Self::SIZE,
            owned: Some(owned),
            fixed: false,
        })
    }

    pub(crate) fn new_at(address: NonNull<Page>, size: NonZeroUsize) -> Self {
        Self {
            address,
            size,
            owned: None,
            fixed: true,
        }
    }

    pub(crate) fn new_contiguous<const COUNT: usize>() -> crate::Result<[Self; COUNT]> {
        let owned = shm::Reservation::<SIZE_RESERVATION>::new_contiguous::<COUNT>()?;
        Ok(owned.map(|reservation| {
            let address = reservation.start();
            Self {
                address,
                size: Self::SIZE,
                owned: Some(reservation),
                fixed: false,
            }
        }))
    }

    pub(crate) fn start(&self) -> NonNull<Page> {
        self.address
    }

    pub(crate) fn end(&self) -> NonNull<Page> {
        unsafe { self.address.byte_add(self.size.get()) }
    }

    pub(crate) fn size(&self) -> NonZeroUsize {
        self.size
    }

    pub(crate) fn unmap(&self) -> crate::Result<()> {
        match &self.owned {
            Some(owned) => Ok(owned.unmap()?),
            None if self.fixed => unsafe { munmap(self.address, Self::SIZE) },
            None => Ok(()),
        }
    }
}

impl core::fmt::Debug for Reservation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let lo = self.address;
        let hi = self.address.as_ptr().wrapping_byte_add(SIZE_RESERVATION);
        lo.fmt(f)?;
        write!(f, "..")?;
        hi.fmt(f)
    }
}

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
        write!(cursor, "-{suffix}").unwrap();
        let last = buffer.iter().rposition(|byte| *byte != 0).unwrap();
        Self {
            buffer,
            len: last + 1,
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buffer[..self.len]).unwrap()
    }
}

pub(crate) trait Region {
    fn address(&self) -> NonNull<Page>;
    fn size(&self) -> NonZeroUsize;
    fn is_clean(&self) -> bool;
    fn id(&self) -> &str;
    fn unmap(&self) -> crate::Result<()>;
}

pub(crate) struct Fixed {
    id: Id,
    create: bool,
    address: NonNull<Page>,
    unmap_size: NonZeroUsize,
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
            unmap_size: size,
        })
    }

    pub(super) fn new_at(
        backend: &Backend,
        id: Id,
        size: NonZeroUsize,
        address: NonNull<Page>,
    ) -> crate::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let file = backend.open(id.as_str(), size)?;
        let create = file.is_create();
        let mapped = unsafe {
            file.map()
                .address(address)
                .maybe_numa(backend.numa().cloned())
                .maybe_populate(backend.populate())
                .call()?
        };
        assert_eq!(mapped, address);

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
            unmap_size: Reservation::SIZE,
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
            unmap_size: size,
        })
    }
}

impl Region for Fixed {
    fn address(&self) -> NonNull<Page> {
        self.address
    }

    fn size(&self) -> NonZeroUsize {
        self.unmap_size
    }

    fn is_clean(&self) -> bool {
        self.create
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn unmap(&self) -> crate::Result<()> {
        unsafe { munmap(self.address, self.unmap_size) }
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
            (offset + size.get()) < crate::mcas::Buffer::SIZE_TARGET,
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

    fn size(&self) -> NonZeroUsize {
        match self {
            Sequential::Normal { reservation, .. } => reservation.size(),
            Sequential::Mcas { size, .. } => *size,
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
                    self.id.with_suffix(format_args!("{offset:#x}")).as_str(),
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
        let id = self.id.with_suffix(format_args!("{offset:#x}"));
        let _ = unsafe { munmap(self.address().byte_add(offset), size) };
        let _ = backend.unlink(id.as_str());
    }
}

impl Region for Random {
    fn address(&self) -> NonNull<Page> {
        self.reservation.start()
    }

    fn size(&self) -> NonZeroUsize {
        self.reservation.size()
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

pub(crate) fn fixed_reservation(address: NonNull<Page>, size: NonZeroUsize) -> Reservation {
    Reservation::new_at(address, size)
}

pub(crate) fn reserve_address_space(address: NonNull<Page>, size: usize) -> crate::Result<()> {
    unsafe {
        let result = libc::mmap64(
            address.as_ptr().cast(),
            size,
            libc::PROT_NONE,
            libc::MAP_ANONYMOUS | libc::MAP_PRIVATE | libc::MAP_FIXED_NOREPLACE,
            -1,
            0,
        );
        if result == libc::MAP_FAILED {
            return Err(crate::Error::Mmap(io::Error::last_os_error()));
        }
        if result != address.as_ptr().cast() {
            let _ = libc::munmap(result, size);
            return Err(crate::Error::Mmap(io::Error::from_raw_os_error(
                libc::ENOMEM,
            )));
        }
    }
    Ok(())
}
