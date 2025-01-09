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

#[ribbit::pack(size = 32)]
#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct Meta {
    #[ribbit(size = 16)]
    pub(crate) version: Version,

    #[ribbit(size = 16)]
    pub(crate) claim: Option<thread::Id>,
}

#[ribbit::pack(size = 24)]
#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct Owner {
    #[ribbit(size = 8)]
    pub(crate) class: size::Small,

    #[ribbit(size = 16)]
    pub(crate) id: Option<thread::Id>,
}
