use core::array;
use core::fmt;
use core::fmt::Display;
use core::ops;

use crate::SIZE_BIT_SET;
use crate::SIZE_SLAB;

pub(crate) const MIN: usize = 8;

pub(crate) trait Bracket {
    const SIZE_SLAB: usize;
}

impl Bracket for Class {
    // TODO: get rid of crate::SIZE_SLAB
    const SIZE_SLAB: usize = crate::SIZE_SLAB;
}

#[repr(transparent)]
#[derive(Debug)]
pub(crate) struct Array<T>([T; 1 + CLASS_COUNT]);

impl<T> Array<T> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Class, &T)> {
        self.0
            .iter()
            .enumerate()
            .map(|(index, element)| (Class::new_internal(index as u8), element))
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

    fn index(&self, class: Class) -> &Self::Output {
        unsafe { self.0.get_unchecked(class._0() as usize) }
    }
}

impl<T> ops::IndexMut<Class> for Array<T> {
    fn index_mut(&mut self, class: Class) -> &mut Self::Output {
        unsafe { self.0.get_unchecked_mut(class._0() as usize) }
    }
}

#[ribbit::pack(size = 8, debug, new(rename = "new_internal", vis = ""))]
#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct Class(u8);

impl Display for Class {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.size())
    }
}

// 8..1024 4k..32k
const CLASS_COUNT: usize = 128 + 5;

pub(crate) const SLAB: Class = match Class::new(SIZE_SLAB) {
    None => unreachable!(),
    Some(class) => class,
};

impl Class {
    #[inline]
    pub(crate) const fn new(size: usize) -> Option<Self> {
        match size {
            0..1024 => Some(Class::new_internal(((size + 7) / 8) as u8)),
            1024..=32768 => Some(Class::new_internal(
                128 + size.next_power_of_two().trailing_zeros() as u8 - 10,
            )),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn is_zero(&self) -> bool {
        self._0() == 0
    }

    #[inline]
    pub(crate) fn size(&self) -> usize {
        match self._0().checked_sub(128) {
            None => self._0() as usize * 8,
            Some(p) => 1024 << p,
        }
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
    while i <= 128 {
        counts[i] = (SIZE_SLAB / (i * 8)) as u16;
        i += 1;
    }

    while i < counts.len() {
        counts[i] = (SIZE_SLAB / (1024 << (i - 128))) as u16;
        i += 1;
    }

    Array(counts)
}
