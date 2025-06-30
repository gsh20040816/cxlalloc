use core::fmt::Debug;

use ribbit::u4;

use crate::bitset;
use crate::bitset::BitSet;
use crate::size;
use crate::size::Bracket as _;
use crate::SIZE_CACHE_LINE;

/// 1KiB, 2KiB, ..., 1MiB
#[ribbit::pack(size = 4, new(rename = "new_internal", vis = ""), eq, hash)]
#[derive(Default)]
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
            true => Some(Self::new_internal(u4::new(
                (size.next_power_of_two() >> Self::SIZE_MIN_LOG2).trailing_zeros() as u8,
            ))),
            false => None,
        }
    }

    pub(crate) const fn from_index(index: u8) -> Self {
        Self::new_internal(u4::new(index))
    }

    #[inline]
    const fn count(&self) -> u64 {
        Self::SIZE_SLAB as u64 >> Self::SIZE_MIN_LOG2 >> self._0().value()
    }

    pub(crate) const fn bit_sets() -> [<Self as size::Bracket>::BitSet; Self::COUNT] {
        let mut bit_sets = [const { BitSet::new() }; Self::COUNT];

        let mut class = 0;
        while class < bit_sets.len() {
            let count = Self::new_internal(u4::new(class as u8)).count();
            bit_sets[class] = BitSet::filled(count);
            class += 1;
        }

        bit_sets
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
            .map(Self::new_internal)
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
        (Self::SIZE_MIN as u64) << self._0().value()
    }

    #[inline]
    fn count(&self) -> u64 {
        self.count()
    }
}
