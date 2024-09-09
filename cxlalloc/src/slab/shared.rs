use crate::atomic::Packed;
use crate::atomic::Version;
use crate::bitset::AtomicBitSet;
use crate::size;
use crate::Atomic;
use crate::SIZE_BIT_SET;

#[repr(C, align(64))]
pub(crate) struct Shared {
    pub(crate) meta: Atomic<Meta>,
    pub(crate) free: AtomicBitSet<SIZE_BIT_SET>,
}

#[repr(C)]
pub(crate) struct Meta(u64);

impl Meta {
    pub(crate) fn new(version: Version, class: size::Class) -> Self {
        Self(version.pack() << size::Class::BITS | class.pack())
    }

    pub(crate) fn version(&self) -> Version {
        Version::unpack(self.0 >> 32)
    }

    pub(crate) fn class(&self) -> size::Class {
        size::Class::unpack(self.0)
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
