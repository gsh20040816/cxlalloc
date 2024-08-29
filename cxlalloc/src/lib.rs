mod allocator;
mod block;
mod heap;
pub mod raw;
mod region;
mod root;
mod size;
mod slab;
mod thread;

pub use heap::Heap;

pub(crate) const SIZE_PAGE: usize = 4096;
pub(crate) const SIZE_SLAB: usize = SIZE_PAGE;

pub(crate) const COUNT_THREAD: usize = 64;
pub(crate) const COUNT_ROOT: usize = COUNT_THREAD + 1;
