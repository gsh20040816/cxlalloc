use core::fmt;
use core::fmt::Debug;
use core::marker::PhantomData;

use ribbit::u7;

use crate::bitset;
use crate::bitset::BitSet;
use crate::bitset::Interface as _;
use crate::size;
use crate::size::Bracket as _;
use crate::SIZE_CACHE_LINE;

/// 0B, 8B, 16B, 24B, ..., 1016B
#[repr(transparent)]
#[derive(ribbit::Pack, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[ribbit(size = 7)]
pub(crate) struct Small(u7);

impl Debug for Small {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

impl Small {
    #[inline]
    pub(crate) const fn new(size: usize) -> Option<Self> {
        match size <= Self::SIZE_MAX {
            true => Some(Small(u7::new((size.next_multiple_of(8) / 8) as u8))),
            false => None,
        }
    }

    #[allow(unused)]
    pub(crate) const fn from_index(size: u8) -> Self {
        Self(u7::new(size))
    }

    const fn counts() -> size::Array<Small, u16> {
        let mut counts = [0u16; Small::COUNT];

        // Special case: zero size class to defer branch
        counts[0] = 0;

        // Special case: the smallest size class has some
        // bits in its bitset reserved for slab metadata.
        counts[1] = (<Self as size::Bracket>::BitSet::SIZE_DATA * 8) as u16;

        let mut i = 2;
        while i < counts.len() {
            counts[i] = (Self::SIZE_SLAB / (i * 8)) as u16;
            i += 1;
        }

        size::Array {
            inner: counts,
            _bracket: PhantomData,
        }
    }
}

impl size::Bracket for Small {
    const NAME: &'static str = "small";

    const SIZE_SLAB: usize = (SIZE_CACHE_LINE * 8) * 8 * Self::SIZE_MIN;
    const SIZE_MIN: usize = 8;
    const SIZE_MAX: usize = 1016;
    const COUNT: usize = 128;

    type Array<T> = [T; Self::COUNT];

    // Number of 64-bit chunks in free bitset, minus metadata
    type BitSet = BitSet<
        { (SIZE_CACHE_LINE * 8 - bitset::SIZE_METADATA - crate::slab::local::SIZE_METADATA) / 8 },
    >;

    #[inline]
    fn new(size: usize) -> Option<Self> {
        Self::new(size)
    }

    #[inline]
    fn from_index(index: usize) -> Option<Self> {
        u8::try_from(index)
            .ok()
            .and_then(|index| u7::try_new(index).ok())
            .map(Self)
    }

    #[inline]
    fn array<T: Default>() -> Self::Array<T> {
        core::array::from_fn(|_| T::default())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.0.value() == 0
    }

    #[inline]
    fn size(&self) -> u64 {
        self.0.value() as u64 * 8
    }

    #[inline]
    fn count(&self) -> u64 {
        static COUNTS: size::Array<Small, u16> = Small::counts();
        COUNTS[*self] as u64
    }
}
