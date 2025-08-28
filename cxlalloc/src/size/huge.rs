use core::fmt::Debug;

use crate::bitset::BitSet;
use crate::size;

#[ribbit::pack(size = 8, eq)]
pub struct Huge;

impl Debug for Huge {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Huge")
    }
}

impl size::Bracket for Huge {
    const NAME: &'static str = "huge";

    const SIZE_SLAB: usize = 1 << 30;
    const SIZE_MIN: usize = 4096;
    const SIZE_MAX: usize = 4096;
    const COUNT: usize = 1;

    type Array<T> = [T; 1];
    type BitSet = BitSet<0>;

    fn new(_: usize) -> Option<Self> {
        Some(Huge::new())
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Huge::new()),
            _ => None,
        }
    }

    fn array<T: Default>() -> Self::Array<T> {
        [T::default()]
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
