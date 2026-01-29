#![allow(clippy::missing_safety_doc)]

use core::cell::Cell;
use core::mem;
use core::ptr;
use std::alloc::Layout;
use std::cell::RefCell;
use std::ffi;
use std::ffi::CStr;
use std::ptr::NonNull;
use std::sync::OnceLock;

use cxlalloc::raw;
use cxlalloc::Allocator;

static RAW: OnceLock<raw::Raw> = OnceLock::new();

fn handle_sigsegv(_: libc::c_int, info: *const libc::siginfo_t, _: *const libc::c_void) {
    let address = unsafe { info.read().si_addr() };
    let id = THREAD_ID.get();
    if raw().map(id, address) {
        return;
    }

    unsafe {
        let mut action = mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

thread_local! {
    static THREAD_ID: Cell<cxlalloc::thread::Id> = const { Cell::new(unsafe { cxlalloc::thread::Id::new(0) }) };

    // > Initialization is dynamically performed on the first call to with within a thread...
    //
    // https://doc.rust-lang.org/std/thread/struct.LocalKey.html
    static ALLOCATOR: RefCell<Allocator<'static>> = RefCell::new(raw().allocator(THREAD_ID.get()));
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
    let heap_id = CStr::from_ptr(heap_id)
        .to_str()
        .expect("Heap ID must be valid UTF-8")
        // Hack for memento + ralloc compatibility
        .trim_start_matches("/dev/shm/");

    enum Backend {
        Mmap,
        Shm,
        Ivshmem,
    }

    let heap_backend = CStr::from_ptr(heap_backend)
        .to_str()
        .ok()
        .and_then(|backend| match backend {
            "mmap" => Some(Backend::Mmap),
            "shm" => Some(Backend::Shm),
            "ivshmem" => Some(Backend::Ivshmem),
            _ => None,
        })
        .expect("Heap backend one of [mmap, shm, ivshmem]");

    let heap_numa = heap_numa.is_positive().then_some(shm::Numa::Bind {
        node: heap_numa as usize,
    });

    RAW.get_or_init(move || {
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
        action.sa_sigaction = handle_sigsegv as _;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
        unsafe {
            libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
        }

        let builder = raw::Backend::builder().maybe_numa(heap_numa);
        let backend = match heap_backend {
            Backend::Mmap => builder.backend(raw::backend::Mmap).build(),
            Backend::Shm => builder.backend(raw::backend::Shm).build(),
            Backend::Ivshmem => builder
                .backend(shm::backend::Ivshmem::new().expect("Failed to open ivshmem device"))
                .build(),
        };

        raw::Raw::builder()
            .backend(backend)
            .size_small(heap_size)
            .thread_count(thread_count as usize)
            .build(heap_id)
            .expect("Failed to initialize allocator for process")
    });

    cxlalloc_init_thread(thread_id);

    // Eagerly initialize thread-local state to fail fast on buggy recovery
    ALLOCATOR.with(|_| ());
}

/// Initialize the allocator for this thread.
///
/// `thread_id` must be (1) unique for each thread and (2) less than `thread_count`.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init_thread(thread_id: u16) {
    THREAD_ID.set(unsafe { cxlalloc::thread::Id::new(thread_id) });
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_malloc(size: usize) -> *mut ffi::c_void {
    ALLOCATOR.with_borrow_mut(|allocator| allocator.allocate_untyped(size))
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_free(pointer: *mut ffi::c_void) {
    let Some(pointer) = NonNull::new(pointer) else {
        return;
    };

    match ALLOCATOR.try_with(|allocator| allocator.borrow_mut().free_untyped(pointer.cast())) {
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

    ALLOCATOR.with_borrow_mut(|allocator| allocator.realloc_untyped(block, size))
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_memalign(size: usize, alignment: usize) -> *mut ffi::c_void {
    let layout = Layout::from_size_align(size, alignment).expect("Invalid size and alignment");
    ALLOCATOR.with_borrow_mut(|allocator| allocator.allocate_untyped(layout.pad_to_align().size()))
}

/// Try to convert a pointer into a persistent offset. Returns false if the pointer was
/// not allocated in this heap.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_pointer_to_offset(
    pointer: *const ffi::c_void,
    offset: *mut u64,
) -> bool {
    match NonNull::new(pointer as *mut ffi::c_void)
        .map(|pointer| ALLOCATOR.with_borrow(|allocator| allocator.pointer_to_offset(pointer)))
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
    ALLOCATOR.with_borrow(|allocator| allocator.offset_to_pointer(offset as usize).as_ptr())
}

#[inline]
fn raw() -> &'static raw::Raw {
    RAW.get()
        .expect("Uninitialized heap: was cxlalloc_init called?")
}
