use core::array;
use core::fmt::Debug;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

use crate::bitset::Bit;

/// Fixed-size bitset implementation.
///
/// `SIZE` is in units of 8 bytes.
#[repr(C, align(8))]
pub(crate) struct AtomicBitSet<const SIZE: usize>([AtomicU64; SIZE]);

impl<const SIZE: usize> Default for AtomicBitSet<SIZE> {
    fn default() -> Self {
        Self(array::from_fn(|_| AtomicU64::new(0)))
    }
}

impl<const SIZE: usize> AtomicBitSet<SIZE> {
    pub(crate) fn clear(&self) {
        self.0
            .iter()
            .for_each(|row| row.store(0, Ordering::Release));
    }

    pub(crate) fn set(&self, bit: Bit) {
        let row = bit.row();
        let col = bit.col();
        self.0[row].fetch_or(1 << col, Ordering::SeqCst);
        crate::flush(&self.0[row], false);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.iter().all(|row| row.load(Ordering::Acquire) == 0)
    }

    pub(crate) fn is_full(&self, count: usize) -> bool {
        let rows = count / 64;

        // Full rows of 1s
        self.0
            .iter()
            .take(rows)
            .all(|row| row.load(Ordering::Acquire) == u64::MAX)
        &&
        // Partial row of 1s
        match count % 64 {
            0 => true,
            remainder => {
                self.0[rows].load(Ordering::Acquire).count_ones() as usize == remainder
            }
        }
    }
}

impl<const SIZE: usize> Debug for AtomicBitSet<SIZE> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        super::debug(f, self.0.iter().map(|row| row.load(Ordering::Acquire)))
    }
}
