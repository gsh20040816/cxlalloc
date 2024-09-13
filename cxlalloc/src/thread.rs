use core::num::NonZeroU16;
use core::ops::Index;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::COUNT_THREAD;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Id(NonZeroU16);

impl Id {
    pub const unsafe fn new(id: u16) -> Self {
        assert!(id < u16::MAX);
        Self(NonZeroU16::new_unchecked(id.wrapping_add(1)))
    }

    pub(crate) fn index(&self) -> u16 {
        self.0.get() - 1
    }
}

unsafe impl Packed for Id {
    const BITS: u8 = 16;

    fn pack(&self) -> u64 {
        self.0.get() as u64
    }

    fn unpack(value: u64) -> Self {
        unsafe { Id(NonZeroU16::new_unchecked(value as u16)) }
    }
}

unsafe impl NonZero for Id {}

#[repr(C)]
pub struct Array<T>([T; COUNT_THREAD]);

impl<T> Index<Id> for Array<T> {
    type Output = T;
    fn index(&self, index: Id) -> &Self::Output {
        &self.0[index.0.get() as usize]
    }
}
