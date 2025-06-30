mod huge;
mod large;
mod small;

pub(crate) use huge::Huge;
pub(crate) use large::Large;
pub(crate) use small::Small;

use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops;

use crate::bitset;

pub(crate) trait Bracket: ribbit::Pack + Debug + 'static {
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

#[cfg(test)]
mod test {

    use super::Bracket;
    use super::Large;
    use super::Small;

    #[test]
    fn small_consistent() {
        // Skip special size classes
        for i in 2..Small::COUNT {
            let class = Small::from_index(i as u8);

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
            let class = Large::from_index(i as u8);
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
