use std::fmt::Display;

use crate::atomic::Packed;
use crate::SIZE_PAGE;

#[repr(transparent)]
pub(crate) struct Array<T>([T; CLASS_COUNT]);

impl<T> Default for Array<T>
where
    T: Default,
{
    fn default() -> Self {
        Self(std::array::from_fn(|_| T::default()))
    }
}

impl<T> std::ops::Index<Small> for Array<T> {
    type Output = T;
    fn index(&self, Small(index): Small) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl<T> std::ops::IndexMut<Small> for Array<T> {
    fn index_mut(&mut self, Small(index): Small) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Class {
    Small(Small),
    Large(Large),
}

/// Largest size class for small allocations
const MAX: usize = CLASSES[CLASS_COUNT - 1] as usize;

const fn cache() -> [Small; MAX + 1] {
    let mut table = [Small(0); MAX + 1];
    let mut index = 0;
    let mut class = 0;

    while index < table.len() {
        if (CLASSES[class] as usize) < index {
            class += 1;
        }

        table[index] = Small(class as u8);
        index += 1;
    }

    table
}

impl Class {
    pub(crate) fn new(size: usize) -> Self {
        const CACHE: [Small; MAX + 1] = cache();

        CACHE
            .get(size)
            .copied()
            .map(Self::Small)
            .unwrap_or_else(|| {
                let aligned = size.next_multiple_of(SIZE_PAGE);
                Self::Large(Large(aligned))
            })
    }

    pub(crate) fn size(&self) -> usize {
        match self {
            Self::Small(small) => small.size(),
            Self::Large(large) => large.size(),
        }
    }
}

impl Display for Class {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Small(u8);

impl Display for Small {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

impl Small {
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        (0..CLASS_COUNT as u8).map(Self)
    }

    pub(crate) fn size(&self) -> usize {
        CLASSES[self.0 as usize] as usize
    }

    pub(crate) fn count(&self) -> usize {
        match self.size() {
            // FIXME: tie to slab descriptor bitset size
            8 => 448,
            size => crate::SIZE_SLAB / size,
        }
    }
}

impl Default for Small {
    fn default() -> Self {
        Self(0)
    }
}

unsafe impl Packed for Class {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        match self {
            Class::Small(small) => small.0 as u64,
            Class::Large(large) => large.0 as u64,
        }
    }

    fn unpack(value: u64) -> Self {
        match value {
            index if (index as usize) < CLASS_COUNT => Self::Small(Small(value as u8)),
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
}

impl Display for Large {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
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

const CLASS_COUNT: usize = 39;

#[rustfmt::skip]
#[allow(clippy::zero_prefixed_literal)]
const CLASSES: [u16; CLASS_COUNT] = [
    sc!(000, 03, 03, 0),
    sc!(001, 03, 03, 1),
    sc!(002, 03, 03, 2),
    sc!(003, 03, 03, 3),

    sc!(004, 05, 03, 1),
    sc!(005, 05, 03, 2),
    sc!(006, 05, 03, 3),
    sc!(007, 05, 03, 4),

    sc!(008, 06, 04, 1),
    sc!(009, 06, 04, 2),
    sc!(010, 06, 04, 3),
    sc!(011, 06, 04, 4),

    sc!(012, 07, 05, 1),
    sc!(013, 07, 05, 2),
    sc!(014, 07, 05, 3),
    sc!(015, 07, 05, 4),

    sc!(016, 08, 06, 1),
    sc!(017, 08, 06, 2),
    sc!(018, 08, 06, 3),
    sc!(019, 08, 06, 4),

    sc!(020, 09, 07, 1),
    sc!(021, 09, 07, 2),
    sc!(022, 09, 07, 3),
    sc!(023, 09, 07, 4),

    sc!(024, 10, 08, 1),
    sc!(025, 10, 08, 2),
    sc!(026, 10, 08, 3),
    sc!(027, 10, 08, 4),

    sc!(028, 11, 09, 1),
    sc!(029, 11, 09, 2),
    sc!(030, 11, 09, 3),
    sc!(031, 11, 09, 4),

    sc!(032, 12, 10, 1),
    sc!(033, 12, 10, 2),
    sc!(034, 12, 10, 3),
    sc!(035, 12, 10, 4),

    sc!(036, 13, 11, 1),
    sc!(037, 13, 11, 2),
    sc!(038, 13, 11, 3),
];
