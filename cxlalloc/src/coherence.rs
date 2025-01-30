use crate::stat;
use crate::SIZE_CACHE_LINE;

#[derive(Copy, Clone, Debug)]
pub(crate) enum Invalidate {
    No,
    Yes,
}

impl From<Invalidate> for bool {
    fn from(invalidate: Invalidate) -> Self {
        match invalidate {
            Invalidate::No => false,
            Invalidate::Yes => true,
        }
    }
}

#[inline]
pub(crate) fn flush<T>(address: &T, invalidate: Invalidate) {
    if cfg!(feature = "arch-gpf") {
        return;
    }

    for line in 0..size_of::<T>().next_multiple_of(SIZE_CACHE_LINE) / SIZE_CACHE_LINE {
        clflush(
            (address as *const T as *const u8).wrapping_byte_add(line * SIZE_CACHE_LINE),
            invalidate,
        );
    }
}

#[inline]
pub(crate) fn clflush(address: *const u8, invalidate: Invalidate) {
    unsafe {
        match invalidate {
            Invalidate::No if cfg!(feature = "arch-clwb") => core::arch::asm! {
                "clwb [{address}]",
                address = in(reg) address,
                options(nomem, preserves_flags, nostack),
            },
            _ if cfg!(feature = "arch-clflushopt") => core::arch::asm! {
                "clflushopt [{address}]",
                address = in(reg) address,
                options(nomem, preserves_flags, nostack),
            },
            _ => core::arch::x86_64::_mm_clflush(address),
        }
    }
}

#[inline]
pub(crate) fn sfence() {
    // CLFLUSH is serializing, so we don't need a fence.
    if cfg!(any(
        feature = "arch-gpf",
        feature = "arch-clwb",
        feature = "arch-clflushopt"
    )) {
        unsafe {
            core::arch::x86_64::_mm_sfence();
        }
    }
}
