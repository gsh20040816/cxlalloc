use std::sync::atomic::AtomicU64;

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
    pub(crate) fn peek(&mut self) -> Option<Index> {
        self.0
            .iter_mut()
            .map(AtomicU64::get_mut)
            .enumerate()
            .filter(|(_, row)| **row != 0)
            .map(|(i, row)| (i, row.trailing_zeros() as usize))
            .map(Index::from_row_column)
            .next()
    }

    pub(crate) fn set(&mut self, index: Index) {
        let (row, column) = index.into_row_column();
        *self.0[row].get_mut() |= 1 << column;
    }

    pub(crate) fn clear(&mut self, index: Index) {
        let (row, column) = index.into_row_column();
        *self.0[row].get_mut() &= !(1 << column);
    }

    pub(crate) fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    pub(crate) fn fill(&mut self, value: bool) {
        let value = -(value as i64) as u64;
        self.0
            .iter_mut()
            .map(AtomicU64::get_mut)
            .for_each(|row| *row = value);
    }

    pub(crate) fn len(&mut self) -> usize {
        self.0
            .iter_mut()
            .map(AtomicU64::get_mut)
            .map(|chunk| chunk.count_ones())
            .sum::<u32>() as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Index(usize);

impl Index {
    fn from_row_column((i, j): (usize, usize)) -> Self {
        Self((i << 6) + j)
    }

    fn into_row_column(self) -> (usize, usize) {
        (self.0 >> 6, self.0 & ((1 << 6) - 1))
    }
}

#[test]
fn clear_next() {
    let mut set: Set<8> = Set::default();
    set.fill(true);

    for index in (0..64).map(Index) {
        assert_eq!(set.peek(), Some(index));
        set.clear(index);
    }
}
