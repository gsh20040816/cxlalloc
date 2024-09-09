use core::sync::atomic::AtomicU64;

pub struct HiBitSet<const SIZE: usize> {
    sparse: u64,
    dense: [u64; SIZE],
}

impl<const SIZE: usize> HiBitSet<SIZE> {}
