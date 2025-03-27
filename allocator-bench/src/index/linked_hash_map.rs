use core::hash::Hash;
use core::hash::Hasher as _;
use core::hint;
use core::num::NonZeroU64;
use core::slice;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::ffi::CString;
use std::io;

use rapidhash::RapidHasher;
use shm::Shm;

use crate::Allocator;
use crate::Index;
use crate::allocator::Handle as _;
use crate::ebr;

/// Separate chaining hashmap
///
/// Inserted nodes are one contiguous allocation with the
/// next pointer, key, and value.
pub struct LinkedHashMap {
    len: usize,
    ebr: Shm<ebr::Global>,
    raw: shm::Raw,
}

impl<A: Allocator> Index<A> for LinkedHashMap {
    fn new(
        numa: Option<usize>,
        name: &str,
        len: usize,
        populate: bool,
        thread_total: usize,
    ) -> io::Result<Self> {
        let ebr = shm::Shm::new(numa, c"/ebr".to_owned(), populate)?;

        unsafe {
            ebr::Global::init(ebr.address_mut(), thread_total);
        }

        Ok(Self {
            len,
            ebr,
            raw: shm::Raw::new(numa, CString::new(name).unwrap(), len * 8, populate)?,
        })
    }

    fn insert<F: FnOnce(*mut u8)>(
        &self,
        thread_id: usize,
        allocator: &mut A,
        key: &[u8],
        size: usize,
        with: F,
    ) {
        unsafe {
            self.ebr().start(thread_id, allocator);
        }

        let view = self.view();
        let index = self.index(&key);

        let len = key.len();

        let handle_node = allocator.allocate(24).unwrap();
        let pointer_next = handle_node.as_ptr().cast::<u64>();
        let pointer_key = unsafe { handle_node.as_ptr().byte_add(8).cast::<u64>() };
        let pointer_value = unsafe { handle_node.as_ptr().byte_add(16).cast::<u64>() };

        let handle_key = allocator.allocate(8 + len).unwrap();
        unsafe {
            let pointer_key = handle_key.as_ptr();
            pointer_key.cast::<usize>().write(len);
            pointer_key
                .byte_add(8)
                .cast::<u8>()
                .copy_from_nonoverlapping(key.as_ptr(), len);
        }
        unsafe {
            allocator.link(pointer_key, &handle_key);
        }

        if size > 0 {
            let handle_value = allocator.allocate(size).unwrap();
            with(handle_value.as_ptr().cast::<u8>());
            unsafe {
                allocator.link(pointer_value, &handle_value);
            }
        } else {
            unsafe {
                pointer_value.write(0);
            }
        }

        let head = &view[index];
        loop {
            let next = head.load(Ordering::Acquire);

            unsafe {
                pointer_next.write(next);
            }

            match head.compare_exchange(next, u64::MAX, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => unsafe {
                    allocator.link(head.as_ptr(), &handle_node);
                    return;
                },
                Err(_) => continue,
            }
        }
    }

    fn get<F: FnOnce(&mut A, *const u8)>(
        &self,
        thread_id: usize,
        allocator: &mut A,
        key: &[u8],
        with: F,
    ) -> bool {
        unsafe {
            self.ebr().start(thread_id, allocator);
        }

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
            let handle = match NonZeroU64::new(head) {
                None => return false,
                Some(offset) => allocator.offset_to_handle(offset),
            };

            let pointer_next = handle.as_ptr().cast::<u64>();
            let pointer_walk = unsafe { handle.as_ptr().byte_add(8) };
            let pointer_value = unsafe { handle.as_ptr().byte_add(8 + 8).cast::<u8>() };

            let walk_len = unsafe { pointer_walk.cast::<usize>().read() };
            let walk =
                unsafe { slice::from_raw_parts(pointer_walk.byte_add(8).cast::<u8>(), walk_len) };

            match key == walk {
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

    fn ebr(&self) -> &ebr::Global {
        unsafe { self.ebr.address().as_ref().unwrap() }
    }

    fn view(&self) -> &[AtomicU64] {
        unsafe { std::slice::from_raw_parts(self.raw.address().cast::<AtomicU64>(), self.len) }
    }
}
