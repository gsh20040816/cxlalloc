use core::cell::UnsafeCell;

use crate::atomic::Packed;
use crate::bitset::HiBitSet;
use crate::size;
use crate::slab;
use crate::Atomic;
use crate::SIZE_BIT_SET;
use crate::SIZE_CACHE_LINE;

#[repr(C, align(64))]
pub(crate) struct Owned {
    pub(crate) meta: Atomic<Meta>,
    pub(crate) free: UnsafeCell<HiBitSet<SIZE_BIT_SET>>,
}

const _: () = assert!(size_of::<Owned>() % SIZE_CACHE_LINE == 0);

pub(crate) struct Meta(u64);

impl Meta {
    pub(crate) fn new(next: Option<slab::Index>, class: size::Class) -> Self {
        Self(next.pack() << 32 | class.pack())
    }

    pub(crate) fn next(&self) -> Option<slab::Index> {
        Packed::unpack(self.0 >> 32)
    }

    pub(crate) fn class(&self) -> size::Class {
        Packed::unpack(self.0)
    }
}

impl core::fmt::Debug for Meta {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Meta")
            .field("next", &self.next())
            .field("class", &self.class())
            .finish()
    }
}

unsafe impl Packed for Meta {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}
