use core::hint;

use crate::atomic::Packed;
use crate::atomic::Version;
use crate::atomic::Versioned;
use crate::Atomic;

#[repr(C, align(64))]
pub(crate) struct Barrier(Atomic<Versioned<Set>>);

impl Barrier {
    pub fn version(&self) -> Version {
        self.0.load().version()
    }

    pub fn request(&self, count: usize, version: Version) {
        let old = version;
        let new = version.next();

        let _ = self.0.compare_exchange(
            Versioned::new(Set::new(0), old),
            Versioned::new(Set::new((1 << count) - 1), new),
        );

        loop {
            let set = self.0.load();
            let now = set.version();

            // Either version has changed, so the current
            // request has been replaced, or the version is
            // the same and everyone has acknowledged.
            if now != new || set.inner().value() == 0 {
                return;
            }

            hint::spin_loop();
        }
    }

    pub fn has_request(&self, id: usize) -> bool {
        self.0.load().inner().value() & (1 << (id as u64)) > 0
    }

    pub fn acknowledge(&self, id: usize) {
        self.0.fetch_xor(1 << id);
    }
}

#[derive(Debug)]
struct Set(u64);

impl Set {
    fn new(value: u64) -> Self {
        debug_assert!(value < (1 << Self::BITS));
        Self(value)
    }

    fn value(&self) -> u64 {
        self.0 & Self::MASK
    }
}

unsafe impl Packed for Set {
    const BITS: u8 = 48;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value & Self::MASK)
    }
}
