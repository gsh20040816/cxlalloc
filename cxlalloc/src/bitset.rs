mod atomic;
mod hi;

pub(crate) use atomic::AtomicBitSet;
pub(crate) use hi::HiBitSet;

#[ribbit::pack(size = 12, debug, eq, ord)]
pub(crate) struct Bit {
    #[ribbit(offset = 6)]
    row: u6,
    col: u6,
}

impl Bit {
    pub(crate) unsafe fn from_packed(packed: u16) -> Self {
        ribbit::private::unpack(packed)
    }
}

impl From<Bit> for u64 {
    fn from(bit: Bit) -> Self {
        ribbit::private::pack(bit) as u64
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
