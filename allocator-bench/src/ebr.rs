use core::cell::UnsafeCell;
use core::ffi;
use core::marker::PhantomData;
use core::mem;
use core::num::NonZeroIsize;
use core::ops::Deref;
use core::ptr::NonNull;
use core::sync::atomic::AtomicIsize;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use crate::Allocator;

pub struct Global {
    thread_count: usize,

    local: [Pad<UnsafeCell<Local>>; 64],

    // FIXME: replace with cache-line padded boolean array if highly contended
    token: AtomicUsize,
}

impl Global {
    pub unsafe fn start<A: Allocator<Handle = NonNull<ffi::c_void>>>(
        &self,
        thread_id: usize,
        allocator: &mut A,
    ) {
        let rotate = self.has_token(thread_id);
        let local = unsafe { self.local[thread_id].get().as_mut().unwrap() };
        if rotate {
            local.rotate();
        }

        local.free(allocator);
    }

    fn has_token(&self, thread_id: usize) -> bool {
        self.token.load(Ordering::Acquire) % self.thread_count == thread_id
    }
}

struct Local {
    new: Queue,
    old: Queue,
    free: Queue,
}

impl Local {
    fn rotate(&mut self) {
        Queue::transfer(&mut self.old, &mut self.free);
        mem::swap(&mut self.old, &mut self.new);
    }

    fn free<A: Allocator<Handle = NonNull<ffi::c_void>>>(&mut self, allocator: &mut A) {
        self.free.free(allocator);
    }
}

struct Queue {
    head: Option<Ptr<Stack>>,
    tail: Option<Ptr<Stack>>,
}

impl Queue {
    fn transfer(source: &mut Self, dest: &mut Self) {
        let Some(dest_tail) = &dest.tail else {
            // Fast path: dest is empty
            assert!(dest.tail.is_none());
            dest.head.store(
                source.head().map(|head| head as *const _),
                Ordering::Relaxed,
            );
            dest.tail.store(
                source.tail().map(|tail| tail as *const _),
                Ordering::Relaxed,
            );
            source.head.take();
            source.tail.take();
            return;
        };

        // Step 1: Initial state
        //
        //   dest.head                     dest.tail
        // ┌───────────┐                 ┌───────────┐
        // │ dest_head ├─►     ...     ─►│ dest_tail │
        // └───────────┘                 └───────────┘
        // ┌───────────┐  ┌───────────┐                 ┌───────────┐
        // │source_head│─►│source_next├─►     ...     ─►│           │
        // └───────────┘  └───────────┘                 └───────────┘
        //  source.head                                  source.tail
        let Some(source_head) = source.head() else {
            return;
        };
        let Some(source_next) = source_head.next() else {
            return;
        };

        // Step 2: Link stacks together
        //
        //   dest.head                     dest.tail
        // ┌───────────┐                 ┌───────────┐
        // │ dest_head ├─►     ...     ─►│ dest_tail │
        // └───────────┘                 └─────┬─────┘
        //                      ┌──────────────┘
        //                      ▼
        // ┌───────────┐  ┌───────────┐                 ┌───────────┐
        // │source_head│  │source_next├─►     ...     ─►│           │
        // └───────────┘  └───────────┘                 └───────────┘
        //  source.head                                  source.tail
        dest_tail.next.store(Some(source_next), Ordering::Relaxed);
        source_head.next.store(None, Ordering::Relaxed);

        // Step 3: Update queue head and tail
        //
        //   dest.head
        // ┌───────────┐                 ┌───────────┐
        // │ dest_head ├─►     ...     ─►│ dest_tail │
        // └───────────┘                 └─────┬─────┘
        //                      ┌──────────────┘
        //                      ▼
        // ┌───────────┐  ┌───────────┐                 ┌───────────┐
        // │source_head│  │source_next├─►     ...     ─►│           │
        // └───────────┘  └───────────┘                 └───────────┘
        //  source.head                                   dest.tail
        //  source.tail
        dest.tail.store(
            source.tail().map(|tail| tail as *const _),
            Ordering::Relaxed,
        );
        source.tail.store(
            source.head().map(|head| head as *const _),
            Ordering::Relaxed,
        );
    }

    fn free<A: Allocator<Handle = NonNull<ffi::c_void>>>(&mut self, allocator: &mut A) {
        // Empty queue
        let Some(head) = self.head.as_ref() else {
            return;
        };

        // Non-empty head
        if head.free(allocator) {
            return;
        }

        // Dequeue and free empty head stack
        let next = head.next().map(|next| next as *const _);
        let head = NonNull::from(head).cast::<ffi::c_void>();
        self.head.store(next, Ordering::Relaxed);
        unsafe { allocator.deallocate(head) };

        // Can recurse at most once
        self.free(allocator);
    }

    fn head(&self) -> Option<&Stack> {
        self.head.as_deref()
    }

    fn tail(&self) -> Option<&Stack> {
        self.tail.as_deref()
    }
}

struct Stack {
    next: AtomicPtr<Self>,
    len: AtomicUsize,
    data: [AtomicU64; 62],
}

impl Stack {
    fn free<A: Allocator>(&self, allocator: &mut A) -> bool {
        let Some(index) = self.len().checked_sub(1) else {
            return false;
        };

        let offset = self.data[index].load(Ordering::Relaxed);
        let handle = allocator.offset_to_handle(offset).unwrap();
        unsafe {
            allocator.deallocate(handle);
        }

        self.data[index - 1].store(0, Ordering::Relaxed);
        self.len.store(index, Ordering::Relaxed);
        true
    }

    fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        self.len() == self.data.len()
    }

    fn next(&self) -> Option<&Self> {
        self.next.load(Ordering::Relaxed)
    }
}

const _: () = assert!(mem::size_of::<Stack>() == 512);

struct AtomicPtr<T> {
    offset: AtomicIsize,
    _type: PhantomData<T>,
}

impl<T> AtomicPtr<T> {
    fn load(&self, ordering: Ordering) -> Option<&T> {
        let offset = NonZeroIsize::new(self.offset.load(ordering))?;
        let base = self as *const Self;
        unsafe { base.byte_offset(offset.get()).cast::<T>().as_ref() }
    }

    fn store(&self, address: Option<&T>, ordering: Ordering) {
        let offset = match address {
            None => 0,
            Some(address) => {
                let address = address as *const T;
                unsafe { address.byte_offset_from(self) }
            }
        };

        self.offset.store(offset, ordering);
    }
}

#[repr(align(64))]
struct Pad<T>(T);

impl<T> Deref for Pad<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct Ptr<T> {
    offset: NonZeroIsize,
    _type: PhantomData<T>,
}

trait PtrStore<T> {
    fn store(&mut self, address: Option<*const T>, ordering: Ordering);
}

impl<T> PtrStore<T> for Option<Ptr<T>> {
    fn store(&mut self, address: Option<*const T>, ordering: Ordering) {
        *self = match address {
            None => None,
            Some(address) => {
                NonZeroIsize::new(unsafe { address.byte_offset_from(self) }).map(|offset| Ptr {
                    offset,
                    _type: PhantomData,
                })
            }
        };
    }
}

impl<T> Deref for Ptr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        let base = self as *const Self;
        let offset = self.offset.get();
        unsafe {
            base.byte_offset(offset)
                .cast::<Self::Target>()
                .as_ref()
                .unwrap()
        }
    }
}
