use core::hash::Hash;
use core::hash::Hasher as _;
use core::sync::atomic::AtomicU64;
use std::ffi::CString;
use std::io;

use rapidhash::RapidHasher;

use crate::Allocator;
use crate::Index;

// Open-addressed, linear probing hashmap.
pub struct LinearHashMap {
    len: usize,
    raw: shm::Raw,
}

impl<A: Allocator> Index<A> for LinearHashMap {
    fn new(
        numa: Option<usize>,
        name: &str,
        len: usize,
        populate: bool,
        _thread_total: usize,
    ) -> io::Result<Self> {
        Ok(Self {
            len,
            raw: shm::Raw::new(numa, CString::new(name).unwrap(), len * 8, populate)?,
        })
    }

    fn insert<F: FnOnce(*mut u8)>(
        &self,
        _thread_id: usize,
        _allocator: &mut A,
        _key: &[u8],
        _size: usize,
        _with: F,
    ) {
        todo!()
    }

    fn get<F: FnOnce(*const u8)>(
        &self,
        _thread_id: usize,
        _allocator: &mut A,
        _key: &[u8],
        _with: F,
    ) -> bool {
        todo!()
    }

    fn unlink(&mut self) -> io::Result<()> {
        self.raw.unlink()
    }
}

impl LinearHashMap {
    #[expect(dead_code)]
    fn index<K: Hash>(&self, key: &K) -> usize {
        let mut hasher = RapidHasher::default();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.len
    }

    #[expect(dead_code)]
    fn view(&self) -> &[AtomicU64] {
        unsafe { std::slice::from_raw_parts(self.raw.address().cast::<AtomicU64>(), self.len) }
    }
}
