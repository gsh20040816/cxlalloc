mod atomic;
mod hi;

pub(crate) use atomic::AtomicBitSet;
pub(crate) use hi::HiBitSet;

#[ribbit::pack(size = 12, debug)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Bit(u12);

impl Bit {
    fn from_row_col(row: u8, col: u8) -> Self {
        Self::new(::ribbit::private::u12::new(
            ((row as u16) << 6) | (col as u16),
        ))
    }

    fn row(self) -> usize {
        (self._0().value() >> 6) as usize
    }

    fn col(self) -> usize {
        (self._0().value() & ((1 << 6) - 1)) as usize
    }
}

impl From<Bit> for u64 {
    fn from(bit: Bit) -> Self {
        bit._0().value() as u64
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
