mod atomic;
mod hi;

pub(crate) use atomic::AtomicBitSet;
pub(crate) use hi::HiBitSet;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Bit(usize);

impl Bit {
    // TODO: only used for ownership bitmap, maybe decouple?
    pub(crate) fn new(bit: usize) -> Self {
        Self(bit)
    }

    fn from_row_col((row, col): (usize, usize)) -> Self {
        Self((row << 6) + col)
    }

    fn row(self) -> usize {
        self.0 >> 6
    }

    fn col(self) -> usize {
        self.0 & ((1 << 6) - 1)
    }
}

impl From<Bit> for usize {
    fn from(bit: Bit) -> Self {
        bit.0
    }
}
