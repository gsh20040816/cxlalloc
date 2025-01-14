use core::ptr::NonNull;

use cxlalloc::raw;
use cxlalloc::Allocator;

fn with_allocator<F: FnOnce(&mut Allocator)>(apply: F) {
    let _ = env_logger::try_init();
    let raw = raw::Builder::default().build("").unwrap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let mut allocator = raw.allocator(id);
    apply(&mut allocator)
}

#[test]
fn create() {
    with_allocator(|_| ())
}

#[test]
fn allocate_small() {
    with_allocator(|allocator| unsafe {
        let small = allocator
            .allocate_untyped(8)
            .cast::<u64>()
            .as_mut()
            .unwrap();

        *small = 5;
        assert_eq!(*small, 5);

        allocator.free_untyped(NonNull::from(small).cast());
    })
}
