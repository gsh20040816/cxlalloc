use core::fmt::Debug;
use core::mem;

use ribbit::private::u6;

use crate::cache;

pub(crate) const SIZE_METADATA: usize = mem::size_of::<u64>() * 2;

pub(crate) trait Interface: Copy + Debug + Sized {
    const SIZE: usize = Self::SIZE_DATA + SIZE_METADATA;
    const SIZE_DATA: usize;

    #[allow(unused)]
    fn fill(&mut self, count: u64);
    fn peek(&self) -> Bit;
    fn set(&mut self, bit: Bit);
    fn unset(&mut self, bit: Bit);
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[repr(C, align(8))]
#[derive(Copy, Clone)]
pub(crate) struct BitSet<const SIZE: usize> {
    count: u64,
    sparse: u64,
    dense: [u64; SIZE],
}

impl<const SIZE: usize> BitSet<SIZE> {
    #[track_caller]
    fn validate(&self) {
        const { assert!(SIZE <= 64) }

        if !cfg!(feature = "validate") {
            return;
        }

        let total = self.dense.iter().copied().map(u64::count_ones).sum::<u32>();
        assert_eq!(
            total as u64, self.count,
            "Count is consistent with dense bitset"
        );

        for bit in 0..SIZE {
            assert_eq!(
                (self.sparse & (1 << bit)) > 0,
                self.dense[bit] > 0,
                "Sparse bitset is consistent with dense bitset",
            );
        }

        for bit in SIZE..64 {
            assert_eq!(
                self.sparse & (1 << bit),
                0,
                "Sparse bitset does not overflow",
            );
        }
    }
}

impl<const SIZE: usize> BitSet<SIZE> {
    pub(crate) const fn new() -> Self {
        Self {
            count: 0,
            sparse: 0,
            dense: [0; SIZE],
        }
    }

    pub(crate) const fn filled(count: u64) -> Self {
        let mut filled = Self::new();
        filled.fill(count);
        filled
    }

    pub(crate) const fn fill(&mut self, count: u64) {
        let rows = count / 64;
        let cols = count % 64;

        let mut i = 0;
        while i < rows as usize {
            self.dense[i] = u64::MAX;
            i += 1;
        }

        self.sparse = (1u64 << rows) - 1;
        let skip = match cols {
            0 => 0,
            _ => {
                self.dense[rows as usize] = (1 << cols) - 1;
                self.sparse |= ((cols > 0) as u64) << rows;
                1
            }
        };

        let mut j = i + skip;
        while j < self.dense.len() {
            self.dense[j] = 0;
            j += 1;
        }

        self.count = count;
    }
}

impl<const SIZE: usize> Interface for BitSet<SIZE> {
    const SIZE_DATA: usize = mem::size_of::<u64>() * SIZE;

    fn fill(&mut self, count: u64) {
        self.fill(count)
    }

    fn peek(&self) -> Bit {
        let row = self.sparse.trailing_zeros() as u8;
        let col = unsafe { self.dense.get_unchecked(row as usize) }.trailing_zeros() as u8;
        Bit::new(u6::new(row), u6::new(col))
    }

    fn set(&mut self, bit: Bit) {
        let row = bit.row().value() as usize;
        let col = bit.col().value() as usize;
        let cols = unsafe { self.dense.get_unchecked_mut(row) };

        if cfg!(feature = "validate") {
            assert!(*cols & (1 << col) == 0, "Double free");
        }

        *cols |= 1 << col;
        cache::flush(cols, cache::Invalidate::No);

        self.count += 1;
        self.sparse |= 1 << row;
        self.validate();
    }

    fn unset(&mut self, bit: Bit) {
        let row = bit.row().value() as usize;
        let col = bit.col().value() as usize;
        let cols = unsafe { self.dense.get_unchecked_mut(row) };

        if cfg!(feature = "validate") {
            assert!(*cols & (1 << col) > 0, "Double allocate");
        }

        *cols &= !(1 << col);
        cache::flush(cols, cache::Invalidate::No);

        self.count -= 1;
        self.sparse &= !((*cols == 0) as u64) << row;
        self.validate();
    }

    fn len(&self) -> u64 {
        self.count
    }
}

impl<const SIZE: usize> Debug for BitSet<SIZE> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{{ count: {}, sparse: {:b}, dense: ",
            self.count, self.sparse
        )?;

        write!(f, "[")?;

        for (i, row) in self.dense.iter().copied().enumerate() {
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

        write!(f, "]")?;
        write!(f, " }}")
    }
}

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
