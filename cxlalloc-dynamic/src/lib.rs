//! This API was obtained by building mimalloc2 from [mimalloc-bench][mb],
//! and then running the following command to filter external symbols:
//!
//! ```bash
//! nm -gU libmimalloc.so | rg -v "T (mi_|_)"
//! ```
//!
//! [mb]: https://github.com/daanx/mimalloc-bench

mod stat;

use core::alloc::Layout;
use core::cell::Cell;
use core::cell::UnsafeCell;
use core::ffi;
use core::mem;
use core::mem::ManuallyDrop;
use core::mem::MaybeUninit;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use std::sync::LazyLock;

/// We explicitly opt out of the system allocator so that
/// `cxlalloc` can allocate DRAM internally without recursion.
/// For now, this is mainly used for statistics, and not
/// during actual operation.
#[global_allocator]
static MI_MALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[repr(transparent)]
struct Key(core::cell::UnsafeCell<libc::pthread_key_t>);

unsafe impl Sync for Key {}

static DESTRUCTOR: Key = unsafe { Key(MaybeUninit::zeroed().assume_init()) };

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
static RAW: LazyLock<cxlalloc::raw::Raw> = LazyLock::new(|| {
    log::set_max_level(log::LevelFilter::Info);
    log::set_logger(&Logger).unwrap();

    unsafe {
        assert_eq!(
            libc::pthread_key_create(DESTRUCTOR.0.get(), Some(on_pthread_exit)),
            0,
            "pthread_key_create failed",
        );
    }

    let mut action = unsafe { mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = handle_sigsegv as _;
    action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;

    unsafe {
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }

    cxlalloc::raw::Builder::default()
        .size(1usize << 34)
        .thread_count(64)
        .build("cxl")
        .expect("Heap creation failed")
});

fn handle_sigsegv(_: libc::c_int, info: *const libc::siginfo_t, _: *const libc::c_void) {
    let address = unsafe { info.read().si_addr() };

    if RAW.map(address) {
        return;
    }

    unsafe {
        let mut action = mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

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
    static THREAD_ID: Cell<usize> = Cell::new(thread_id());

    static ALLOCATOR: UnsafeCell<ManuallyDrop<cxlalloc::Allocator<'static>>> = {
        // Ensure heap has been initialized
        let raw = LazyLock::force(&RAW);

        unsafe {
            // Destructor will only run if key is non-null.
            assert_eq!(
                libc::pthread_setspecific(*DESTRUCTOR.0.get(), NonNull::dangling().as_ptr()),
                0,
                "pthread_setspecific failed",
            );
        }

        let id = THREAD_ID.get();
        let allocator = raw.allocator(unsafe { cxlalloc::thread::Id::new(id as u16) });

        UnsafeCell::new(ManuallyDrop::new(allocator))
    };
}

#[no_mangle]
pub unsafe extern "C" fn aligned_alloc(_alignment: usize, _size: usize) -> *mut ffi::c_void {
    unimplemented!("aligned_alloc")
}

#[no_mangle]
pub unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut ffi::c_void {
    log::trace!("calloc {count} * {size}");
    stat::inc(&stat::CALLOC);

    let allocation = malloc(size * count);
    stat::dec(&stat::MALLOC);

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
    stat::inc(&stat::FREE);

    let Some(pointer) = NonNull::new(pointer) else {
        return;
    };

    with_mut(|allocator| allocator.free_untyped(pointer))
}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut ffi::c_void {
    log::trace!("malloc {size}");
    stat::inc(&stat::MALLOC);

    with_mut(|allocator| allocator.allocate_untyped(size))
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
    stat::inc(&stat::MALLOC_USABLE_SIZE);

    let Some(pointer) = NonNull::new(pointer) else {
        return 0;
    };

    with(|allocator| allocator.class_untyped(pointer))
}

#[no_mangle]
pub unsafe extern "C" fn memalign(alignment: usize, size: usize) -> *mut ffi::c_void {
    stat::inc(&stat::MEMALIGN);

    with_mut(|allocator| {
        // FIXME: pass layout directly
        allocator
            .allocate_untyped(
                Layout::from_size_align(size, alignment)
                    .unwrap()
                    .pad_to_align()
                    .size(),
            )
            .cast()
    })
}

#[no_mangle]
pub unsafe extern "C" fn posix_memalign(
    pointer: *mut *mut ffi::c_void,
    alignment: usize,
    size: usize,
) -> ffi::c_int {
    stat::inc(&stat::POSIX_MEMALIGN);

    if size == 0 {
        return -1;
    }

    let allocation = memalign(alignment, size);
    stat::dec(&stat::MEMALIGN);

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
    stat::inc(&stat::REALLOC);

    let Some(pointer) = NonNull::new(pointer) else {
        let allocation = malloc(size);
        stat::dec(&stat::MALLOC);
        return allocation;
    };

    with_mut(|allocator| allocator.realloc_untyped(pointer.cast(), size).cast())
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

unsafe fn with<F: FnOnce(&cxlalloc::Allocator) -> T, T>(apply: F) -> T {
    ALLOCATOR.with(|allocator| apply(&*allocator.get()))
}

unsafe fn with_mut<F: FnOnce(&mut cxlalloc::Allocator) -> T, T>(apply: F) -> T {
    ALLOCATOR.with(|allocator| apply(&mut *allocator.get()))
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

static GLOBAL_LO: AtomicUsize = AtomicUsize::new(0xFFFF_FFFF_FFFF_FFFF);
static GLOBAL_HI: AtomicUsize = AtomicUsize::new(0xFFFF_FFFF_FFFF_FFFF);

fn thread_id() -> usize {
    let mut prev = GLOBAL_LO.load(Ordering::Acquire);
    while prev > 0 {
        let next = prev & (prev - 1);
        let id = prev & !(prev - 1);
        match GLOBAL_LO.compare_exchange(prev, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return id.trailing_zeros() as usize,
            Err(current) => prev = current,
        }
    }

    let mut prev = GLOBAL_HI.load(Ordering::Acquire);
    while prev > 0 {
        let next = prev & (prev - 1);
        let id = prev & !(prev - 1);
        match GLOBAL_HI.compare_exchange(prev, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return id.trailing_zeros() as usize + 64,
            Err(current) => prev = current,
        }
    }

    unreachable!()
}

unsafe extern "C" fn on_pthread_exit(_: *mut libc::c_void) {
    on_exit();
}

#[ctor::dtor]
fn on_exit() {
    let id = THREAD_ID.get();
    if id < 64 {
        GLOBAL_LO.fetch_or(1 << id, Ordering::AcqRel);
    } else {
        GLOBAL_HI.fetch_or(1 << id, Ordering::AcqRel);
    }
    cxlalloc::stat::dump(id);
    stat::dump_counters(id);
    let _ = ALLOCATOR.try_with(|allocator| unsafe { ManuallyDrop::drop(&mut *allocator.get()) });
}
