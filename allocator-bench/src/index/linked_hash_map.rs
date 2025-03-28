use core::hash::Hash;
use core::hash::Hasher as _;
use core::hint;
use core::num::NonZeroU64;
use core::num::NonZeroUsize;
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

        let bucket = self.bucket(key);

        // Initialize value
        let handle_value = match NonZeroUsize::new(size) {
            None => None,
            Some(size) => {
                let handle_value = allocator.allocate(size.get()).unwrap();
                with(handle_value.as_ptr().cast::<u8>());
                Some(handle_value)
            }
        };
        let offset_value = handle_value
            .as_ref()
            .map(|handle| unsafe { allocator.handle_to_offset(handle) })
            .map(NonZeroU64::get)
            .unwrap_or(0);

        // Fast path: swap value in place
        if self.try_swap(
            thread_id,
            allocator,
            bucket.load(Ordering::Relaxed),
            key,
            offset_value,
        ) {
            return;
        }

        // Slow path: allocate node
        let handle_node = allocator.allocate(24).unwrap();
        let offset_node = unsafe { allocator.handle_to_offset(&handle_node) };

        // Initialize key
        let pointer_key = unsafe { handle_node.as_ptr().byte_add(8).cast::<u64>() };
        let handle_key = allocator.allocate(8 + key.len()).unwrap();
        unsafe {
            let pointer_key = handle_key.as_ptr();
            pointer_key.cast::<usize>().write(key.len());
            pointer_key
                .byte_add(8)
                .cast::<u8>()
                .copy_from_nonoverlapping(key.as_ptr(), key.len());
        }
        unsafe {
            allocator.link(pointer_key, &handle_key);
        }

        // Link value into node
        let pointer_value = unsafe { handle_node.as_ptr().byte_add(16).cast::<u64>() };
        match handle_value.as_ref() {
            None => unsafe { pointer_value.write(0) },
            Some(handle) => unsafe { allocator.link(pointer_value, handle) },
        }

        let pointer_next = handle_node.as_ptr().cast::<u64>();

        loop {
            // Must store current head before calling find
            let head = bucket.load(Ordering::Relaxed);
            unsafe { pointer_next.write(head) };

            if self.try_swap(thread_id, allocator, head, key, offset_value) {
                unsafe {
                    allocator.deallocate(handle_key);
                    allocator.deallocate(handle_node);
                }
                return;
            }

            // Try to CAS new node at head
            // FIXME: ABA problem
            if bucket
                .compare_exchange(head, offset_node.get(), Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
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

        let bucket = self.bucket(key);

        let head = loop {
            match bucket.load(Ordering::Acquire) {
                0 => return false,
                u64::MAX => {
                    hint::spin_loop();
                    continue;
                }
                offset => break offset,
            }
        };

        let Some(handle) = self.find(allocator, key, head) else {
            return false;
        };

        let offset_value =
            NonZeroU64::new(unsafe { handle.as_ptr().byte_add(16).cast::<u64>().read() }).unwrap();
        let handle_value = allocator.offset_to_handle(offset_value);
        with(allocator, handle_value.as_ptr().cast());
        true
    }

    fn unlink(&mut self) -> io::Result<()> {
        self.ebr.unlink()?;
        self.raw.unlink()
    }
}

impl LinkedHashMap {
    fn bucket(&self, key: &[u8]) -> &AtomicU64 {
        let mut hasher = RapidHasher::default();
        key.hash(&mut hasher);
        &self.view()[hasher.finish() as usize % self.len]
    }

    fn try_swap<A: Allocator>(
        &self,
        thread_id: usize,
        allocator: &mut A,
        head: u64,
        key: &[u8],
        value: u64,
    ) -> bool {
        let Some(handle_node) = self.find(allocator, key, head) else {
            return false;
        };

        let offset =
            unsafe { AtomicU64::from_ptr(handle_node.as_ptr().byte_add(16).cast::<u64>()) };
        let old = NonZeroU64::new(offset.swap(value, Ordering::AcqRel)).unwrap();
        unsafe { self.ebr().retire(thread_id, allocator, old) }
        true
    }

    fn find<A: Allocator>(&self, allocator: &mut A, target: &[u8], head: u64) -> Option<A::Handle> {
        let offset = NonZeroU64::new(head)?;
        let mut handle = allocator.offset_to_handle(offset);

        loop {
            let offset_key =
                NonZeroU64::new(unsafe { handle.as_ptr().byte_add(8).cast::<u64>().read() })
                    .unwrap();

            let handle_key = allocator.offset_to_handle(offset_key);
            let key_len = unsafe { handle_key.as_ptr().cast::<usize>().read() };
            let key = unsafe {
                slice::from_raw_parts(handle_key.as_ptr().cast::<u8>().byte_add(8), key_len)
            };

            if key == target {
                return Some(handle);
            }

            let offset_next = NonZeroU64::new(unsafe { handle.as_ptr().cast::<u64>().read() })?;
            handle = allocator.offset_to_handle(offset_next);
        }
    }

    fn ebr(&self) -> &ebr::Global {
        unsafe { self.ebr.address().as_ref().unwrap() }
    }

    fn view(&self) -> &[AtomicU64] {
        unsafe { std::slice::from_raw_parts(self.raw.address().cast::<AtomicU64>(), self.len) }
    }
}
