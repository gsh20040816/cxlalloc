use core::ffi;
use core::ffi::CStr;
use core::fmt::Display;
use core::fmt::Write as _;
use core::num::NonZeroUsize;
use core::ops::Deref;
use core::ptr::NonNull;
use std::io;

use arrayvec::ArrayString;
pub(crate) use shm::Page;
pub(crate) type Reservation = shm::Reservation<{ 1 << 40 }>;

use crate::raw::backend::Backend;

#[derive(Clone, Debug)]
pub(crate) struct Id(ArrayString<{ Self::SIZE }>);

impl Id {
    pub(crate) const SIZE: usize = 32;

    pub(crate) fn new(inner: &str) -> Self {
        ArrayString::from(inner)
            .map(Self)
            .expect("Region identifiers must be less than 32 bytes")
    }

    pub(crate) fn with_suffix<T: Display>(&self, suffix: T) -> Self {
        let mut id = self.clone().0;
        write!(&mut id, "-{}", suffix).unwrap();
        Self(id)
    }

    fn as_cstr(&self) -> &CStr {
        CStr::from_bytes_with_nul(self.0.as_bytes()).unwrap()
    }
}

impl Deref for Id {
    type Target = ArrayString<{ Self::SIZE }>;
    fn deref(&self) -> &Self::Target {
        &self.0
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

pub(crate) struct Sequential {
    id: Id,
    create: bool,
    reservation: Reservation,
    size: NonZeroUsize,
}

pub(crate) struct Random {
    id: Id,
    reservation: Reservation,
}

impl Fixed {
    pub(super) fn new(backend: &Backend, id: Id, size: NonZeroUsize) -> crate::Result<Self> {
        let size = NonZeroUsize::new(size.get().next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let file = backend.open(id.as_cstr(), size)?;
        let create = file.is_create();
        let address = unsafe {
            file.map()
                .maybe_numa(backend.numa().cloned())
                .maybe_populate(backend.populate())
                .call()?
        };
        Ok(Self {
            id,
            address,
            create,
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
        &self.id.0
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
                let file = backend.open(id.with_suffix(0).as_cstr(), size)?;
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

        Ok(Sequential {
            id,
            create,
            reservation,
            size,
        })
    }

    pub(crate) fn map(&self, backend: &Backend, offset: usize) -> crate::Result<()> {
        let index = offset / self.size.get();
        unsafe {
            backend
                .open(self.id.with_suffix(index).as_cstr(), self.size)?
                .map()
                .address(self.reservation.start().byte_add(self.size.get() * index))
                .maybe_numa(backend.numa().cloned())
                .maybe_populate(backend.populate())
                .call()?;
        }

        Ok(())
    }
}

impl Region for Sequential {
    fn address(&self) -> NonNull<Page> {
        self.reservation.start()
    }

    fn is_clean(&self) -> bool {
        self.create
    }

    fn id(&self) -> &str {
        &self.id.0
    }

    /// Remove all virtual address space mappings for this region.
    fn unmap(&self) -> crate::Result<()> {
        self.reservation.unmap()?;
        Ok(())
    }
}

impl Random {
    pub(super) fn new(id: Id, reservation: Reservation) -> io::Result<Self> {
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
                    self.id.with_suffix(format_args!("{:#x}", offset)).as_cstr(),
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
        let _ = backend.unlink(id.as_cstr());
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
        &self.id.0
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
