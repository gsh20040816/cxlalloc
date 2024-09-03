use core::sync::atomic::Ordering;
use std::sync::atomic::AtomicU64;

use crate::size;

/// Fixed-size bitset implementation.
///
/// `SIZE` is in units of 8 bytes.
#[repr(C, align(8))]
pub(crate) struct Set<const SIZE: usize>([AtomicU64; SIZE]);

impl<const SIZE: usize> Default for Set<SIZE> {
    fn default() -> Self {
        Self(std::array::from_fn(|_| AtomicU64::new(0)))
    }
}

impl<const SIZE: usize> Set<SIZE> {
    pub(crate) fn peek(&self) -> Option<Index> {
        self.0
            .iter()
            .map(|row| row.load(Ordering::Acquire))
            .enumerate()
            .find(|(_, row)| *row != 0)
            .map(|(i, row)| (i, row.trailing_zeros() as usize))
            .map(Index::from_row_col)
    }

    pub(crate) fn set(&self, index: Index) {
        let (row, col) = index.into_row_col();
        let old = self.0[row].load(Ordering::Acquire);
        let new = old | (1 << col);
        self.0[row].store(new, Ordering::Release);
    }

    pub(crate) fn clear(&self, index: Index) {
        let (row, col) = index.into_row_col();
        let old = self.0[row].load(Ordering::Acquire);
        let new = old & !(1 << col);
        self.0[row].store(new, Ordering::Release);
    }

    pub(crate) fn get(&self, index: Index) -> bool {
        let (row, col) = index.into_row_col();
        self.0[row].load(Ordering::Acquire) & (1 << col) > 0
    }

    pub(crate) fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    pub(crate) fn fill(&self, count: usize) {
        let rows = count / 64;
        self.0
            .iter()
            .take(rows)
            .for_each(|row| row.store(u64::MAX, Ordering::Release));

        match count % 64 {
            0 => (),
            remainder => self.0[rows].store((1 << remainder) - 1, Ordering::Release),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.0
            .iter()
            .map(|row| row.load(Ordering::Acquire))
            .map(|chunk| chunk.count_ones())
            .sum::<u32>() as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Index(usize);

impl Index {
    // TODO: only used for ownership bitmap, maybe decouple?
    pub(crate) fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) fn to_offset(self, class: size::Small) -> usize {
        self.0 * class.size()
    }

    fn from_row_col((i, j): (usize, usize)) -> Self {
        Self((i << 6) + j)
    }

    fn into_row_col(self) -> (usize, usize) {
        (self.0 >> 6, self.0 & ((1 << 6) - 1))
    }
}

#[test]
fn clear_next() {
    let set: Set<8> = Set::default();
    set.fill(512);

    for index in (0..512).map(Index) {
        assert_eq!(set.peek(), Some(index));
        set.clear(index);
    }
}

#[test]
fn fill() {
    for i in 0..1024 {
        let set: Set<16> = Set::default();
        assert_eq!(set.len(), 0);
        set.fill(i);
        assert_eq!(set.len(), i);
    }
}
