pub mod barrier;
pub mod benchmark;
pub mod process;

use core::cell::Cell;
use std::time::Instant;

pub use barrier::Barrier;
pub use benchmark::Benchmark;

pub trait Backend: Send + Sync {
    type Allocator: Allocator;
    fn open(name: &str, size: usize) -> Self;
    fn allocator(&self, thread_id: usize) -> Self::Allocator;
}

pub trait Allocator: Sized {
    type Ptr;
    fn allocate(&mut self, size: usize) -> Option<Self::Ptr>;
    unsafe fn deallocate(&mut self, pointer: Self::Ptr);
    unsafe fn pointer_to_offset(&mut self, pointer: Self::Ptr) -> u64;
    fn offset_to_pointer(&mut self, offset: u64) -> Option<Self::Ptr>;
}

pub struct Timer {
    barrier: Barrier,
}

thread_local! {
    static START: Cell<Option<Instant>> = const { Cell::new(None) };
}

impl Timer {
    fn new() -> Self {
        Self {
            barrier: Barrier::open(c"barrier").unwrap(),
        }
    }

    fn start(&self) {
        self.barrier.wait();
        START.set(Some(Instant::now()));
    }

    fn stop(&self, thread_id: usize) {
        let time = START
            .get()
            .map(|start| start.elapsed())
            .unwrap_or_default()
            .as_micros();

        eprintln!("{},{}", thread_id, time);
    }
}
