use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem;
use core::num::NonZeroIsize;
use core::ops::Deref;
use core::sync::atomic::AtomicIsize;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use crate::Allocator;

pub struct Global<A> {
    local: [Pad<UnsafeCell<Local>>; 64],

    token: AtomicU64,

    _allocator: PhantomData<fn() -> A>,
}

impl<A: Allocator> Global<A> {
    pub unsafe fn start(&self, thread_id: usize, allocator: &mut A) {
        let data = unsafe { self.local[thread_id].get().as_mut() };
    }
}

struct Local {
    new: Queue,
    old: Queue,
    free: Queue,
}

struct Queue {
    head: Option<Ptr<Stack>>,
    tail: Option<Ptr<Stack>>,
}

impl Queue {
    fn transfer<A: Allocator>(&mut self, allocator: &mut A, source: &mut Self) {
        let Some(dest_tail) = &self.tail else {
            // Fast path: self is empty
            assert!(self.tail.is_none());
            self.head = source.head.take();
            self.tail = source.tail.take();
            return;
        };

        // Step 1: Initial state
        //
        //   self.head                     self.tail
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
        //   self.head                     self.tail
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
        //   self.head
        // ┌───────────┐                 ┌───────────┐
        // │ dest_head ├─►     ...     ─►│ dest_tail │
        // └───────────┘                 └─────┬─────┘
        //                      ┌──────────────┘
        //                      ▼
        // ┌───────────┐  ┌───────────┐                 ┌───────────┐
        // │source_head│  │source_next├─►     ...     ─►│           │
        // └───────────┘  └───────────┘                 └───────────┘
        //  source.head                                   self.tail
        //  source.tail
        self.tail.store(
            source.tail().map(|tail| tail as *const _),
            Ordering::Relaxed,
        );
        source.tail.store(
            source.head().map(|head| head as *const _),
            Ordering::Relaxed,
        );
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
