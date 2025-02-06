pub(crate) mod benchmark;
pub mod process;

pub(crate) use benchmark::Benchmark;

pub trait Backend {
    type Allocator: Allocator + Send;
    fn open(name: &str, size: usize) -> Self;
    fn allocator(&mut self, thread_id: usize) -> Self::Allocator;
}

pub trait Allocator: Sized {
    type Ptr;
    fn allocate(&mut self, size: usize) -> Option<Self::Ptr>;
    unsafe fn deallocate(&mut self, pointer: Self::Ptr);
    unsafe fn pointer_to_offset(&mut self, pointer: Self::Ptr) -> u64;
    fn offset_to_pointer(&mut self, pointer: u64) -> Self::Ptr;
}

pub struct Timer {}

impl Timer {
    fn start() {}
    fn stop() {}
}
