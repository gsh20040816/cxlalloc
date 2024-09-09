use crate::bitset::Bit;

#[repr(C, align(8))]
pub(crate) struct HiBitSet<const SIZE: usize> {
    sparse: u64,
    dense: [u64; SIZE],
}

impl<const SIZE: usize> HiBitSet<SIZE> {
    pub(crate) fn fill(&mut self, count: usize) {
        let rows = count / 64;

        // Full rows of 1s
        self.dense
            .iter_mut()
            .take(rows)
            .for_each(|row| *row = u64::MAX);

        self.sparse = (1 << rows) - 1;

        // Partial row of 1s
        let skip = match count % 64 {
            0 => 0,
            remainder => {
                self.dense[rows] = (1 << remainder) - 1;
                self.sparse |= 1 << rows;
                1
            }
        };

        // Full rows of 0s
        self.dense
            .iter_mut()
            .skip(rows)
            .skip(skip)
            .for_each(|row| *row = 0);
    }

    pub(crate) fn peek(&self) -> Bit {
        let row = self.sparse.trailing_zeros() as usize;
        let col = self.dense[row].trailing_zeros() as usize;
        Bit::from_row_col((row, col))
    }

    pub(crate) fn set(&mut self, bit: Bit) {
        let row = bit.row();
        let col = bit.col();
        self.dense[row] |= 1 << col;
        self.sparse |= 1 << row;
    }

    pub(crate) fn unset(&mut self, bit: Bit) {
        let row = bit.row();
        let col = bit.col();

        self.dense[row] &= !(1 << col);
        self.sparse &= !(((self.dense[row] == 0) as u64) << row);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sparse == 0
    }

    pub(crate) fn is_full(&self, count: usize) -> bool {
        let rows = count / 64;

        // Note: +1 to account for when 64 * SIZE is not
        // evenly divisible by count.
        if self.sparse.count_ones() as usize + 1 < rows {
            return false;
        }

        self.dense
            .iter()
            .copied()
            .take(rows + 1)
            .map(u64::count_ones)
            .sum::<u32>() as usize
            == count
    }

    pub(crate) fn len(&self) -> usize {
        self.dense.iter().copied().map(u64::count_ones).sum::<u32>() as usize
    }
}
