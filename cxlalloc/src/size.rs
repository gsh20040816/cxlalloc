use core::array;
use core::fmt;
use core::fmt::Display;
use core::num::NonZeroU16;
use core::ops;

use crate::atomic::Packed;
use crate::SIZE_BIT_SET;
use crate::SIZE_SLAB;

pub(crate) const MIN: usize = 8;

#[repr(transparent)]
pub(crate) struct Array<T>([T; 1 + CLASS_COUNT]);

impl<T> Array<T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Small, &T)> {
        self.0
            .iter()
            .enumerate()
            .map(|(index, element)| (Small(index as u8), element))
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

impl<T> ops::Index<Small> for Array<T> {
    type Output = T;

    fn index(&self, Small(index): Small) -> &Self::Output {
        unsafe { self.0.get_unchecked(index as usize) }
    }
}

impl<T> ops::IndexMut<Small> for Array<T> {
    fn index_mut(&mut self, Small(index): Small) -> &mut Self::Output {
        unsafe { self.0.get_unchecked_mut(index as usize) }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Class {
    Small(Small),
    Large(Large),
}

impl Class {
    #[inline]
    pub(crate) fn new(size: usize) -> Self {
        match size {
            0..1025 => Self::Small(Small(((size + 7) / 8) as u8)),
            _ => Self::Large(Large(unsafe {
                NonZeroU16::new_unchecked(((size + SIZE_SLAB - 1) / SIZE_SLAB) as u16)
            })),
        }
    }

    pub(crate) fn size(&self) -> usize {
        match self {
            Self::Small(small) => small.size(),
            Self::Large(large) => large.size(),
        }
    }
}

impl Display for Class {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

unsafe impl Packed for Class {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        match self {
            Class::Small(small) => small.pack(),
            Class::Large(large) => large.pack() << Small::BITS,
        }
    }

    fn unpack(value: u64) -> Self {
        if value as u32 as usize <= CLASS_COUNT {
            Self::Small(Small::unpack(value))
        } else {
            Self::Large(Large::unpack(value >> Small::BITS))
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Small(u8);

impl Display for Small {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

const CLASS_COUNT: usize = 128;

impl Small {
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

unsafe impl Packed for Small {
    const BITS: u8 = 8;

    fn pack(&self) -> u64 {
        self.0 as u64
    }

    fn unpack(value: u64) -> Self {
        Self(value as u8)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Large(NonZeroU16);

impl Large {
    pub(crate) fn size(&self) -> usize {
        self.0.get() as usize * SIZE_SLAB
    }

    pub(crate) fn count(&self) -> usize {
        self.0.get() as usize
    }
}

impl Display for Large {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.0)
    }
}

unsafe impl Packed for Large {
    const BITS: u8 = 16;

    fn pack(&self) -> u64 {
        self.0.get() as u64
    }

    fn unpack(value: u64) -> Self {
        Self(unsafe { NonZeroU16::new_unchecked(value as u16) })
    }
}
