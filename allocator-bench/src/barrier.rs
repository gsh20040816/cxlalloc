use core::ffi::CStr;
use core::hint;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::io;

use shm::Shm;

pub struct Barrier {
    total: u64,
    shm: Shm<u64>,
}

unsafe impl Sync for Barrier {}

impl Barrier {
    const PATH: &CStr = c"/barrier";

    pub fn new(create: bool, total: u64) -> io::Result<Self> {
        Shm::builder()
            .create(create)
            .name(Self::PATH.to_owned())
            .populate(true)
            .build()
            .map(|shm| Self { total, shm })
    }

    pub fn unlink(&mut self) -> io::Result<()> {
        self.shm.unlink()
    }

    // https://nullprogram.com/blog/2022/03/13/
    pub fn wait(&self, add: u64) {
        let barrier = self.get();
        let value = barrier.fetch_add(add, Ordering::Relaxed);

        if (value + add) % self.total == 0 {
            return;
        }

        let epoch = value / self.total;
        while barrier.load(Ordering::Relaxed) / self.total == epoch {
            hint::spin_loop()
        }
    }

    fn get(&self) -> &AtomicU64 {
        unsafe { AtomicU64::from_ptr(self.shm.address_mut()) }
    }
}
