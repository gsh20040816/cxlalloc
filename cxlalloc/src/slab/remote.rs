use crate::atomic::Version;
use crate::bitset::AtomicBitSet;
use crate::thread;
use crate::Atomic;
use crate::SIZE_BIT_SET;

#[repr(C, align(64))]
pub(crate) struct Remote<B> {
    pub(crate) meta: Atomic<Meta>,
    pub(crate) owner: Atomic<Owner<B>>,
    pub(crate) free: AtomicBitSet<SIZE_BIT_SET>,
}

#[ribbit::pack(size = 64, eq)]
#[repr(C)]
pub(crate) struct Meta {
    #[ribbit(size = 32)]
    pub(crate) version: Version,

    #[ribbit(size = 32)]
    pub(crate) claim: Option<thread::Id>,
}

#[ribbit::pack(size = 64)]
#[repr(C)]
pub(crate) struct Owner<B> {
    #[ribbit(size = 32)]
    pub(crate) class: B,

    #[ribbit(size = 32)]
    pub(crate) id: Option<thread::Id>,
}
