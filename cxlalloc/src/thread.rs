use core::fmt::Debug;
use core::fmt::Display;
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

impl From<Id> for u16 {
    fn from(id: Id) -> Self {
        id.0.get() - 1
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[repr(C)]
pub struct Array<T>([T; COUNT_THREAD]);

impl<T> Index<Id> for Array<T> {
    type Output = T;
    fn index(&self, index: Id) -> &Self::Output {
        &self.0[index.0.get() as usize]
    }
}

impl<T> Array<T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Id, &T)> {
        self.0
            .iter()
            .enumerate()
            .map(|(index, value)| (unsafe { Id::new(index as u16) }, value))
    }
}
