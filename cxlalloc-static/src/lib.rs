#![allow(clippy::missing_safety_doc)]

use core::mem;
use core::ops::Deref;
use core::ptr;
use core::sync::atomic::AtomicIsize;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::alloc::Layout;
use std::cell::Cell;
use std::cell::RefCell;
use std::env;
use std::ffi;
use std::ffi::CStr;
use std::ptr::NonNull;
use std::sync::OnceLock;

use cxlalloc::raw;
use cxlalloc::Allocator;

static RAW: OnceLock<raw::Raw> = OnceLock::new();
static BACKEND: OnceLock<Backend> = OnceLock::new();

fn handle_sigsegv(_: libc::c_int, info: *const libc::siginfo_t, _: *const libc::c_void) {
    let address = unsafe { info.read().si_addr() };
    let id = THREAD_ID.with_borrow(|id| id.as_ref().unwrap().id);
    if raw().map(id, address) {
        return;
    }

    unsafe {
        let mut action = mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

enum Backend {
    Mmap,
    Shm,
    Ivshmem,
}

impl Backend {
    fn parse(name: &str) -> Self {
        match name {
            "mmap" => Backend::Mmap,
            "ivshmem" => Backend::Ivshmem,
            "shm" => Backend::Shm,
            unknown => panic!("Expected one of [mmap, ivshmem, shm], but got {}", unknown),
        }
    }

    fn instantiate(&self) -> raw::Backend {
        let numa = env::var("CXL_NUMA_NODE")
            .ok()
            .and_then(|node| node.parse::<usize>().ok())
            .map(|node| cxlalloc::raw::backend::Numa::Bind { node });

        let builder = raw::Backend::builder().maybe_numa(numa);

        match self {
            Backend::Mmap => builder.kind(raw::backend::Mmap).build(),

            #[cfg(feature = "backend-shm")]
            Backend::Shm => builder.kind(raw::backend::Shm).build(),
            #[cfg(not(feature = "backend-shm"))]
            Backend::Shm => {
                panic!("cxlalloc-static crate was compiled without `backend-shm` feature")
            }

            #[cfg(feature = "backend-ivshmem")]
            Backend::Ivshmem => builder.kind(raw::backend::Ivshmem::new()).build(),
            #[cfg(not(feature = "backend-ivshmem"))]
            Backend::Ivshmem => {
                panic!("cxlalloc-static crate was compiled without `backend-ivshmem` feature")
            }
        }
    }
}

thread_local! {
    // Using a const initializer was causing some linking errors when using clang-15.
    static THREAD_ID: RefCell<Option<Id>> = const { RefCell::new(None) };

    // > Initialization is dynamically performed on the first call to with within a thread...
    //
    // https://doc.rust-lang.org/std/thread/struct.LocalKey.html
    static ALLOCATOR: RefCell<Allocator<'static>> = RefCell::new(raw().allocator(thread_id()));
}

static POOL: AtomicU64 = AtomicU64::new(u64::MAX);

struct Id {
    id: cxlalloc::thread::Id,
    pool: bool,
}

impl Deref for Id {
    type Target = cxlalloc::thread::Id;
    fn deref(&self) -> &Self::Target {
        &self.id
    }
}

impl Drop for Id {
    fn drop(&mut self) {
        cxlalloc::stat::dump(u16::from(self.id) as usize);

        if self.pool {
            POOL.fetch_or(1 << u16::from(self.id), Ordering::AcqRel);
        }
    }
}

/// Override the default backend. Must be called before `cxlalloc_init`.
///
/// Backend string must be one of [mmap, shm, cxl].
/// The `destroy` parameter indicates whether the backing file (if it exists)
/// should be deleted after process exit.
///
/// Note: this is a separate function for backward compatibility.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init_backend(backend: *const ffi::c_char) {
    BACKEND.get_or_init(|| {
        CStr::from_ptr(backend)
            .to_str()
            .map(Backend::parse)
            .expect("Backend name must be valid UTF-8")
    });
}

/// Control the global logger filter at runtime.
///
/// Level string must be one of [off, error, warn, info, debug, trace].
///
/// This function is thread-safe.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_set_log(level: *const ffi::c_char) {
    let level = match CStr::from_ptr(level)
        .to_str()
        .expect("Level must be valid UTF-8")
    {
        "off" => log::LevelFilter::Off,
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        unknown => panic!(
            "Expected one of [off, error, warn, info, debug, trace], but got {}",
            unknown
        ),
    };

    log::set_max_level(level);
}

/// Initialize the global CXL allocator.
///
/// Defaults to the mmap driver if `cxlalloc_init_backend` was not called.
#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init(
    name: *const ffi::c_char,
    size: usize,
    thread_id: u8,
    thread_count: u8,
    process_id: u8,
    process_count: u8,
) {
    cxlalloc_init_thread(thread_id as usize);

    RAW.get_or_init(move || {
        let _ = env_logger::Builder::from_default_env()
            .format(move |buffer, record| {
                use std::io::Write;
                use std::time::Instant;

                use env_logger::fmt::style;

                static START: OnceLock<Instant> = OnceLock::new();

                // Color-coded process ID if there is more than one process
                if process_count > 1 {
                    let process = process_id;
                    let style_process = style::Ansi256Color::from(process).on_default();
                    write!(buffer, "[{style_process}P{process:02}{style_process:#}]")?;
                }

                // Color-coded thread ID if there is more than one thread
                match THREAD_ID.with(|id| u16::from(id.borrow().as_ref().unwrap().id)) {
                    thread if thread_count > 1 => {
                        let style_thread =
                            style::Ansi256Color::from(thread as u8 + 16).on_default();
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

        // Hack for memento + ralloc compatibility
        let name = CStr::from_ptr(name)
            .to_str()
            .unwrap()
            .trim_start_matches("/dev/shm/");

        let mut action = unsafe { mem::zeroed::<libc::sigaction>() };
        action.sa_sigaction = handle_sigsegv as _;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
        unsafe {
            libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
        }

        raw::Raw::builder()
            .backend(BACKEND.get_or_init(|| Backend::Mmap).instantiate())
            .size_small(size)
            .thread_count(thread_count as usize)
            .build(name)
            .unwrap()
    });

    // Eagerly initialize thread-local state to fail fast on buggy recovery.
    ALLOCATOR.with(|_| ());
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_is_clean() -> bool {
    raw().is_clean()
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_init_thread(thread_id: usize) {
    THREAD_ID.set(Some(unsafe {
        if thread_id == 0xFF {
            let mut pool = POOL.load(Ordering::Acquire);
            let id = loop {
                match POOL.compare_exchange(
                    pool,
                    pool & !(1 << pool.trailing_zeros()),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break pool.trailing_zeros(),
                    Err(next) => pool = next,
                }
            };
            Id {
                id: cxlalloc::thread::Id::new(id as u16),
                pool: true,
            }
        } else {
            Id {
                id: cxlalloc::thread::Id::new(thread_id as u16),
                pool: false,
            }
        }
    }));
}

thread_local! {
    static SAVE: Cell<Option<*mut ffi::c_void>> = const { Cell::new(None) };
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_malloc(size: usize) -> *mut ffi::c_void {
    let allocation = ALLOCATOR.with_borrow_mut(|allocator| allocator.allocate_untyped(size));
    SAVE.replace(Some(allocation));
    allocation
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_link(pointer: *mut ffi::c_void) {
    let saved = SAVE.take().expect("Called link without previous malloc");
    let offset = saved as isize - pointer as isize;
    pointer
        .cast::<AtomicIsize>()
        .as_ref()
        .unwrap()
        .store(offset, Ordering::Release);
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
pub unsafe extern "C" fn cxlalloc_unlink(pointer: *mut ffi::c_void) {
    cxlalloc_free(pointer);

    // Boost uses 1 as their null pointer:
    // https://www.boost.org/doc/libs/1_35_0/doc/html/interprocess/offset_ptr.html
    pointer
        .cast::<AtomicIsize>()
        .as_ref()
        .unwrap()
        .store(1, Ordering::Release);
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

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_get_root(index: usize) -> *mut ffi::c_void {
    ALLOCATOR.with_borrow(|allocator| {
        allocator
            .root_untyped(index)
            .map(NonNull::as_ptr)
            .unwrap_or_else(ptr::null_mut)
    })
}

#[no_mangle]
pub unsafe extern "C" fn cxlalloc_set_root(index: usize, pointer: *mut ffi::c_void) {
    ALLOCATOR.with_borrow(|allocator| allocator.set_root_untyped(index, pointer))
}

#[no_mangle]
pub extern "C" fn cxlalloc_close() {}

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

fn raw() -> &'static raw::Raw {
    RAW.get()
        .expect("Uninitialized heap: was cxlalloc_init called?")
}

fn thread_id() -> cxlalloc::thread::Id {
    THREAD_ID.with(|id| {
        id.borrow()
            .as_ref()
            .expect("Uninitialized thread ID: was cxlalloc_init called for this thread?")
            .id
    })
}
