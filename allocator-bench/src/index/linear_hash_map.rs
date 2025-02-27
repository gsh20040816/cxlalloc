use core::hash::Hash;
use core::hash::Hasher as _;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::ffi::CString;
use std::io;

use rapidhash::RapidHasher;

pub struct LinearHashMap {
    len: usize,
    raw: shm::Raw,
}

impl LinearHashMap {
    pub fn new(numa: Option<usize>, name: &str, size: usize, populate: bool) -> io::Result<Self> {
        let size = size.next_multiple_of(8);

        Ok(Self {
            len: size / 8,
            raw: shm::Raw::new(numa, CString::new(name).unwrap(), size, populate)?,
        })
    }

    pub fn insert<K: Hash>(&self, key: K, value: u64) {
        let view = self.view();
        let index = self.index(key);
        let mut probe = 0;

        loop {
            while view[(index + probe) % view.len()].load(Ordering::Acquire) > 0 {
                probe += 1;
            }

            if view[(index + probe) % view.len()]
                .compare_exchange(0, value + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }
    }

    pub fn get<K: Hash, F: FnMut(u64) -> Option<T>, T>(&self, key: K, mut compare: F) -> Option<T> {
        let view = self.view();
        let index = self.index(key);
        let mut probe = 0;

        loop {
            match view[(index + probe) % view.len()].load(Ordering::Acquire) {
                0 => return None,
                offset => match compare(offset - 1) {
                    value @ Some(_) => return value,
                    None => probe += 1,
                },
            }
        }
    }

    pub fn unlink(&mut self) -> io::Result<()> {
        self.raw.unlink()
    }

    fn index<K: Hash>(&self, key: K) -> usize {
        let mut hasher = RapidHasher::default();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.len
    }

    fn view(&self) -> &[AtomicU64] {
        unsafe { std::slice::from_raw_parts(self.raw.address().cast::<AtomicU64>(), self.len) }
    }
}
