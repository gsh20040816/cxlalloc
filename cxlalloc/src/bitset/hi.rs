use crate::bitset::Bit;

#[repr(C, align(8))]
pub(crate) struct HiBitSet<const SIZE: usize> {
    full: u64,
    partial: u64,
    blocks: [u64; SIZE],
}

impl<const SIZE: usize> HiBitSet<SIZE> {
    pub(crate) fn fill(&mut self, count: usize) {
        let rows = count / 64;

        // Full rows of 1s
        self.blocks
            .iter_mut()
            .take(rows)
            .for_each(|row| *row = u64::MAX);

        self.full = (1 << rows) - 1;
        self.partial = self.full;

        // Partial row of 1s
        let skip = match count % 64 {
            0 => 0,
            remainder => {
                self.blocks[rows] = (1 << remainder) - 1;
                self.partial |= 1 << rows;
                1
            }
        };

        // Full rows of 0s
        self.blocks
            .iter_mut()
            .skip(rows)
            .skip(skip)
            .for_each(|row| *row = 0);
    }

    pub(crate) fn peek(&self) -> Bit {
        let row = self.partial.trailing_zeros() as usize;
        let col = self.blocks[row].trailing_zeros() as usize;
        Bit::from_row_col((row, col))
    }

    pub(crate) fn set(&mut self, bit: Bit) {
        let row = bit.row();
        let col = bit.col();
        self.blocks[row] |= 1 << col;
        self.full |= ((self.blocks[row] == u64::MAX) as u64) << row;
        self.partial |= 1 << row;
    }

    pub(crate) fn unset(&mut self, bit: Bit) {
        let row = bit.row();
        let col = bit.col();

        self.blocks[row] &= !(1 << col);
        self.full &= !(1 << row);
        self.partial &= !((self.blocks[row] == 0) as u64) << row;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.partial == 0
    }

    pub(crate) fn is_full(&self, count: usize) -> bool {
        let full = self.full.count_ones() as usize;
        full + match count % 64 {
            0 => 0,
            _ => self.blocks[count / 64].count_ones() as usize,
        } == count
    }

    pub(crate) fn len(&self) -> usize {
        self.blocks
            .iter()
            .copied()
            .map(u64::count_ones)
            .sum::<u32>() as usize
    }
}
