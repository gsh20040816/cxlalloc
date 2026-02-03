use core::cell::Cell;
use core::cell::RefCell;
use core::ffi;
use core::mem;
use core::ptr;
use core::ptr::NonNull;
use std::sync::OnceLock;

use cxlalloc::Allocator;
use cxlalloc::stat::Report;

static RAW: OnceLock<cxlalloc::Raw> = OnceLock::new();

pub use cxlalloc::raw::Raw;
pub use cxlalloc::raw::backend;

thread_local! {
    static THREAD_ID: Cell<u16> = const { Cell::new(0) };
    static ALLOCATOR: RefCell<cxlalloc::Allocator<'static, (), ()>> = RefCell::new(RAW.get().unwrap().allocator(unsafe {
        cxlalloc::thread::Id::new(THREAD_ID.get())
    }));
}

pub fn initialize_process<T: cxlalloc::raw::BuilderState>(
    builder: cxlalloc::raw::Builder<T>,
    name: &str,
) {
    RAW.get_or_init(|| builder.build(name).unwrap());

    let mut action = unsafe { mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = handle_sigsegv as _;
    action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
    unsafe {
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

pub fn initialize_thread(id: u16) {
    THREAD_ID.set(id);
    ALLOCATOR.with(|_| ())
}

#[inline]
pub fn allocate_untyped(size: usize) -> Option<NonNull<ffi::c_void>> {
    NonNull::new(with(|allocator| allocator.allocate_untyped(size)))
}

#[inline]
pub fn deallocate_untyped(pointer: *mut ffi::c_void) {
    let Some(pointer) = NonNull::new(pointer) else {
        return;
    };
    with(|allocator| unsafe { allocator.free_untyped(pointer) })
}

#[inline]
pub fn pointer_to_offset(pointer: NonNull<ffi::c_void>) -> usize {
    with(|allocator| allocator.pointer_to_offset(pointer))
}

#[inline]
pub fn offset_to_pointer(offset: usize) -> NonNull<ffi::c_void> {
    with(|allocator| allocator.offset_to_pointer(offset))
}

#[inline]
pub fn is_clean() -> bool {
    RAW.get().unwrap().is_clean()
}

#[inline]
pub fn set_root_untyped(index: usize, root: *mut ffi::c_void) {
    with(|allocator| allocator.set_root_untyped(index, root))
}

#[inline]
pub fn root_untyped(index: usize) -> Option<NonNull<ffi::c_void>> {
    with(|allocator| allocator.root_untyped(index))
}

pub fn report_process() -> Vec<Report> {
    RAW.get()
        .unwrap()
        .report()
        .filter(|event| event.count > 0)
        .collect::<Vec<_>>()
}

pub fn report_thread() -> Vec<Report> {
    with(|allocator| {
        allocator
            .report()
            .filter(|event| event.count > 0)
            .collect::<Vec<_>>()
    })
}

fn handle_sigsegv(_: libc::c_int, info: *const libc::siginfo_t, _: *const libc::c_void) {
    let address = unsafe { info.read().si_addr() };

    if RAW.get().unwrap().map(
        unsafe { cxlalloc::thread::Id::new(THREAD_ID.get()) },
        address,
    ) {
        return;
    }

    unsafe {
        let mut action = mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

#[inline]
fn with<F: FnOnce(&mut Allocator<'static, (), ()>) -> T, T>(apply: F) -> T {
    ALLOCATOR.with_borrow_mut(|allocator| apply(allocator))
}
