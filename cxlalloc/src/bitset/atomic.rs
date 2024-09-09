use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

use crate::bitset::Bit;

/// Fixed-size bitset implementation.
///
/// `SIZE` is in units of 8 bytes.
#[repr(C, align(8))]
#[derive(Debug)]
pub(crate) struct AtomicBitSet<const SIZE: usize>([AtomicU64; SIZE]);

impl<const SIZE: usize> Default for AtomicBitSet<SIZE> {
    fn default() -> Self {
        Self(std::array::from_fn(|_| AtomicU64::new(0)))
    }
}

impl<const SIZE: usize> AtomicBitSet<SIZE> {
    pub(crate) fn peek(&self) -> Option<Bit> {
        self.0
            .iter()
            .map(|row| row.load(Ordering::Acquire))
            .enumerate()
            .find(|(_, row)| *row != 0)
            .map(|(i, row)| (i, row.trailing_zeros() as usize))
            .map(Bit::from_row_col)
    }

    pub(crate) fn set(&self, bit: Bit) -> u32 {
        let row = bit.row();
        let col = bit.col();
        let old = self.0[row].load(Ordering::Acquire);
        let new = old | (1 << col);
        self.0[row].store(new, Ordering::Release);
        new.count_ones()
    }

    pub(crate) fn unset(&self, bit: Bit) -> u32 {
        let row = bit.row();
        let col = bit.col();
        let old = self.0[row].load(Ordering::Acquire);
        let new = old & !(1 << col);
        self.0[row].store(new, Ordering::Release);
        new.count_ones()
    }

    pub(crate) fn get(&self, bit: Bit) -> bool {
        self.0[bit.row()].load(Ordering::Acquire) & (1 << bit.col()) > 0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.iter().all(|row| row.load(Ordering::Acquire) == 0)
    }

    pub(crate) fn is_empty_except(&self, bit: Bit) -> bool {
        self.0
            .iter()
            .enumerate()
            .filter(|(row, _)| *row != bit.row())
            .all(|(_, row)| row.load(Ordering::Acquire) == 0)
    }

    pub(crate) fn reset(&self, count: usize) {
        let rows = count / 64;

        // Full rows of 1s
        self.0
            .iter()
            .take(rows)
            .for_each(|row| row.store(u64::MAX, Ordering::Release));

        // Partial row of 1s
        let skip = match count % 64 {
            0 => 0,
            remainder => {
                self.0[rows].store((1 << remainder) - 1, Ordering::Release);
                1
            }
        };

        // Full rows of 0s
        self.0
            .iter()
            .skip(rows)
            .skip(skip)
            .for_each(|row| row.store(0, Ordering::Release));
    }

    pub(crate) fn is_full(&self, count: usize) -> bool {
        let rows = count / 64;

        // Full rows of 1s
        self.0
            .iter()
            .take(rows)
            .all(|row| row.load(Ordering::Acquire) == u64::MAX)
        &&
        // Partial row of 1s
        match count % 64 {
            0 => true,
            remainder => {
                self.0[rows].load(Ordering::Acquire).count_ones() as usize == remainder
            }
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

#[cfg(test)]
mod tests {
    use super::AtomicBitSet;
    use super::Bit;

    #[test]
    fn peek_unset() {
        let set: AtomicBitSet<8> = AtomicBitSet::default();
        set.reset(512);

        for bit in (0..512).map(Bit) {
            assert_eq!(set.peek(), Some(bit));
            set.unset(bit);
        }
    }

    #[test]
    fn reset_len() {
        for i in 0..1024 {
            let set: AtomicBitSet<16> = AtomicBitSet::default();
            assert_eq!(set.len(), 0);
            set.reset(i);
            assert_eq!(set.len(), i);
        }
    }

    #[test]
    fn peek_set() {
        let set: AtomicBitSet<7> = AtomicBitSet::default();
        assert_eq!(set.peek(), None);

        for bit in (0..448).rev().map(Bit) {
            set.set(bit);
            assert_eq!(set.peek(), Some(bit));
        }
    }
}
