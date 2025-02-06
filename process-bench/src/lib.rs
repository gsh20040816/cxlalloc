pub mod barrier;
pub mod benchmark;
pub mod process;

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

pub struct Timer {}

impl Timer {
    fn start() {}
    fn stop() {}
}
