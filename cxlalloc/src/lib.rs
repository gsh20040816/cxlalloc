mod allocator;
pub mod atomic;
mod barrier;
mod bitset;
mod r#box;
mod extend;
mod heap;
mod huge;
mod link;
pub mod raw;
mod region;
pub mod root;
mod size;
mod slab;
pub mod stat;
pub mod thread;

pub use allocator::Allocator;
pub use atomic::Atomic;
pub(crate) use barrier::Barrier;
pub use extend::Epoch;
pub use heap::Heap;
pub use r#box::Box;
pub use root::Root;

pub(crate) const SIZE_CACHE_LINE: usize = 64;
pub(crate) const SIZE_PAGE: usize = 4096;

const SIZE_METADATA: usize = if cfg!(feature = "validate") { 4 } else { 3 };

// Number of 64-bit chunks in free bitset, minus three for metadata
pub(crate) const SIZE_BIT_SET: usize = (SIZE_CACHE_LINE * 8) / 8 - SIZE_METADATA;

// Each chunk maps to 64 blocks of the minimum size class
pub(crate) const SIZE_SLAB: usize = (SIZE_BIT_SET + SIZE_METADATA) * 64 * size::MIN;

pub(crate) const COUNT_THREAD: usize = 96;
pub(crate) const COUNT_ROOT: usize = COUNT_THREAD + 1;

pub(crate) const COUNT_CACHE_SLAB: usize = 32;
pub(crate) const BATCH_GLOBAL_PUSH: usize = 24;
pub(crate) const BATCH_BUMP_POP: u32 = 16;

#[inline]
pub(crate) fn flush<T>(address: &T, invalidate: bool) {
    if cfg!(feature = "arch-gpf") {
        return;
    }

    fn inner(address: *const u8, invalidate: bool) {
        unsafe {
            match invalidate {
                false if cfg!(feature = "arch-clwb") => core::arch::asm! {
                    "clwb {address}",
                    address = in(reg) address,
                },
                _ if cfg!(feature = "arch-clflushopt") => core::arch::asm! {
                    "clflushopt {address}",
                    address = in(reg) address,
                },
                _ => core::arch::x86_64::_mm_clflush(address),
            }
        }
    }

    for line in 0..size_of::<T>() / SIZE_CACHE_LINE {
        inner(
            (address as *const T as *const u8).wrapping_byte_add(line * SIZE_CACHE_LINE),
            invalidate,
        );
    }
}

#[inline]
pub(crate) fn fence() {
    // CLFLUSH is serializing, so we don't need a fence.
    if cfg!(not(any(
        feature = "arch-gpf",
        feature = "arch-clwb",
        feature = "arch-clflushopt"
    ))) {
        unsafe {
            core::arch::x86_64::_mm_sfence();
        }
    }
}
