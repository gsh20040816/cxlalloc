use core::fmt::Debug;
use core::mem;

use ribbit::private::u6;

use crate::cache;

pub(crate) const SIZE_METADATA: usize = mem::size_of::<u64>() * 2;

pub(crate) trait Interface: Copy + Debug + Sized {
    const SIZE: usize = Self::SIZE_DATA + SIZE_METADATA;
    const SIZE_DATA: usize;

    fn fill(&mut self, count: u64);

    // Return the first set bit.
    //
    // # Safety
    //
    // Caller must guarantee that this bitset is non-empty.
    unsafe fn peek_unchecked(&self) -> Bit;

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

        validate_eq!(
            self.dense
                .iter()
                .copied()
                .map(u64::count_ones)
                .map(u64::from)
                .sum::<u64>(),
            self.count,
            "Count is consistent with dense bitset"
        );

        for bit in 0..SIZE {
            validate_eq!(
                (self.sparse & (1 << bit)) > 0,
                self.dense[bit] > 0,
                "Sparse bitset is consistent with dense bitset",
            );
        }

        for bit in SIZE..64 {
            validate_eq!(
                self.sparse & (1 << bit),
                0,
                "Sparse bitset does not overflow",
            );
        }
    }
}

impl<const SIZE: usize> BitSet<SIZE> {
    #[cfg(test)]
    pub(crate) const fn new() -> Self {
        Self {
            count: 0,
            sparse: 0,
            dense: [0; SIZE],
        }
    }

    #[cfg(test)]
    pub(crate) fn filled(count: u64) -> Self {
        let mut filled = Self::new();
        filled.fill(count);
        filled
    }
}

impl<const SIZE: usize> Interface for BitSet<SIZE> {
    const SIZE_DATA: usize = mem::size_of::<u64>() * SIZE;

    fn fill(&mut self, count: u64) {
        let rows = count / 64;
        let cols = count % 64;

        let mut i = 0;
        while i < rows as usize {
            self.dense[i] = u64::MAX;
            i += 1;
        }

        self.sparse = match 1u64.checked_shl(rows as u32) {
            Some(bit) => bit - 1,
            None => u64::MAX,
        };

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

    #[inline]
    unsafe fn peek_unchecked(&self) -> Bit {
        let row = self.sparse.trailing_zeros() as u8;
        let col = unsafe { self.dense.get_unchecked(row as usize) }.trailing_zeros() as u8;
        Bit {
            row: u6::new(row),
            col: u6::new(col),
        }
    }

    #[inline]
    fn set(&mut self, bit: Bit) {
        let row = bit.row.value() as usize;
        let col = bit.col.value() as usize;
        let cols = unsafe { self.dense.get_unchecked_mut(row) };

        validate_eq!(*cols & (1 << col), 0, "Double free");

        *cols |= 1 << col;
        cache::flush(cols, cache::Invalidate::No);

        self.count += 1;
        self.sparse |= 1 << row;
        self.validate();
    }

    #[inline]
    fn unset(&mut self, bit: Bit) {
        let row = bit.row.value() as usize;
        let col = bit.col.value() as usize;
        let cols = unsafe { self.dense.get_unchecked_mut(row) };

        validate!(*cols & (1 << col) > 0, "Double allocate");

        *cols &= !(1 << col);
        cache::flush(cols, cache::Invalidate::No);

        self.count -= 1;
        self.sparse &= !((*cols == 0) as u64) << row;
        self.validate();
    }

    #[inline]
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
                write!(f, "{i}:")?;
            }

            for byte in 0..8 {
                match ((row >> (byte * 8)) as u8).reverse_bits() {
                    0 => write!(f, "0-")?,
                    0xFF => write!(f, "1-")?,
                    byte => write!(f, "{byte:08b}")?,
                }
            }
        }

        write!(f, "]")?;
        write!(f, " }}")
    }
}

#[derive(ribbit::Pack, Copy, Clone, Debug, PartialEq, Eq)]
#[ribbit(size = 12)]
pub(crate) struct Bit {
    col: u6,
    row: u6,
}

impl Bit {
    pub(crate) unsafe fn from_loose(bit: u16) -> Self {
        Self {
            row: u6::new((bit >> 6) as u8),
            col: u6::new((bit & 0b111111) as u8),
        }
    }
}

impl From<Bit> for u64 {
    fn from(bit: Bit) -> u64 {
        (bit.col.value() as u64) | ((bit.row.value() as u64) << 6)
    }
}

// A bitset filled to `len` has length `len`.
#[test]
fn fill_len() {
    let mut set = BitSet::<64>::new();
    for len in 0..=64 * 64 {
        set.fill(0);
        assert_eq!(set.len(), 0);
        set.fill(len);
        assert_eq!(set.len(), len);
    }
}

// Peeking and unsetting one bit at a time decreases `len` one at a time.
#[test]
fn peek_unset_len() {
    let mut set = BitSet::<64>::filled(64 * 64);
    for (bit, len) in (0..64 * 64).zip((0..64 * 64).rev()) {
        let bit = unsafe { Bit::from_loose(bit as u16) };
        assert_eq!(unsafe { set.peek_unchecked() }, bit);
        set.unset(bit);
        assert_eq!(set.len(), len);
    }
}

// Setting a bit one at a time increases `len` and decreases `peek`.
#[test]
fn set_peek_len() {
    let mut set = BitSet::<64>::filled(0);
    for (bit, len) in (0..64 * 64).rev().zip(0..64 * 64) {
        let bit = unsafe { Bit::from_loose(bit as u16) };
        assert_eq!(set.len(), len);
        set.set(bit);
        assert_eq!(unsafe { set.peek_unchecked() }, bit);
    }
}
