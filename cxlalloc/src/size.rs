use core::fmt;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops;

use ribbit::private::u4;
use ribbit::private::u7;

use crate::bitset;
use crate::bitset::BitSet;
use crate::bitset::Interface as _;
use crate::SIZE_CACHE_LINE;

pub(crate) trait Bracket: ribbit::Pack<Loose = u8> + Default + Debug + 'static {
    const NAME: &'static str;

    const SIZE_SLAB: usize;
    const SIZE_MIN: usize;
    const SIZE_MAX: usize;
    const COUNT: usize;

    type Array<T>: AsRef<[T]> + AsMut<[T]>;
    type BitSet: bitset::Interface;

    fn new(size: usize) -> Option<Self>;

    fn from_index(index: usize) -> Option<Self>;

    fn array<T: Default>() -> Self::Array<T>;

    fn pack(self) -> u8;

    fn is_zero(&self) -> bool;

    fn size(&self) -> u64;

    fn count(&self) -> u64;
}

#[ribbit::pack(size = 8, eq)]
#[derive(Default)]
pub struct Huge;

impl Debug for Huge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Huge")
    }
}

impl Bracket for Huge {
    const NAME: &'static str = "huge";

    const SIZE_SLAB: usize = 1 << 30;
    const SIZE_MIN: usize = 4096;
    const SIZE_MAX: usize = 4096;
    const COUNT: usize = 1;

    type Array<T> = [T; 1];
    type BitSet = BitSet<0>;

    fn new(_: usize) -> Option<Self> {
        Some(Huge::default())
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Huge::default()),
            _ => None,
        }
    }

    fn array<T: Default>() -> Self::Array<T> {
        [T::default()]
    }

    fn pack(self) -> u8 {
        0
    }

    #[inline]
    fn is_zero(&self) -> bool {
        false
    }

    #[inline]
    fn size(&self) -> u64 {
        // HACK: used to detect huge allocation in stat module
        u64::MAX
    }

    #[inline]
    fn count(&self) -> u64 {
        unreachable!()
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub(crate) struct Array<B: Bracket, T> {
    pub(crate) inner: B::Array<T>,
    pub(crate) _bracket: PhantomData<B>,
}

impl<B: Bracket, T> Array<B, T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (B, &T)> {
        self.inner
            .as_ref()
            .iter()
            .enumerate()
            .map(|(index, element)| (B::from_index(index).unwrap(), element))
    }
}

impl<B, T> Default for Array<B, T>
where
    B: Bracket,
    T: Default,
{
    fn default() -> Self {
        Self {
            inner: B::array(),
            _bracket: PhantomData,
        }
    }
}

impl<B: Bracket, T> ops::Index<B> for Array<B, T> {
    type Output = T;

    fn index(&self, class: B) -> &Self::Output {
        unsafe { self.inner.as_ref().get_unchecked(class.pack() as usize) }
    }
}

impl<B: Bracket, T> ops::IndexMut<B> for Array<B, T> {
    fn index_mut(&mut self, class: B) -> &mut Self::Output {
        unsafe { self.inner.as_mut().get_unchecked_mut(class.pack() as usize) }
    }
}

/// 0B, 8B, 16B, 24B, ..., 1016B
#[ribbit::pack(size = 7, new(rename = "new_internal", vis = ""), eq, hash)]
#[derive(Default)]
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
            true => Some(Small::new_internal(u7::new(
                (size.next_multiple_of(8) / 8) as u8,
            ))),
            false => None,
        }
    }

    const fn counts() -> Array<Small, u16> {
        let mut counts = [0u16; Small::COUNT];

        // Special case: zero size class to defer branch
        counts[0] = 0;

        // Special case: the smallest size class has some
        // bits in its bitset reserved for slab metadata.
        counts[1] = (<Self as Bracket>::BitSet::SIZE_DATA * 8) as u16;

        let mut i = 2;
        while i < counts.len() {
            counts[i] = (Self::SIZE_SLAB / (i * 8)) as u16;
            i += 1;
        }

        Array {
            inner: counts,
            _bracket: PhantomData,
        }
    }

    pub(crate) const fn bit_sets() -> [<Self as Bracket>::BitSet; Self::COUNT] {
        let counts = Self::counts().inner;
        let mut bit_sets = [const { BitSet::new() }; Self::COUNT];

        let mut class = 0;
        while class < counts.len() {
            bit_sets[class] = BitSet::filled(counts[class] as u64);
            class += 1;
        }

        bit_sets
    }
}

impl Bracket for Small {
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
            .map(Self::new_internal)
    }

    #[inline]
    fn array<T: Default>() -> Self::Array<T> {
        core::array::from_fn(|_| T::default())
    }

    #[inline]
    fn pack(self) -> u8 {
        self._0().value()
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self._0().value() == 0
    }

    #[inline]
    fn size(&self) -> u64 {
        self._0().value() as u64 * 8
    }

    #[inline]
    fn count(&self) -> u64 {
        static COUNTS: Array<Small, u16> = Small::counts();
        COUNTS[*self] as u64
    }
}

/// 1KiB, 2KiB, ..., 1MiB
#[ribbit::pack(size = 4, new(rename = "new_internal", vis = ""), eq, hash)]
#[derive(Default)]
pub(crate) struct Large(u4);

impl Debug for Large {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

    #[inline]
    const fn count(&self) -> u64 {
        Self::SIZE_SLAB as u64 >> Self::SIZE_MIN_LOG2 >> self._0().value()
    }

    pub(crate) const fn bit_sets() -> [<Self as Bracket>::BitSet; Self::COUNT] {
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

impl Bracket for Large {
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
    fn pack(self) -> u8 {
        self._0().value()
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

#[cfg(test)]
mod test {

    use super::Bracket;
    use super::Large;
    use super::Small;

    #[test]
    fn small_consistent() {
        // Skip special size classes
        for i in 2..Small::COUNT {
            let class = Small::from_index(i).unwrap();

            if Small::SIZE_SLAB as u64 % class.size() == 0 {
                assert_eq!(
                    class.size() * class.count(),
                    Small::SIZE_SLAB as u64,
                    "Class {:?}, size {}, count {}",
                    class,
                    class.size(),
                    class.count()
                );
            } else {
                assert!(
                    class.size() * class.count() <= Small::SIZE_SLAB as u64,
                    "Class {:?}, size {}, count {}",
                    class,
                    class.size(),
                    class.count()
                );
            }
        }
    }

    #[test]
    fn large_consistent() {
        for i in 0..Large::COUNT {
            let class = Large::from_index(i).unwrap();
            assert_eq!(
                class.size() * class.count(),
                Large::SIZE_SLAB as u64,
                "Class {:?}, size {}, count {}",
                class,
                class.size(),
                class.count()
            );
        }
    }
}
