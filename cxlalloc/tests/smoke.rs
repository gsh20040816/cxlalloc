use cxlalloc::raw;
use cxlalloc::Allocator;

fn with_allocator<F: FnOnce(&mut Allocator)>(apply: F) {
    let raw = raw::Builder::default().build("").unwrap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let mut allocator = raw.allocator(id);
    apply(&mut allocator)
}

#[test]
fn create() {
    let raw = raw::Builder::default().build("").unwrap();
    let _heap = raw.heap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let _allocator = raw.allocator(id);
}
