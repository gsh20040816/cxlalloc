#![allow(clippy::missing_safety_doc)]

use core::cell::Cell;
use core::mem;
use core::ptr;
use std::alloc::Layout;
use std::cell::RefCell;
use std::ffi;
use std::ffi::CStr;
use std::ptr::NonNull;
use std::sync::Mutex;

use cxlalloc::raw;
use cxlalloc::Allocator;

fn ffi_or_null(apply: impl FnOnce() -> *mut ffi::c_void) -> *mut ffi::c_void {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(apply)).unwrap_or(ptr::null_mut())
}

static RAW: Mutex<Option<Box<raw::Raw>>> = Mutex::new(None);

thread_local! {
    static THREAD_ID: Cell<cxlalloc::thread::Id> = const { Cell::new(unsafe { cxlalloc::thread::Id::new(0) }) };

    // > Initialization is dynamically performed on the first call to with within a thread...
    //
    // https://doc.rust-lang.org/std/thread/struct.LocalKey.html
    static ALLOCATOR: RefCell<Option<Allocator<'static>>> = const { RefCell::new(None) };
}

fn with_allocator<R>(apply: impl FnOnce(&mut Allocator<'static>) -> R) -> R {
    ALLOCATOR.with_borrow_mut(|allocator| {
        if allocator.is_none() {
            *allocator = Some(raw().allocator(THREAD_ID.get()));
        }
        apply(allocator.as_mut().unwrap())
    })
}

fn try_with_allocator<R>(apply: impl FnOnce(&mut Allocator<'static>) -> R) -> Result<R, ()> {
    ALLOCATOR
        .try_with(|allocator| {
            let mut allocator = allocator.borrow_mut();
            if allocator.is_none() {
                *allocator = Some(raw().allocator(THREAD_ID.get()));
            }
            apply(allocator.as_mut().unwrap())
        })
        .map_err(|_| ())
}

fn reset_thread_allocator() {
    let _ = ALLOCATOR.try_with(|allocator| {
        *allocator.borrow_mut() = None;
    });
}

fn try_init_thread(thread_id: u16) -> bool {
    THREAD_ID
        .try_with(|id| id.set(unsafe { cxlalloc::thread::Id::new(thread_id) }))
        .is_ok()
}

enum BackendKind {
    Mmap,
    Shm,
    Ivshmem,
}

fn parse_heap_id(heap_id: *const ffi::c_char) -> &'static str {
    unsafe { CStr::from_ptr(heap_id) }
        .to_str()
        .expect("Heap ID must be valid UTF-8")
        // Hack for memento + ralloc compatibility
        .trim_start_matches("/dev/shm/")
}

fn parse_heap_backend(heap_backend: *const ffi::c_char) -> BackendKind {
    unsafe { CStr::from_ptr(heap_backend) }
        .to_str()
        .ok()
        .and_then(|backend| match backend {
            "mmap" => Some(BackendKind::Mmap),
            "shm" => Some(BackendKind::Shm),
            "ivshmem" => Some(BackendKind::Ivshmem),
            _ => None,
        })
        .expect("Heap backend one of [mmap, shm, ivshmem]")
}

fn make_backend(kind: BackendKind, heap_numa: i8) -> raw::Backend {
    let heap_numa = heap_numa.is_positive().then_some(shm::Numa::Bind {
        node: heap_numa as usize,
    });
    let builder = raw::Backend::builder().maybe_numa(heap_numa);
    match kind {
        BackendKind::Mmap => builder.backend(raw::backend::Mmap).build(),
        BackendKind::Shm => builder.backend(raw::backend::Shm).build(),
        BackendKind::Ivshmem => builder
            .backend(shm::backend::Ivshmem::new().expect("Failed to open ivshmem device"))
            .build(),
    }
}

fn reset_sigsegv_handler() {
    unsafe {
        let mut action = mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

fn handle_sigsegv(_: libc::c_int, info: *const libc::siginfo_t, _: *const libc::c_void) {
    let address = unsafe { info.read().si_addr() };
    let Some(raw) = try_raw() else {
        reset_sigsegv_handler();
        return;
    };
    let Ok(id) = THREAD_ID.try_with(Cell::get) else {
        reset_sigsegv_handler();
        return;
    };
    if raw.map(id, address) {
        return;
    }

    reset_sigsegv_handler();
}

unsafe fn init_process(
    heap_id: *const ffi::c_char,
    heap_numa: i8,
    heap_backend: *const ffi::c_char,
    small_heap_size: usize,
    large_heap_size: usize,
    large_reserve_size: usize,
    thread_count: u16,
    thread_id: u16,
    fixed_base: Option<usize>,
) {
    let heap_id = parse_heap_id(heap_id);
    let heap_backend = parse_heap_backend(heap_backend);

    let mut raw_guard = RAW.lock().expect("cxlalloc RAW mutex poisoned");
    if raw_guard.is_none() {
        #[cfg(feature = "log")]
        let _ = env_logger::Builder::from_default_env()
            .format(move |buffer, record| {
                use std::io::Write;
                use std::time::Instant;

                use env_logger::fmt::style;

                static START: OnceLock<Instant> = OnceLock::new();

                // Color-coded thread ID if there is more than one thread
                match THREAD_ID.with(|id| u16::from(id.get())) {
                    thread if thread_count > 1 => {
                        let style_thread = style::Ansi256Color::from(thread as u8).on_default();
                        write!(buffer, "[{style_thread}T{thread:02}{style_thread:#}]")?;
                    }
                    _ => (),
                }

                // Abbreviated log level
                let level = match record.level() {
                    log::Level::Error => "E",
                    log::Level::Warn => "W",
                    log::Level::Info => "I",
                    log::Level::Debug => "D",
                    log::Level::Trace => "T",
                };
                let style_level = buffer.default_level_style(record.level());
                write!(buffer, "[{style_level}{level}{style_level:#}]")?;

                // Nanosecond timestamp since `cxlalloc_init` was called
                // Zero-padded to 15 digits, which is 10^6 seconds ~ 278h
                let time = START.get_or_init(Instant::now).elapsed().as_nanos();
                write!(buffer, "[{time:015}]")?;

                writeln!(buffer, "[{}]: {}", record.target(), record.args())?;
                buffer.flush()?;
                Ok(())
            })
            .try_init();

        let mut action = unsafe { mem::zeroed::<libc::sigaction>() };
        action.sa_sigaction = handle_sigsegv as *const () as _;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
        unsafe {
            libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
        }

        let raw = raw::Raw::builder()
            .backend(make_backend(heap_backend, heap_numa))
            .size_small(small_heap_size)
            .size_large(large_heap_size)
            .reserve_large(large_reserve_size)
            .thread_count(thread_count as usize)
            .maybe_fixed_base(fixed_base)
            .build(heap_id)
            .expect("Failed to initialize allocator for process");
        *raw_guard = Some(Box::new(raw));
    }
    drop(raw_guard);

    if !try_init_thread(thread_id) {
        return;
    }

    // Eagerly initialize thread-local state to fail fast on buggy recovery
    let _ = try_with_allocator(|_| ());
}

/// Initialize the allocator for this process. This thread does not need to call
/// `cxlalloc_init_thread`.
///
/// `heap_id` is an application-defined string used to correlate heaps between processes.
/// `heap_numa` is -1 or else a NUMA node to bind heap memory to.
/// `heap_backend` must be one of [mmap, shm, ivshmem].
/// `heap_size` is the initial heap size in bytes.
/// `thread_count` is the total number of threads that will call the allocator.
/// `thread_id` must be (1) unique for each thread and (2) less than `thread_count`.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init_process(
    heap_id: *const ffi::c_char,
    heap_numa: i8,
    heap_backend: *const ffi::c_char,
    heap_size: usize,
    thread_count: u16,
    thread_id: u16,
) {
    init_process(
        heap_id,
        heap_numa,
        heap_backend,
        heap_size,
        0,
        0,
        thread_count,
        thread_id,
        None,
    );
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init_process_fixed(
    heap_id: *const ffi::c_char,
    heap_numa: i8,
    heap_backend: *const ffi::c_char,
    heap_size: usize,
    thread_count: u16,
    thread_id: u16,
    fixed_base: usize,
) {
    init_process(
        heap_id,
        heap_numa,
        heap_backend,
        heap_size,
        0,
        0,
        thread_count,
        thread_id,
        Some(fixed_base),
    );
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init_process_fixed_split(
    heap_id: *const ffi::c_char,
    heap_numa: i8,
    heap_backend: *const ffi::c_char,
    small_heap_size: usize,
    large_heap_size: usize,
    large_reserve_size: usize,
    thread_count: u16,
    thread_id: u16,
    fixed_base: usize,
) {
    init_process(
        heap_id,
        heap_numa,
        heap_backend,
        small_heap_size,
        large_heap_size,
        large_reserve_size,
        thread_count,
        thread_id,
        Some(fixed_base),
    );
}

#[no_mangle]
pub extern "C" fn cxlalloc_set_large_reserve_enabled(enabled: bool) {
    cxlalloc::set_large_reserve_enabled(enabled);
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_unlink_heap(
    heap_id: *const ffi::c_char,
    heap_backend: *const ffi::c_char,
) -> bool {
    let heap_id = parse_heap_id(heap_id);
    let heap_backend = parse_heap_backend(heap_backend);
    raw::Raw::unlink(heap_id, &make_backend(heap_backend, -1)).is_ok()
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_close_process() {
    reset_thread_allocator();
    let mut raw_guard = RAW.lock().expect("cxlalloc RAW mutex poisoned");
    *raw_guard = None;
    reset_sigsegv_handler();
}

/// Initialize the allocator for this thread.
///
/// `thread_id` must be (1) unique for each thread and (2) less than `thread_count`.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init_thread(thread_id: u16) {
    let _ = try_init_thread(thread_id);
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_malloc(size: usize) -> *mut ffi::c_void {
    ffi_or_null(|| {
        try_with_allocator(|allocator| allocator.allocate_untyped(size)).unwrap_or(ptr::null_mut())
    })
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_free(pointer: *mut ffi::c_void) {
    let Some(pointer) = NonNull::new(pointer) else {
        return;
    };

    match ALLOCATOR.try_with(|allocator| {
        if let Some(allocator) = allocator.borrow_mut().as_mut() {
            allocator.free_untyped(pointer.cast());
        }
    }) {
        Ok(()) => (),
        Err(_) => log::error!("Called cxlalloc_free({pointer:?}) after TLS destroyed"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_realloc(
    pointer: *mut ffi::c_void,
    size: usize,
) -> *mut ffi::c_void {
    let block = match NonNull::new(pointer) {
        None => return cxlalloc_malloc(size),
        Some(block) => block.cast(),
    };

    ffi_or_null(|| {
        try_with_allocator(|allocator| allocator.realloc_untyped(block, size)).unwrap_or(ptr::null_mut())
    })
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_memalign(size: usize, alignment: usize) -> *mut ffi::c_void {
    ffi_or_null(|| {
        let Ok(layout) = Layout::from_size_align(size, alignment) else {
            return ptr::null_mut();
        };
        try_with_allocator(|allocator| allocator.allocate_untyped(layout.pad_to_align().size()))
            .unwrap_or(ptr::null_mut())
    })
}

/// Try to convert a pointer into a persistent offset. Returns false if the pointer was
/// not allocated in this heap.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_pointer_to_offset(
    pointer: *const ffi::c_void,
    offset: *mut u64,
) -> bool {
    match NonNull::new(pointer as *mut ffi::c_void)
        .map(|pointer| with_allocator(|allocator| allocator.pointer_to_offset(pointer)))
    {
        None => false,
        Some(_offset) => {
            offset.write_volatile(_offset as u64);
            true
        }
    }
}

/// Convert a persistent offset into a pointer in this process address space.
#[no_mangle]
pub extern "C" fn cxlalloc_offset_to_pointer(offset: u64) -> *mut ffi::c_void {
    with_allocator(|allocator| allocator.offset_to_pointer(offset as usize).as_ptr())
}

#[inline]
fn raw() -> &'static raw::Raw {
    try_raw().expect("Uninitialized heap: was cxlalloc_init called?")
}

#[inline]
fn try_raw() -> Option<&'static raw::Raw> {
    let raw_guard = RAW.lock().expect("cxlalloc RAW mutex poisoned");
    let raw = raw_guard.as_ref()?.as_ref() as *const raw::Raw;
    drop(raw_guard);
    Some(unsafe { &*raw })
}
