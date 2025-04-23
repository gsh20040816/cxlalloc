use core::marker::PhantomData;
use core::mem;
use std::ffi::CString;
use std::io;

mod barrier;
mod error;
mod raw;

pub use barrier::Barrier;
pub use error::Error;
pub(crate) use error::try_libc;
pub use raw::Raw;

pub type Result<T> = std::result::Result<T, Error>;

use bon::bon;

const PAGE: usize = 4096;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "policy", rename_all = "snake_case"))]
pub enum Numa {
    Bind { node: usize },
    Interleave { nodes: Vec<usize> },
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Populate {
    PageTable,
    Physical,
}

pub struct Shm<T> {
    inner: Raw,
    r#type: PhantomData<T>,
}

#[bon]
impl<T> Shm<T> {
    #[builder]
    pub fn new(
        numa: Option<Numa>,
        name: CString,
        #[builder(default)] create: bool,
        populate: Option<Populate>,
    ) -> crate::Result<Self> {
        let inner = Raw::builder()
            .maybe_numa(numa)
            .name(name)
            .size(Self::SIZE)
            .create(create)
            .maybe_populate(populate)
            .build()?;

        Ok(Self {
            inner,
            r#type: PhantomData,
        })
    }
}

impl<T> Shm<T> {
    const SIZE: usize = mem::size_of::<T>().next_multiple_of(PAGE);

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

    pub fn unlink(&mut self) -> crate::Result<()> {
        self.inner.unlink()
    }
}
