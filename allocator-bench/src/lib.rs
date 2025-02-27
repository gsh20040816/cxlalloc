pub mod barrier;
pub mod benchmark;
mod index;
pub mod process;

use core::cell::Cell;
use core::ffi;
use core::ptr::NonNull;
use std::io;
use std::time::Instant;

pub use barrier::Barrier;
pub use benchmark::Benchmark;
use serde::Deserialize;
use serde::Serialize;

pub trait Backend: Send + Sync + Sized {
    type Allocator: Allocator;
    fn create(node: usize, name: &str, size: usize) -> io::Result<Self> {
        Self::open(node, name, size)
    }

    fn open(node: usize, name: &str, size: usize) -> io::Result<Self>;

    fn unlink(self) -> io::Result<()>;
    fn allocator(&self, thread_id: usize) -> Self::Allocator;
}

pub trait Allocator: Sized {
    type Ptr: Pointer;
    fn allocate(&mut self, size: usize) -> Option<Self::Ptr>;
    unsafe fn deallocate(&mut self, pointer: Self::Ptr);
    unsafe fn pointer_to_offset(&mut self, pointer: &Self::Ptr) -> u64;
    fn offset_to_pointer(&mut self, offset: u64) -> Option<Self::Ptr>;
}

pub trait Pointer {
    fn as_ptr(&self) -> *mut ffi::c_void;
    fn as_u64(&self) -> u64;
    fn from_u64(pointer: u64) -> Self;
}

impl Pointer for *mut ffi::c_void {
    fn as_ptr(&self) -> *mut ffi::c_void {
        *self
    }

    fn as_u64(&self) -> u64 {
        *self as u64
    }

    fn from_u64(pointer: u64) -> Self {
        pointer as Self
    }
}

impl Pointer for NonNull<ffi::c_void> {
    fn as_ptr(&self) -> *mut ffi::c_void {
        (*self).as_ptr()
    }

    fn as_u64(&self) -> u64 {
        self.as_ptr() as u64
    }

    fn from_u64(pointer: u64) -> Self {
        NonNull::new(pointer as *mut ffi::c_void).unwrap()
    }
}

pub struct Timer {}

#[derive(Deserialize, Serialize)]
pub struct Metrics {
    process_id: usize,
    thread_id: usize,
    time: u128,
}

thread_local! {
    static START: Cell<Option<Instant>> = const { Cell::new(None) };
}

impl Timer {
    fn new() -> Self {
        Self {}
    }

    fn start(&self) {
        START.set(Some(Instant::now()));
    }

    fn stop(&self) -> u128 {
        START
            .get()
            .map(|start| start.elapsed())
            .unwrap_or_default()
            .as_micros()
    }
}
