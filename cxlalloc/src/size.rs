use core::array;
use core::fmt;
use core::fmt::Display;
use core::marker::PhantomData;
use core::ops;

use crate::SIZE_BIT_SET;
use crate::SIZE_SLAB;

pub(crate) const MIN: usize = 8;

pub(crate) trait Bracket: ribbit::Pack<Loose = u8> + Display + Default + Eq {
    const SIZE_SLAB: usize;

    fn pack(self) -> u8 {
        ribbit::private::pack(self)
    }

    fn is_min(&self) -> bool;

    fn is_max(&self) -> bool;

    fn size(&self) -> u64;

    fn count(&self) -> u64;
}

#[repr(transparent)]
#[derive(Debug)]
pub(crate) struct Array<B, T> {
    inner: [T; 1 + CLASS_COUNT],
    _bracket: PhantomData<B>,
}

impl<B, T> Array<B, T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Small, &T)> {
        self.inner
            .iter()
            .enumerate()
            .map(|(index, element)| (Small::new_internal(index as u8), element))
            .skip(1)
    }
}

impl<B, T> Default for Array<B, T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            inner: array::from_fn(|_| T::default()),
            _bracket: PhantomData,
        }
    }
}

impl<B, T> ops::Index<B> for Array<B, T>
where
    B: Bracket,
{
    type Output = T;

    fn index(&self, class: B) -> &Self::Output {
        unsafe { self.inner.get_unchecked(class.pack() as usize) }
    }
}

impl<B, T> ops::IndexMut<B> for Array<B, T>
where
    B: Bracket,
{
    fn index_mut(&mut self, class: B) -> &mut Self::Output {
        unsafe { self.inner.get_unchecked_mut(class.pack() as usize) }
    }
}

#[ribbit::pack(size = 8, debug, new(rename = "new_internal", vis = ""))]
#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct Small(u8);

impl Display for Small {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

// 8..1024 4k..32k
const CLASS_COUNT: usize = 128 + 5;

pub(crate) const SLAB: Small = match Small::new(SIZE_SLAB) {
    None => unreachable!(),
    Some(class) => class,
};

impl Small {
    #[inline]
    pub(crate) const fn new(size: usize) -> Option<Self> {
        match size {
            0..1024 => Some(Small::new_internal(((size + 7) / 8) as u8)),
            1024..=32768 => Some(Small::new_internal(
                128 + size.next_power_of_two().trailing_zeros() as u8 - 10,
            )),
            _ => None,
        }
    }
}

impl Bracket for Small {
    // TODO: get rid of crate::SIZE_SLAB
    const SIZE_SLAB: usize = crate::SIZE_SLAB;

    #[inline]
    fn is_min(&self) -> bool {
        self._0() == 0
    }

    #[inline]
    fn is_max(&self) -> bool {
        *self == SLAB
    }

    #[inline]
    fn size(&self) -> u64 {
        match self._0().checked_sub(128) {
            None => self._0() as u64 * 8,
            Some(p) => 1024 << p,
        }
    }

    #[inline]
    fn count(&self) -> u64 {
        static COUNTS: Array<Small, u16> = counts();
        COUNTS[*self] as u64
    }
}

const fn counts() -> Array<Small, u16> {
    let mut counts = [0u16; CLASS_COUNT + 1];

    // Special case: zero size class to defer branch
    counts[0] = 0;

    // Special case: the smallest size class has some
    // bits in its bitset reserved for slab metadata.
    counts[1] = (SIZE_BIT_SET * 64) as u16;

    let mut i = 2;
    while i <= 128 {
        counts[i] = (SIZE_SLAB / (i * 8)) as u16;
        i += 1;
    }

    while i < counts.len() {
        counts[i] = (SIZE_SLAB / (1024 << (i - 128))) as u16;
        i += 1;
    }

    Array {
        inner: counts,
        _bracket: PhantomData,
    }
}
