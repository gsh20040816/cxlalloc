use core::cell::UnsafeCell;
use core::ffi;
use core::marker::PhantomData;
use core::mem;
use core::num::NonZeroIsize;
use core::num::NonZeroU64;
use core::ops::Deref;
use core::ptr::NonNull;
use core::ptr::addr_of_mut;
use core::sync::atomic::AtomicIsize;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use crate::Allocator;
use crate::allocator::Handle as _;

pub struct Global {
    thread_total: usize,

    local: [Pad<UnsafeCell<Local>>; 64],

    // FIXME: replace with cache-line padded boolean array if highly contended
    token: AtomicUsize,
}

impl Global {
    pub unsafe fn init(global: *mut Self, thread_total: usize) {
        unsafe {
            *addr_of_mut!((*global).thread_total) = thread_total;
        }
    }

    pub unsafe fn start<A: Allocator>(&self, thread_id: usize, allocator: &mut A) {
        let rotate = self.has_token(thread_id);
        let local = unsafe { self.local[thread_id].get().as_mut().unwrap() };
        if rotate {
            local.rotate();
        }

        local.free(allocator);
    }

    pub unsafe fn retire<A: Allocator>(
        &self,
        thread_id: usize,
        allocator: &mut A,
        offset: NonZeroU64,
    ) {
        let local = unsafe { self.local[thread_id].get().as_mut().unwrap() };
        local.push(allocator, offset);
    }

    fn has_token(&self, thread_id: usize) -> bool {
        self.token.load(Ordering::Acquire) % self.thread_total == thread_id
    }
}

struct Local {
    new: Stack,
    old: Stack,
    free: Stack,
}

impl Local {
    fn rotate(&mut self) {
        Stack::transfer(&mut self.old, &mut self.free);
        mem::swap(&mut self.old, &mut self.new);
    }

    fn push<A: Allocator>(&mut self, allocator: &mut A, offset: NonZeroU64) {
        match self.new.head() {
            Some(stack) if stack.push(offset) => (),
            None | Some(_) => {
                let stack = self.new.push(allocator);
                assert!(stack.push(offset));
            }
        }
    }

    fn free<A: Allocator>(&mut self, allocator: &mut A) {
        self.free.pop(allocator);
    }
}

struct Stack {
    head: Option<Ptr<Block>>,
    tail: Option<Ptr<Block>>,
}

impl Stack {
    fn transfer(source: &mut Self, dest: &mut Self) {
        let Some(dest_tail) = &dest.tail else {
            // Fast path: dest is empty
            assert!(dest.tail.is_none());
            dest.head.store(source.head().map(NonNull::from));
            dest.tail.store(source.tail().map(NonNull::from));
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

        // Step 2: Link blocks together
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

        // Step 3: Update stack head and tail
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
        dest.tail.store(source.tail().map(NonNull::from));
        source.tail.store(source.head().map(NonNull::from));
    }

    fn push<A: Allocator>(&mut self, allocator: &mut A) -> &Block {
        let handle = allocator.allocate(mem::size_of::<Block>()).unwrap();

        // Initialize block
        let pointer = NonNull::new(handle.as_ptr().cast::<Block>()).unwrap();
        unsafe {
            libc::memset(
                pointer.as_ptr().cast::<ffi::c_void>(),
                0,
                mem::size_of::<Block>(),
            );

            pointer.as_ref().next.store(self.head(), Ordering::Relaxed);
        }

        // Update stack
        self.head.store(Some(pointer));
        if self.tail.is_none() {
            self.tail.store(Some(pointer));
        }

        unsafe { pointer.as_ref() }
    }

    fn pop<A: Allocator>(&mut self, allocator: &mut A) {
        // Empty queue
        let Some(head) = self.head.as_ref() else {
            return;
        };

        // Non-empty head
        if head.pop(allocator) {
            return;
        }

        // Pop and free empty head block
        let next = head.next().map(NonNull::from);
        let head = NonNull::from(head).cast::<ffi::c_void>();
        self.head.store(next);

        let offset = allocator.pointer_to_offset(head);
        let handle = allocator.offset_to_handle(offset.get()).unwrap();
        unsafe { allocator.deallocate(handle) };

        // Can recurse at most once
        self.pop(allocator);
    }

    fn head(&self) -> Option<&Block> {
        self.head.as_deref()
    }

    fn tail(&self) -> Option<&Block> {
        self.tail.as_deref()
    }
}

struct Block {
    next: AtomicPtr<Self>,
    len: AtomicUsize,
    data: [AtomicU64; Self::LEN],
}

const _: () = assert!(mem::size_of::<Block>() == 512);

impl Block {
    const LEN: usize = 62;

    fn push(&self, offset: NonZeroU64) -> bool {
        let index = match self.len() {
            index @ 0..Self::LEN => index,
            Self::LEN => return false,
            _ => unreachable!(),
        };

        self.data[index].store(offset.get(), Ordering::Relaxed);
        self.len.store(index + 1, Ordering::Relaxed);
        true
    }

    fn pop<A: Allocator>(&self, allocator: &mut A) -> bool {
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
    fn store(&mut self, address: Option<NonNull<T>>);
}

impl<T> PtrStore<T> for Option<Ptr<T>> {
    fn store(&mut self, address: Option<NonNull<T>>) {
        *self = match address {
            None => None,
            Some(address) => NonZeroIsize::new(unsafe { address.as_ptr().byte_offset_from(self) })
                .map(|offset| Ptr {
                    offset,
                    _type: PhantomData,
                }),
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
