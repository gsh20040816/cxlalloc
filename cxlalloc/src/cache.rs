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
pub(crate) fn flush_cxl<T>(address: &T) {
    if cfg!(any(
        // Recovery flushing is more fine-grained
        feature = "recover-flush",
        not(any(feature = "cxl-limited", feature = "cxl-mcas"))
    )) {
        return;
    }

    clflush_all(address, Invalidate::Yes)
}

#[inline]
pub(crate) fn fence_cxl() {
    if cfg!(any(
        feature = "recover-flush",
        not(any(feature = "cxl-limited", feature = "cxl-mcas")),
        // CLFLUSH is serializing, so we don't need a fence.
        not(any(feature = "arch-clwb", feature = "arch-clflushopt"))
    )) {
        return;
    }

    unsafe {
        core::arch::x86_64::_mm_sfence();
    }
}

#[inline]
pub(crate) fn flush<T>(address: *const T, invalidate: Invalidate) {
    if !cfg!(feature = "recover-flush") || cfg!(feature = "arch-gpf") {
        return;
    }

    clflush_all(address, invalidate)
}

#[inline]
pub(crate) fn fence() {
    if !cfg!(feature = "recover-flush") {
        return;
    }

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

#[inline]
pub(crate) fn clflush_all<T>(address: *const T, invalidate: Invalidate) {
    for line in 0..size_of::<T>().next_multiple_of(SIZE_CACHE_LINE) / SIZE_CACHE_LINE {
        clflush(
            (address as *const u8).wrapping_byte_add(line * SIZE_CACHE_LINE),
            invalidate,
        );
    }
}

#[inline]
fn clflush(address: *const u8, invalidate: Invalidate) {
    unsafe {
        match invalidate {
            Invalidate::No if cfg!(feature = "arch-clwb") => core::arch::asm! {
                "clwb [{address}]",
                address = in(reg) address,
                options(preserves_flags, nostack),
            },
            _ if cfg!(feature = "arch-clflushopt") => core::arch::asm! {
                "clflushopt [{address}]",
                address = in(reg) address,
                options(preserves_flags, nostack),
            },
            _ => core::arch::x86_64::_mm_clflush(address),
        }
    }
}
