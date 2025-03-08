use core::ffi;
use core::mem;
use core::num::NonZeroU64;
use core::ptr;
use core::ptr::NonNull;
use std::ffi::OsStr;
use std::io;
use std::sync::OnceLock;

pub struct Backend(String);

static RAW: OnceLock<cxlalloc::Raw> = OnceLock::new();

fn handle_sigsegv(_: libc::c_int, info: *const libc::siginfo_t, _: *const libc::c_void) {
    let address = unsafe { info.read().si_addr() };

    if RAW.get().unwrap().map(address) {
        return;
    }

    unsafe {
        let mut action = mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

pub struct Cxlalloc(cxlalloc::Allocator<'static>);

impl allocator_bench::allocator::Backend for Backend {
    type Allocator = Cxlalloc;

    fn open(numa: usize, populate: bool, name: &str, size: usize) -> io::Result<Self> {
        RAW.get_or_init(|| {
            cxlalloc::raw::Builder::default()
                .backend(cxlalloc::raw::backend::Shm {
                    numa: Some(numa),
                    populate,
                })
                .size_small(size / 2)
                .size_large(size / 2)
                .build(name)
                .unwrap()
        });

        let mut action = unsafe { mem::zeroed::<libc::sigaction>() };
        action.sa_sigaction = handle_sigsegv as _;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
        unsafe {
            libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
        }

        Ok(Self(name.to_owned()))
    }

    fn allocator(&self, thread_id: usize) -> Cxlalloc {
        Cxlalloc(
            RAW.get()
                .unwrap()
                .allocator(unsafe { cxlalloc::thread::Id::new(thread_id as u16) }),
        )
    }

    fn unlink(self) -> io::Result<()> {
        for entry in std::fs::read_dir("/dev/shm")? {
            let entry = entry.unwrap();
            let path = entry.path();
            let Some(name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if name.starts_with(&self.0) {
                std::fs::remove_file(path)?;
            }
        }

        Ok(())
    }
}

impl allocator_bench::Allocator for Cxlalloc {
    type Handle = NonNull<ffi::c_void>;

    fn allocate(&mut self, size: usize) -> Option<NonNull<ffi::c_void>> {
        NonNull::new(self.0.allocate_untyped(size))
    }

    unsafe fn deallocate(&mut self, handle: NonNull<ffi::c_void>) {
        self.0.free_untyped(handle)
    }

    unsafe fn handle_to_offset(&mut self, handle: &NonNull<ffi::c_void>) -> NonZeroU64 {
        NonZeroU64::new(self.0.pointer_to_offset(*handle) as u64 + 1).unwrap()
    }

    fn offset_to_handle(&mut self, offset: u64) -> Option<NonNull<ffi::c_void>> {
        Some(self.0.offset_to_pointer(offset as usize - 1))
    }
}
