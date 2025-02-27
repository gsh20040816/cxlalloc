use core::ffi::CStr;
use core::hint;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::io;

use shm::Shm;

pub struct Barrier(Shm<u64>);

unsafe impl Sync for Barrier {}

impl Barrier {
    const PATH: &CStr = c"/barrier";

    pub fn new() -> io::Result<Self> {
        Shm::new(None, Self::PATH.to_owned(), true).map(Self)
    }

    pub fn unlink(&mut self) -> io::Result<()> {
        self.0.unlink()
    }

    // https://nullprogram.com/blog/2022/03/13/
    pub fn wait(&self, total: u64, add: u64) {
        let barrier = self.get();
        let value = barrier.fetch_add(add, Ordering::Relaxed);

        if (value + add) % total == 0 {
            return;
        }

        let epoch = value / total;
        while barrier.load(Ordering::Relaxed) / total == epoch {
            hint::spin_loop()
        }
    }

    fn get(&self) -> &AtomicU64 {
        unsafe { AtomicU64::from_ptr(self.0.address_mut()) }
    }
}
