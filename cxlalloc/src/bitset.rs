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
        (bit.row << 6) | bit.col
    }
}

fn debug<I>(f: &mut core::fmt::Formatter, iter: I) -> core::fmt::Result
where
    I: IntoIterator<Item = u64>,
{
    write!(f, "[")?;

    for (i, row) in iter.into_iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }

        if row == 0 {
            write!(f, "_")?;
            continue;
        } else {
            write!(f, "{}:", i)?;
        }

        for byte in 0..8 {
            match ((row >> (byte * 8)) as u8).reverse_bits() {
                0 => write!(f, "0-")?,
                0xFF => write!(f, "1-")?,
                byte => write!(f, "{:08b}", byte)?,
            }
        }
    }

    write!(f, "]")
}
