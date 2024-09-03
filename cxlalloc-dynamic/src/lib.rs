//! This API was obtained by building mimalloc2 from [mimalloc-bench][mb],
//! and then running the following command to filter external symbols:
//!
//! ```bash
//! nm -gU libmimalloc.so | rg -v "T (mi_|_)"
//! ```
//!
//! [mb]: https://github.com/daanx/mimalloc-bench

#![allow(unused_variables)]

use std::alloc::Layout;
use std::cell::RefCell;
use std::ffi;
use std::ptr::NonNull;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::LazyLock;

// Note: it would be nice to initialize this with an initialization
// function, but that doesn't work well with `LD_PRELOAD`.
//
// The problem is essentially that ld uses the static dependency
// graph defined by `DT_NEEDED` section elements to determine the
// order of `.init` calls. Using `LD_PRELOAD` causes all symbols
// to resolve to our shim, including symbols that are used by
// initialization functions--in our case, libstdc++'s exception
// handling code [eh_alloc.cc](https://github.com/gcc-mirror/gcc/blob/4883c9571f5fb8fc7e873bb8a31aa164c5cfd0e0/libstdc%2B%2B-v3/libsupc%2B%2B/eh_alloc.cc).
// So the net effect is that malloc is bound to our shim,
// libstdc++'s initialization code is run, and calls into
// our shim *before* we can run initialization.
//
// Instead, we add some runtime overhead for now to lazily
// initialize both thread-local and global state upon first access.
static RAW: LazyLock<cxlalloc::raw::Heap> = LazyLock::new(|| {
    log::set_max_level(log::LevelFilter::Info);
    log::set_logger(&Logger).unwrap();

    let raw = cxlalloc::raw::Builder::default()
        .size(1usize << 34)
        .thread_count(64)
        .build("cxl")
        .unwrap();

    log::info!("initialized heap");
    raw
});

// The behavior of these thread locals is unfortunately quite hairy.
//
// The entrypoint is some call to `malloc`, which triggers lazy
// initialization of `ALLOCATOR`, which triggers lazy initialization
// of `THREAD_ID`. The `ThreadId` is not `const`, and our target (Linux on x86-64)
// has a native TLS implementation, so we take [this][init] codepath
// the first time it is accessed.
//
// Trace:
// - malloc
//   - ALLOCATOR::initialize
//     - THREAD_ID::initialize
//
// Because `ThreadId` has a `Drop` implementation, when `register_dtor`
// is called, we execute the linux-like destructor registration
// [here][dtor-rust], which calls into [__cxa_thread_atexit_impl][dtor-c],
// which calls `calloc`, causing us to recurse into the `ALLOCATOR`
// initialization.
//
// Trace:
// - malloc
//   - ALLOCATOR::initialize
//     - THREAD_ID::initialize
//       - state = State::Alive(thread_id)
//       - __cxa_thread_atexit_impl
//         - calloc
//           - ALLOCATOR::initialize
//
// At this point, we do have an initialized `THREAD_ID` (whose destructor
// registration is incomplete). The inner `ALLOCATOR` initialization
// successfully retrieves `THREAD_ID`, triggers `RAW` initialization,
// and returns an allocation to `calloc`. Now we need to be careful
// about the outer `ALLOCATOR::initialize`: just calling `RAW.allocator(...)`
// results in recovery logic being run and reclaiming the `calloc`
// allocation from the inner allocator. This manifests as a SEGFAULT
// at thread exit when the [func][func] pointer is called:
//
// Trace:
// - malloc
//   - ALLOCATOR::initialize
//     - THREAD_ID::initialize
//       - state = State::Alive(thread_id)
//       - __cxa_thread_atexit_impl
//         - calloc
//           - ALLOCATOR::initialize
//             - RAW::initialize
//     - Raw::allocator(thread_id)
//       - Heap::recover
// - __call_tls_dtors
//   - cur->func(cur->obj)
//   - SEGFAULT (cur reclaimed by Heap::recover)
//
// For now, we work around this by not calling recovery.
// Since this crate is only used for `LD_PRELOAD`-based performance
// benchmarks, we should never need recovery.
//
// Note: the semantics of recursive TLS initialization seem underspecified
// as of now. The current [implementation][impl] silently drops the inner
// allocator, which is fine for our purposes, but I couldn't find any
// recent discussion beyond [this issue][issue]
//
// [init]: https://github.com/rust-lang/rust/blob/0b5eb7ba7bd796fb39c8bb6acd9ef6c140f28b65/library/std/src/sys/thread_local/native/lazy.rs#L61-L81
// [dtor-rust]: https://github.com/rust-lang/rust/blob/0b5eb7ba7bd796fb39c8bb6acd9ef6c140f28b65/library/std/src/sys/thread_local/destructors/linux_like.rs#L18
// [dtor-c]: https://github.com/kraj/glibc/blob/11ad033e1c09c8b8e7bbaa72420f41ab8bcf0f63/stdlib/cxa_thread_atexit_impl.c#L101
// [func]: https://github.com/kraj/glibc/blob/11ad033e1c09c8b8e7bbaa72420f41ab8bcf0f63/stdlib/cxa_thread_atexit_impl.c#L84
// [impl]: https://github.com/rust-lang/rust/blob/0b5eb7ba7bd796fb39c8bb6acd9ef6c140f28b65/library/std/src/sys/thread_local/native/lazy.rs#L72-L73
// [issue]: https://github.com/rust-lang/rust/issues/30228
thread_local! {
    static THREAD_ID: ThreadId = {
        let id = ThreadId::new();
        log::info!("Initialized id: {id:?}");
        id
    };

    static ALLOCATOR: RefCell<cxlalloc::Allocator<'static>> = {
        let id = THREAD_ID.with(ThreadId::get);

        // let allocator = unsafe { RAW.allocator_assume_init(id) };
        let allocator = RAW.allocator(unsafe { cxlalloc::thread::Id::new( id as u16) });

        log::info!("Initialized allocator: {id}");
        RefCell::new(allocator)
    };
}

#[no_mangle]
pub unsafe extern "C" fn aligned_alloc(_alignment: usize, _size: usize) -> *mut ffi::c_void {
    unimplemented!("aligned_alloc")
}

#[no_mangle]
pub unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut ffi::c_void {
    log::trace!("calloc {count} * {size}");
    let allocation = malloc(size * count);
    std::ptr::write_bytes(allocation, 0, size * count);
    allocation
}

#[no_mangle]
pub unsafe extern "C" fn cfree(_pointer: *mut ffi::c_void) {
    unimplemented!("cfree")
}

#[no_mangle]
pub unsafe extern "C" fn free(pointer: *mut ffi::c_void) {
    log::trace!("free {pointer:?}");
    let Some(pointer) = NonNull::new(pointer) else {
        return;
    };

    todo!()
    // ALLOCATOR.with_borrow_mut(|allocator| allocator.free_untyped(pointer.cast()))
}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut ffi::c_void {
    log::trace!("malloc {size}");
    // ALLOCATOR.with_borrow_mut(|allocator| allocator.allocate_untyped(size).as_ptr().cast())
    todo!()
}

#[no_mangle]
pub unsafe extern "C" fn malloc_good_size(_size: usize) -> usize {
    unimplemented!("malloc_good_size")
}

#[no_mangle]
pub unsafe extern "C" fn malloc_size(_size: usize) -> usize {
    unimplemented!("malloc_size")
}

#[no_mangle]
pub unsafe extern "C" fn malloc_usable_size(pointer: *mut ffi::c_void) -> usize {
    let Some(pointer) = NonNull::new(pointer) else {
        return 0;
    };

    // ALLOCATOR.with_borrow(|allocator| {
    //     let offset = allocator.heap().pointer_to_offset(pointer);
    //     allocator.heap().class(offset).size()
    // })
    todo!()
}

#[no_mangle]
pub unsafe extern "C" fn memalign(alignment: usize, size: usize) -> *mut ffi::c_void {
    // ALLOCATOR.with_borrow_mut(|allocator| {
    //     allocator
    //         .allocate_aligned_untyped(Layout::from_size_align(size, alignment).unwrap())
    //         .as_ptr()
    //         .cast()
    // })
    todo!()
}

#[no_mangle]
pub unsafe extern "C" fn posix_memalign(
    pointer: *mut *mut ffi::c_void,
    alignment: usize,
    size: usize,
) -> ffi::c_int {
    if size == 0 {
        return -1;
    }

    let allocation = memalign(alignment, size);
    *pointer = allocation;
    0
}

#[no_mangle]
pub unsafe extern "C" fn pvalloc(_size: usize) -> *mut ffi::c_void {
    unimplemented!("pvalloc")
}

#[no_mangle]
pub unsafe extern "C" fn realloc(pointer: *mut ffi::c_void, size: usize) -> *mut ffi::c_void {
    log::trace!("realloc {pointer:?} {size}");
    let Some(pointer) = NonNull::new(pointer) else {
        return malloc(size);
    };

    // ALLOCATOR.with_borrow_mut(|allocator| {
    //     allocator
    //         .realloc_untyped(pointer.cast(), size)
    //         .as_ptr()
    //         .cast()
    // })
    todo!()
}

#[no_mangle]
pub unsafe extern "C" fn reallocarray(
    _pointer: *mut ffi::c_void,
    _count: usize,
    _size: usize,
) -> *mut ffi::c_void {
    unimplemented!("reallocarray")
}

#[no_mangle]
pub unsafe extern "C" fn reallocf(_pointer: *mut ffi::c_void, _size: usize) -> *mut ffi::c_void {
    unimplemented!("reallocf")
}

#[no_mangle]
pub unsafe extern "C" fn valloc(_size: usize) -> *mut ffi::c_void {
    unimplemented!("valloc")
}

#[no_mangle]
pub unsafe extern "C" fn vfree(_pointer: *mut ffi::c_void) {
    unimplemented!("vfree")
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}]: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static ID: AtomicUsize = AtomicUsize::new(0xFFFF_FFFF_FFFF_FFFF);

#[derive(Debug)]
struct ThreadId(usize);

impl ThreadId {
    fn new() -> Self {
        let mut prev = ID.load(Ordering::Acquire);
        loop {
            let next = prev & (prev - 1);
            let id = prev & !(prev - 1);
            match ID.compare_exchange(prev, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break Self(id.trailing_zeros() as usize),
                Err(current) => prev = current,
            }
        }
    }

    fn get(&self) -> usize {
        self.0
    }
}

impl Drop for ThreadId {
    fn drop(&mut self) {
        ID.fetch_or(1 << self.0, Ordering::AcqRel);
    }
}
