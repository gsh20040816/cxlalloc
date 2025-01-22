use core::fmt;
use core::fmt::Display;
use core::marker::PhantomData;
use core::ops;

use ribbit::private::u4;
use ribbit::private::u6;

use crate::SIZE_BIT_SET;

pub(crate) trait Bracket {
    const SIZE_SLAB: usize = (crate::SIZE_BIT_SET + crate::SIZE_METADATA) * 64 * Self::SIZE_MIN;
    const SIZE_MIN: usize;
    const SIZE_MAX: usize;
    const COUNT: usize;

    type Array<T>: AsRef<[T]> + AsMut<[T]>;

    fn array<T: Default>() -> Self::Array<T>;

    fn pack(self) -> u8;

    fn is_zero(&self) -> bool;

    fn size(&self) -> u64;

    fn count(&self) -> u64;
}

#[ribbit::pack(size = 0, eq)]
#[derive(Default)]
pub struct Huge;

impl Display for Huge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Huge")
    }
}

impl Bracket for Huge {
    const SIZE_SLAB: usize = 1 << 30;
    const SIZE_MIN: usize = 4096;
    const SIZE_MAX: usize = 4096;
    const COUNT: usize = 1;

    type Array<T> = [T; 0];

    fn array<T: Default>() -> Self::Array<T> {
        []
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
        unreachable!()
    }

    #[inline]
    fn count(&self) -> u64 {
        unreachable!()
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub(crate) struct Array<B: Bracket, T> {
    inner: B::Array<T>,
    _bracket: PhantomData<B>,
}

impl<B: Bracket, T> Array<B, T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Small, &T)> {
        self.inner
            .as_ref()
            .iter()
            .enumerate()
            .map(|(index, element)| (Small::new_internal(u6::new(index as u8)), element))
            .skip(1)
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

/// 8B, 16B, 24B, ..., 504B
#[ribbit::pack(size = 6, debug, new(rename = "new_internal", vis = ""), eq, hash)]
#[derive(Default)]
pub(crate) struct Small(u6);

impl Display for Small {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

impl Small {
    #[inline]
    pub(crate) const fn new(size: usize) -> Option<Self> {
        match size <= Self::SIZE_MAX {
            true => Some(Small::new_internal(u6::new(
                (size.next_multiple_of(8) / 8) as u8,
            ))),
            false => None,
        }
    }

    const fn counts() -> Array<Small, u16> {
        let mut counts = [0u16; Small::COUNT + 1];

        // Special case: zero size class to defer branch
        counts[0] = 0;

        // Special case: the smallest size class has some
        // bits in its bitset reserved for slab metadata.
        counts[1] = (SIZE_BIT_SET * 64) as u16;

        let mut i = 2;
        while i <= Self::COUNT {
            counts[i] = (Self::SIZE_SLAB / (i * 8)) as u16;
            i += 1;
        }

        Array {
            inner: counts,
            _bracket: PhantomData,
        }
    }
}

impl Bracket for Small {
    const SIZE_MIN: usize = 8;
    const SIZE_MAX: usize = 504;
    const COUNT: usize = 63;

    type Array<T> = [T; 1 + Self::COUNT];

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

/// 512B, 1KiB, 2KiB, ..., 512KiB
#[ribbit::pack(size = 4, debug, new(rename = "new_internal", vis = ""), eq, hash)]
#[derive(Default)]
pub(crate) struct Large(u4);

impl Display for Large {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (512 << self._0().value()).fmt(f)
    }
}

impl Large {
    pub(crate) fn new(size: usize) -> Option<Self> {
        match size <= Self::SIZE_MAX {
            true => Some(Self::new_internal(u4::new(
                (size.next_power_of_two() >> 9).trailing_zeros() as u8,
            ))),
            false => None,
        }
    }
}

impl Bracket for Large {
    const SIZE_MIN: usize = 1 << 9;
    const SIZE_MAX: usize = 1 << 19;
    const COUNT: usize = 11;

    type Array<T> = [T; Self::COUNT];

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
        512 << self._0().value()
    }

    #[inline]
    fn count(&self) -> u64 {
        const COUNT_MIN: u64 = crate::SIZE_BIT_SET as u64 * 64;
        match self._0().value() == 0 {
            true => COUNT_MIN,
            false => Self::SIZE_SLAB as u64 >> 9 >> self._0().value(),
        }
    }
}
