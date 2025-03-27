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

use crate::Allocator;
use crate::Index;
use crate::allocator::Handle as _;

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
        allocator: &mut A,
        key: &[u8],
        size: usize,
        with: F,
    ) {
        let view = self.view();
        let index = self.index(&key);
        let mut probe = 0;

        let handle_node = allocator.allocate(8 + size).unwrap();
        let pointer_key = handle_node.as_ptr().cast::<u64>();
        let pointer_value = unsafe { handle_node.as_ptr().byte_add(8).cast::<u64>() };

        let handle_key = allocator.allocate(8 + key.len()).unwrap();
        unsafe {
            let pointer_key = handle_key.as_ptr();
            pointer_key.cast::<usize>().write(size);
            pointer_key
                .byte_add(8)
                .cast::<u8>()
                .copy_from_nonoverlapping(key.as_ptr(), key.len());
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
                    allocator.link(slot.as_ptr(), &handle_node);
                }
                return;
            }
        }
    }

    fn get<F: FnOnce(&mut A, *const u8)>(
        &self,
        _thread_id: usize,
        allocator: &mut A,
        key: &[u8],
        with: F,
    ) -> bool {
        let view = self.view();
        let index = self.index(&key);
        let mut probe = 0;

        loop {
            match NonZeroU64::new(view[(index + probe) % view.len()].load(Ordering::Acquire)) {
                None => return false,
                // Wait for link operation to complete
                Some(offset) if offset.get() == u64::MAX => hint::spin_loop(),
                Some(offset) => {
                    let handle = allocator.offset_to_handle(offset);

                    let pointer_walk = handle.as_ptr();
                    let walk_len = unsafe { pointer_walk.cast::<usize>().read() };
                    let walk = unsafe {
                        slice::from_raw_parts(pointer_walk.byte_add(8).cast::<u8>(), walk_len)
                    };

                    match key == walk {
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
