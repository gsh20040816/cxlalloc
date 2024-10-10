use crate::atomic::Packed;
use crate::atomic::Version;
use crate::bitset::AtomicBitSet;
use crate::size;
use crate::thread;
use crate::Atomic;
use crate::SIZE_BIT_SET;

#[repr(C, align(64))]
pub(crate) struct Shared {
    pub(crate) meta: Atomic<Meta>,
    pub(crate) owner: Atomic<Owner>,
    pub(crate) free: AtomicBitSet<SIZE_BIT_SET>,
}

const _: [(); 8 * 64] = [(); size_of::<Shared>()];

#[repr(C)]
pub(crate) struct Meta(u64);

impl Meta {
    pub(crate) fn new(version: Version, claim: Option<thread::Id>) -> Self {
        Self(claim.pack() << Version::BITS | version.pack())
    }

    pub(crate) fn version(&self) -> Version {
        Version::unpack(self.0)
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

#[repr(C)]
pub(crate) struct Owner(u64);

impl Owner {
    pub(crate) fn new(class: size::Class, id: Option<thread::Id>) -> Self {
        Self(class.pack() << Option::<thread::Id>::BITS | id.pack())
    }

    pub(crate) fn class(&self) -> size::Class {
        Packed::unpack(self.0 >> thread::Id::BITS)
    }

    pub(crate) fn id(&self) -> Option<thread::Id> {
        Option::<_>::unpack((self.0) as u16 as u64)
    }
}

unsafe impl Packed for Owner {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}
