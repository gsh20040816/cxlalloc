use core::fmt::Debug;

use ribbit::private::u6;

use crate::coherence::flush;
use crate::coherence::Invalidate;

#[repr(C, align(8))]
pub(crate) struct BitSet<const SIZE: usize> {
    count: u64,
    sparse: u64,
    dense: [u64; SIZE],
}

impl<const SIZE: usize> BitSet<SIZE> {
    const INVARIANT: () = assert!(SIZE <= 64);

    pub(crate) fn fill(&mut self, count: u64) {
        let rows = count as usize / 64;

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

        self.count = count;
        flush(self, Invalidate::No);
        self.validate();
    }

    pub(crate) fn peek(&self) -> Bit {
        let row = self.sparse.trailing_zeros() as u8;
        let col = unsafe { self.dense.get_unchecked(row as usize) }.trailing_zeros() as u8;
        Bit::new(u6::new(row), u6::new(col))
    }

    pub(crate) fn set(&mut self, bit: Bit) {
        let row = bit.row().value() as usize;
        let col = bit.col().value() as usize;
        let cols = unsafe { self.dense.get_unchecked_mut(row) };

        if cfg!(feature = "validate") {
            assert!(*cols & (1 << col) == 0, "Double free");
        }

        *cols |= 1 << col;
        flush(cols, Invalidate::No);
        self.count += 1;
        self.sparse |= 1 << row;
        flush(&self.count, Invalidate::No);
        self.validate();
    }

    pub(crate) fn unset(&mut self, bit: Bit) {
        let row = bit.row().value() as usize;
        let col = bit.col().value() as usize;
        let cols = unsafe { self.dense.get_unchecked_mut(row) };

        if cfg!(feature = "validate") {
            assert!(*cols & (1 << col) > 0, "Double allocate");
        }

        *cols &= !(1 << col);
        flush(cols, Invalidate::No);
        self.count -= 1;
        self.sparse &= !((*cols == 0) as u64) << row;
        flush(&self.count, Invalidate::No);
        self.validate();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub(crate) fn len(&self) -> u64 {
        self.count
    }

    #[track_caller]
    fn validate(&self) {
        const { Self::INVARIANT }

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
