mod allocator;
pub mod atomic;
mod bitset;
mod r#box;
mod heap;
mod link;
pub mod raw;
mod region;
pub mod root;
mod size;
mod slab;
pub mod thread;
pub mod transfer;

pub use allocator::Allocator;
pub use atomic::Atomic;
pub(crate) use bitset::BitSet;
pub use heap::Heap;
pub use r#box::Box;
pub use root::Root;
pub use transfer::Transfer;

pub(crate) const SIZE_PAGE: usize = 4096;
pub(crate) const SIZE_SLAB: usize = SIZE_PAGE;

pub(crate) const COUNT_THREAD: usize = 64;
pub(crate) const COUNT_ROOT: usize = COUNT_THREAD + 1;
