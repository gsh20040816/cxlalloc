mod allocator;
pub mod atomic;
mod bitset;
mod r#box;
mod cas;
mod coherence;
mod data;
mod error;
mod heap;
mod huge;
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

pub use atomic::Atomic;
pub(crate) use data::Data;
pub use error::Error;
pub(crate) use heap::Heap;
pub(crate) use huge::Huge;
pub use r#box::Box;
pub use raw::Raw;
pub(crate) use slab::Slab;

pub(crate) const SIZE_CACHE_LINE: usize = 64;
pub(crate) const SIZE_PAGE: usize = 4096;

const SIZE_METADATA: usize = if cfg!(feature = "validate") { 3 } else { 2 };

// Number of 64-bit chunks in free bitset, minus three for metadata
pub(crate) const SIZE_BIT_SET: usize = (SIZE_CACHE_LINE * 8) / 8 - SIZE_METADATA;

pub(crate) const COUNT_THREAD: usize = 96;

pub(crate) const COUNT_CACHE_SLAB: usize = 8;
pub(crate) const BATCH_GLOBAL_PUSH: usize = 4;
pub(crate) const BATCH_BUMP_POP: u32 = 16;

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
