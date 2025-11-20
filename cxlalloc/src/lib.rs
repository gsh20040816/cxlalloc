macro_rules! validate {
    ($($tt:tt)*) => {
        if cfg!(any(feature = "validate", debug_assertions)) {
            assert!($($tt)*);
        }
    };
}

macro_rules! validate_eq {
    ($($tt:tt)*) => {
        if cfg!(any(feature = "validate", debug_assertions)) {
            assert_eq!($($tt)*);
        }
    };
}

mod allocator;
mod bitset;
mod r#box;
mod cache;
mod cas;
mod data;
mod error;
mod heap;
mod huge;
#[cfg(feature = "cxl-mcas")]
mod mcas;
pub mod raw;
mod recover;
mod size;
mod slab;
pub mod stat;
pub mod thread;
mod view;

#[cfg(test)]
mod crash;

#[cfg(not(test))]
mod crash {
    macro_rules! define {
        ($_:ident) => {};
    }

    pub(crate) use define;
}

use core::ops::Deref;
use core::ops::DerefMut;
use core::sync::atomic::AtomicUsize;

pub(crate) use data::Data;
pub use error::Error;
pub(crate) use heap::Heap;
pub(crate) use huge::Huge;
pub use r#box::Box;
pub use raw::Raw;
pub(crate) use slab::Slab;

pub(crate) const SIZE_CACHE_LINE: usize = 64;
pub(crate) const SIZE_PAGE: usize = 4096;

pub(crate) const COUNT_THREAD: usize = 512;

pub(crate) static COUNT_CACHE_SLAB: AtomicUsize = AtomicUsize::new(0);
pub(crate) static BATCH_GLOBAL_PUSH: AtomicUsize = AtomicUsize::new(1);
pub(crate) static BATCH_BUMP_POP: AtomicUsize = AtomicUsize::new(1);

pub struct Allocator<'raw, S: 'raw = (), O: 'raw = ()>(
    allocator::Allocator<'raw, view::Focus, S, O>,
);

impl<'raw, S: 'raw, O: 'raw> Allocator<'raw, S, O> {
    pub(crate) fn new(inner: allocator::Allocator<'raw, view::Focus, S, O>) -> Self {
        Self(inner)
    }
}

impl<'raw, S: 'raw, O: 'raw> Deref for Allocator<'raw, S, O> {
    type Target = allocator::Allocator<'raw, view::Focus, S, O>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, O> DerefMut for Allocator<'_, S, O> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub type Result<T> = core::result::Result<T, Error>;
