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
            0..1025 => Self::Small(Small(((size + 7) >> 3) as u8)),
            _ => Self::Large(Large(size.next_multiple_of(SIZE_SLAB))),
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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Small(u8);

impl Display for Small {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

const CLASS_COUNT: usize = 128;

impl Small {
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        (0..CLASS_COUNT as u8).map(Self)
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
    let mut i = 2;
    counts[1] = (SIZE_BIT_SET * 64) as u16;
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

unsafe impl Packed for Class {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        match self {
            Class::Small(small) => small.pack(),
            Class::Large(large) => large.0 as u64,
        }
    }

    fn unpack(value: u64) -> Self {
        let inner = value as u32;
        match inner {
            index if (index as usize) <= CLASS_COUNT => Self::Small(Small::unpack(value)),
            size => Self::Large(Large(size as usize)),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Large(usize);

impl Large {
    pub(crate) fn size(&self) -> usize {
        self.0
    }

    pub(crate) fn count(&self) -> usize {
        self.0 / SIZE_SLAB
    }
}

impl Display for Large {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.0)
    }
}

/// For reference:
/// - https://jemalloc.net/jemalloc.3.html#size_classes
/// - https://github.com/ricleite/lrmalloc/blob/34c6474861ec7583ac146da5a8f39190de6db991/size_classes.h
/// - https://github.com/urcs-sync/ralloc/blob/6b9d7a1af75ba75232107bfaeb6a034799d5b182/src/SizeClass.hpp
macro_rules! sc {
    ($index:expr, $lg_grp:expr, $lg_delta:expr, $ndelta:expr) => {{
        const BLOCK_SIZE: usize = (1 << $lg_grp) + ($ndelta << $lg_delta);

        // Statically confirm that we can fit size class information in 16 bits.
        const _: () = assert!(BLOCK_SIZE < u16::MAX as usize);

        BLOCK_SIZE as u16
    }};
}
