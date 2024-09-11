mod atomic;
mod hi;

pub(crate) use atomic::AtomicBitSet;
pub(crate) use hi::HiBitSet;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Bit {
    row: usize,
    col: usize,
}

impl Bit {
    // TODO: only used for ownership bitmap, maybe decouple?
    pub(crate) fn new(bit: usize) -> Self {
        Self {
            row: bit >> 6,
            col: bit & ((1 << 6) - 1),
        }
    }

    fn from_row_col(row: usize, col: usize) -> Self {
        Self { row, col }
    }

    fn row(self) -> usize {
        self.row
    }

    fn col(self) -> usize {
        self.col
    }
}

impl From<Bit> for usize {
    fn from(bit: Bit) -> Self {
        bit.row << 6 | bit.col
    }
}
