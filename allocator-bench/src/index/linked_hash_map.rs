use core::hash::Hash;
use core::hash::Hasher as _;
use core::hint;
use core::mem::MaybeUninit;
use core::slice;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::ffi::CString;
use std::io;

use rapidhash::RapidHasher;

use crate::Allocator;
use crate::Index;
use crate::allocator::Handle as _;

use super::Key;

/// Separate chaining hashmap
///
/// Inserted nodes are one contiguous allocation with the
/// next pointer, key, and value.
pub struct LinkedHashMap {
    len: usize,
    raw: shm::Raw,
}

impl<A: Allocator, K: Key> Index<A, K> for LinkedHashMap {
    fn new(numa: Option<usize>, name: &str, len: usize, populate: bool) -> io::Result<Self> {
        Ok(Self {
            len,
            raw: shm::Raw::new(numa, CString::new(name).unwrap(), len * 8, populate)?,
        })
    }

    fn insert<F: FnOnce(&mut A, *mut u8)>(&self, allocator: &mut A, key: K, size: usize, with: F) {
        let view = self.view();
        let index = self.index(&key);

        let len = key.len();
        let handle = allocator.allocate(8 + len + size).unwrap();

        let pointer_next = handle.as_ptr().cast::<u64>();
        let pointer_key = unsafe {
            slice::from_raw_parts_mut(handle.as_ptr().byte_add(8).cast::<MaybeUninit<u8>>(), len)
        };
        let pointer_value = unsafe { handle.as_ptr().byte_add(8 + len).cast::<u8>() };

        unsafe {
            key.copy(pointer_key);
            with(allocator, pointer_value);
        }

        let head = &view[index];
        loop {
            let next = head.load(Ordering::Acquire);

            unsafe {
                pointer_next.write(next);
            }

            match head.compare_exchange(next, u64::MAX, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => unsafe {
                    allocator.link(head.as_ptr(), &handle);
                    return;
                },
                Err(_) => continue,
            }
        }
    }

    fn get<F: FnOnce(&mut A, *const u8)>(&self, allocator: &mut A, key: K, with: F) -> bool {
        let view = self.view();
        let index = self.index(&key);

        let mut head = loop {
            match view[index].load(Ordering::Acquire) {
                0 => return false,
                u64::MAX => {
                    hint::spin_loop();
                    continue;
                }
                offset => break offset,
            }
        };

        loop {
            let handle = match head {
                0 => return false,
                offset => allocator.offset_to_handle(offset).unwrap(),
            };

            let pointer_next = handle.as_ptr().cast::<u64>();
            let candidate = unsafe { K::from_ptr(handle.as_ptr().byte_add(8).cast::<u8>()) };
            let len = candidate.len();
            let pointer_value = unsafe { handle.as_ptr().byte_add(8 + len).cast::<u8>() };

            match key == candidate {
                true => {
                    with(allocator, pointer_value);
                    return true;
                }
                false => {
                    head = unsafe { pointer_next.read() };
                }
            }
        }
    }

    fn unlink(&mut self) -> io::Result<()> {
        self.raw.unlink()
    }
}

impl LinkedHashMap {
    fn index<K: Hash>(&self, key: &K) -> usize {
        let mut hasher = RapidHasher::default();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.len
    }

    fn view(&self) -> &[AtomicU64] {
        unsafe { std::slice::from_raw_parts(self.raw.address().cast::<AtomicU64>(), self.len) }
    }
}
