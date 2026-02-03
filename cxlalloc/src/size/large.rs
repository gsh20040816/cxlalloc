use core::fmt::Debug;

use ribbit::u4;

use crate::bitset;
use crate::bitset::BitSet;
use crate::size;
use crate::size::Bracket as _;
use crate::SIZE_CACHE_LINE;

/// 1KiB, 2KiB, ..., 1MiB
#[repr(transparent)]
#[derive(ribbit::Pack, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[ribbit(size = 4)]
pub(crate) struct Large(u4);

impl Debug for Large {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.size().fmt(f)
    }
}

impl Large {
    const SIZE_MIN_LOG2: usize = 10;
    const SIZE_MAX_LOG2: usize = 19;

    #[inline]
    pub(crate) const fn new(size: usize) -> Option<Self> {
        match size <= Self::SIZE_MAX {
            true => Some(Self(u4::new(
                (size.next_power_of_two() >> Self::SIZE_MIN_LOG2).trailing_zeros() as u8,
            ))),
            false => None,
        }
    }

    #[allow(unused)]
    pub(crate) const fn from_index(index: u8) -> Self {
        Self(u4::new(index))
    }

    #[inline]
    const fn count(&self) -> u64 {
        Self::SIZE_SLAB as u64 >> Self::SIZE_MIN_LOG2 >> self.0.value()
    }
}

impl size::Bracket for Large {
    const NAME: &'static str = "large";

    #[expect(clippy::identity_op)]
    const SIZE_SLAB: usize = (SIZE_CACHE_LINE * 1) * 8 * Self::SIZE_MIN;
    const SIZE_MIN: usize = 1 << Self::SIZE_MIN_LOG2;
    const SIZE_MAX: usize = 1 << Self::SIZE_MAX_LOG2;
    const COUNT: usize = Self::SIZE_MAX_LOG2 - Self::SIZE_MIN_LOG2 + 1;

    type BitSet = BitSet<
        { (SIZE_CACHE_LINE * 2 - bitset::SIZE_METADATA - crate::slab::local::SIZE_METADATA) / 8 },
    >;

    type Array<T> = [T; Self::COUNT];

    #[inline]
    fn new(size: usize) -> Option<Self> {
        Self::new(size)
    }

    #[inline]
    fn from_index(index: usize) -> Option<Self> {
        u8::try_from(index)
            .ok()
            .and_then(|index| u4::try_new(index).ok())
            .map(Self)
    }

    #[inline]
    fn array<T: Default>() -> Self::Array<T> {
        core::array::from_fn(|_| T::default())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        false
    }

    #[inline]
    fn size(&self) -> u64 {
        (Self::SIZE_MIN as u64) << self.0.value()
    }

    #[inline]
    fn count(&self) -> u64 {
        self.count()
    }
}
