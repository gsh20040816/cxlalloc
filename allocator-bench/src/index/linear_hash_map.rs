use core::hash::Hash;
use core::hash::Hasher as _;
use core::hint;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::ffi::CString;
use std::io;

use rapidhash::RapidHasher;

use crate::Allocator;
use crate::Index;
use crate::allocator::Handle as _;

// Open-addressed, linear probing hashmap.
pub struct LinearHashMap {
    len: usize,
    raw: shm::Raw,
}

impl<A: Allocator> Index<A> for LinearHashMap {
    fn new(numa: Option<usize>, name: &str, len: usize, populate: bool) -> io::Result<Self> {
        Ok(Self {
            len,
            raw: shm::Raw::new(numa, CString::new(name).unwrap(), len * 8, populate)?,
        })
    }

    fn insert<F: FnOnce(&mut A, *mut u8)>(
        &self,
        allocator: &mut A,
        key: u64,
        size: usize,
        with: F,
    ) {
        let view = self.view();
        let index = self.index(&key);
        let mut probe = 0;

        let handle = allocator.allocate(8 + size).unwrap();

        unsafe {
            handle.as_ptr().cast::<u64>().write(key);
            with(allocator, handle.as_ptr().byte_add(8).cast::<u8>())
        }

        loop {
            while view[(index + probe) % view.len()].load(Ordering::Acquire) > 0 {
                probe += 1;
            }

            let slot = &view[(index + probe) % view.len()];

            if slot
                .compare_exchange(0, u64::MAX, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                unsafe {
                    allocator.link(slot.as_ptr(), &handle);
                }
                return;
            }
        }
    }

    fn get<F: FnOnce(&mut A, *const u8)>(&self, allocator: &mut A, key: u64, with: F) -> bool {
        let view = self.view();
        let index = self.index(&key);
        let mut probe = 0;

        loop {
            match view[(index + probe) % view.len()].load(Ordering::Acquire) {
                0 => return false,
                // Wait for link operation to complete
                u64::MAX => hint::spin_loop(),
                offset => {
                    let handle = allocator.offset_to_handle(offset).unwrap();
                    let pointer_key = handle.as_ptr().cast::<u64>();

                    match key == unsafe { pointer_key.read() } {
                        false => probe += 1,
                        true => {
                            let pointer_value = unsafe { handle.as_ptr().byte_add(8).cast::<u8>() };
                            with(allocator, pointer_value);
                            return true;
                        }
                    }
                }
            }
        }
    }

    fn unlink(&mut self) -> io::Result<()> {
        self.raw.unlink()
    }
}

impl LinearHashMap {
    fn index<K: Hash>(&self, key: &K) -> usize {
        let mut hasher = RapidHasher::default();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.len
    }

    fn view(&self) -> &[AtomicU64] {
        unsafe { std::slice::from_raw_parts(self.raw.address().cast::<AtomicU64>(), self.len) }
    }
}
