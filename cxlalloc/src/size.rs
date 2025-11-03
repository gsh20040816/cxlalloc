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

/// A set of size classes that share the same slab size and
/// in-memory representation.
pub(crate) trait Bracket: ribbit::Pack + Debug + Eq + 'static {
    /// Name of this size bracket (for logging and statistics).
    const NAME: &'static str;

    /// Size of the associated slab for this size bracket in bytes.
    const SIZE_SLAB: usize;

    /// Smallest size class in this bracket in bytes.
    const SIZE_MIN: usize;

    /// Largest size class in this bracket in bytes.
    const SIZE_MAX: usize;

    /// Number of size classes in this bracket.
    const COUNT: usize;

    /// Workaround for not being able to use associated const as
    /// a const generic (for `[T; B::COUNT]` in `struct Array` below).
    type Array<T>: AsRef<[T]> + AsMut<[T]>;

    /// Workaround for not being able to use associated const as
    /// a const generic (for `BitSet<{ SIZE_SLAB / SIZE_MIN }`).
    type BitSet: bitset::Interface;

    /// Bin `size` into the nearest size class in this bracket.
    fn new(size: usize) -> Option<Self>;

    /// Check if this size class corresponds to zero bytes.
    fn is_zero(&self) -> bool;

    /// Size of this size class in bytes.
    fn size(&self) -> u64;

    /// Number of blocks of this size class per slab.
    fn count(&self) -> u64;

    /// Private: construct size class from index.
    fn from_index(index: usize) -> Option<Self>;

    /// Private: create default-initialized array.
    fn array<T: Default>() -> Self::Array<T>;
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
        let index = ribbit::convert::loose_to_loose::<_, u64>(ribbit::convert::packed_to_loose(
            class.pack(),
        )) as usize;
        unsafe { self.inner.as_ref().get_unchecked(index) }
    }
}

impl<B: Bracket, T> ops::IndexMut<B> for Array<B, T> {
    fn index_mut(&mut self, class: B) -> &mut Self::Output {
        let index = ribbit::convert::loose_to_loose::<_, u64>(ribbit::convert::packed_to_loose(
            class.pack(),
        )) as usize;
        unsafe { self.inner.as_mut().get_unchecked_mut(index) }
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
