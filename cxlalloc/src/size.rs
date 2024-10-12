use core::array;
use core::fmt;
use core::fmt::Display;
use core::ops;

use crate::atomic::Packed;
use crate::SIZE_BIT_SET;
use crate::SIZE_SLAB;

pub(crate) const MIN: usize = 8;

#[repr(transparent)]
pub(crate) struct Array<T>([T; 1 + CLASS_COUNT]);

impl<T> Array<T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Class, &T)> {
        self.0
            .iter()
            .enumerate()
            .map(|(index, element)| (Class(index as u8), element))
            .skip(1)
    }
}

impl<T> Default for Array<T>
where
    T: Default,
{
    fn default() -> Self {
        Self(array::from_fn(|_| T::default()))
    }
}

impl<T> ops::Index<Class> for Array<T> {
    type Output = T;

    fn index(&self, Class(index): Class) -> &Self::Output {
        unsafe { self.0.get_unchecked(index as usize) }
    }
}

impl<T> ops::IndexMut<Class> for Array<T> {
    fn index_mut(&mut self, Class(index): Class) -> &mut Self::Output {
        unsafe { self.0.get_unchecked_mut(index as usize) }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Class(u8);

impl Display for Class {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

// 8..1024
const CLASS_COUNT: usize = 128;

impl Class {
    #[inline]
    pub(crate) fn new(size: usize) -> Option<Self> {
        match size {
            0..1024 => Some(Class(((size + 7) / 8) as u8)),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn is_zero(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub(crate) fn size(&self) -> usize {
        self.0 as usize * 8
    }

    #[inline]
    pub(crate) fn count(&self) -> usize {
        static COUNTS: Array<u16> = counts();
        COUNTS[*self] as usize
    }
}

const fn counts() -> Array<u16> {
    let mut counts = [0u16; CLASS_COUNT + 1];

    // Special case: zero size class to defer branch
    counts[0] = 0;

    // Special case: the smallest size class has some
    // bits in its bitset reserved for slab metadata.
    counts[1] = (SIZE_BIT_SET * 64) as u16;

    let mut i = 2;
    while i < counts.len() {
        counts[i] = (SIZE_SLAB / (i * 8)) as u16;
        i += 1;
    }

    Array(counts)
}

unsafe impl Packed for Class {
    const BITS: u8 = 8;

    fn pack(&self) -> u64 {
        self.0 as u64
    }

    fn unpack(value: u64) -> Self {
        Self(value as u8)
    }
}
