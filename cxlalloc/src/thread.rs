use core::fmt::Debug;
use core::fmt::Display;
use core::num::NonZeroU16;
use core::ops::Index;

use crate::COUNT_THREAD;

#[repr(transparent)]
#[derive(ribbit::Pack, Copy, Clone, PartialEq, Eq, Hash)]
#[ribbit(size = 16, nonzero, packed(rename = "IdPacked"), new(vis = ""))]
pub struct Id(NonZeroU16);

impl Id {
    pub const unsafe fn new(id: u16) -> Self {
        assert!(id < COUNT_THREAD as u16);
        Self(NonZeroU16::new_unchecked(id.wrapping_add(1)))
    }
}

impl From<Id> for u16 {
    fn from(id: Id) -> Self {
        id.0.get() - 1
    }
}

impl Debug for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&(self.0.get() - 1), f)
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&(self.0.get() - 1), f)
    }
}

#[repr(C, align(64))]
pub struct Array<T>(pub(crate) [T; COUNT_THREAD + 1]);

impl<T> Array<T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.0.iter().skip(1)
    }
}

impl<T> Index<Id> for Array<T> {
    type Output = T;
    fn index(&self, index: Id) -> &Self::Output {
        &self.0[index.0.get() as usize]
    }
}

impl<T: Default> Default for Array<T> {
    fn default() -> Self {
        Self(core::array::from_fn(|_| T::default()))
    }
}
