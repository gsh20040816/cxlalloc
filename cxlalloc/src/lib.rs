mod allocator;
pub mod atomic;
mod bitset;
mod r#box;
pub mod cell;
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
pub use cell::UnsafeCell;
pub use heap::Heap;
pub use r#box::Box;
pub use root::Root;
pub use transfer::Transfer;

pub(crate) const SIZE_CACHE_LINE: usize = 64;
pub(crate) const SIZE_PAGE: usize = 4096;

const SIZE_METADATA: usize = if cfg!(feature = "validate") { 4 } else { 3 };

// Number of 64-bit chunks in free bitset, minus three for metadata
pub(crate) const SIZE_BIT_SET: usize = (SIZE_CACHE_LINE * 8) / 8 - SIZE_METADATA;

// Each chunk maps to 64 blocks of the minimum size class
pub(crate) const SIZE_SLAB: usize = (SIZE_BIT_SET + SIZE_METADATA) * 64 * size::MIN;

pub(crate) const COUNT_THREAD: usize = 64;
pub(crate) const COUNT_ROOT: usize = COUNT_THREAD + 1;
